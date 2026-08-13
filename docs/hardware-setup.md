# Hardware Setup Guide

This is the front-to-back walkthrough for standing up a crowCloud fleet on
real hardware: one VyOS box acting as the fabric's router/route-reflector,
one or more Proxmox VE hosts as compute, and crowCloud itself running in a
small VM that a Proxmox host stands up for you. It starts from bare metal
(building installer images) and ends with a running crowCloud instance and
a repeatable process for adding more hosts later.

It assumes you're comfortable with basic networking (VLANs, static
routing) and have console/IPMI access to the boxes you're installing —
you'll need it, since neither VyOS nor Proxmox VE has a fully hands-off
install mode.

> Every build command below runs on your own workstation (or wherever you
> keep the `crow` CLI), not on the target hardware. You build an image
> locally, flash it, then finish setup by hand over SSH once the box is
> up.

## Table of contents

- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Worked example](#worked-example)
- [Step 0 — Install the crow CLI](#step-0--install-the-crow-cli)
- [Step 1 — Plan and save your fabric config](#step-1--plan-and-save-your-fabric-config)
- [Step 2 — Build and bring up the VyOS route-reflector](#step-2--build-and-bring-up-the-vyos-route-reflector)
- [Step 3 — Build and bring up the first (seed) Proxmox host](#step-3--build-and-bring-up-the-first-seed-proxmox-host)
- [Step 4 — Finish setup in the UI](#step-4--finish-setup-in-the-ui)
- [Step 5 — Add more Proxmox hosts](#step-5--add-more-proxmox-hosts)
- [Fleet secret management](#fleet-secret-management)
- [Troubleshooting](#troubleshooting)
- [Command reference](#command-reference)

## Architecture

```
                    Internet / upstream LAN
                              |
                        [ uplink NIC ]
                       +-------------+
                       |    VyOS     |   route-reflector, BGP EVPN RR,
                       | (route-refl)|   OSPF underlay, NAT + Caddy
                       +-------------+   for ExposedEndpoints
                              |
                        [ trunk NIC ]
                              |
                    ==========================  switch, trunk port
                    ||          ||          ||   carrying underlay +
                 [trunk]     [trunk]     [trunk]  mgmt VLANs, tagged
                    |           |           |
              +-----------+ +-----------+ +-----------+
              | Proxmox 1 | | Proxmox 2 | | Proxmox N |
              | (seed →   | |           | |           |
              |  runs the | |           | |           |
              |  crowCloud| |           | |           |
              |  VM)      | |           | |           |
              +-----------+ +-----------+ +-----------+
```

- **The trunk** carries two tagged VLANs to every box: the **underlay**
  (OSPF + BGP EVPN between VyOS and every Proxmox host's own SDN
  controller — this is what backs `PrivateSubnet`'s VXLAN dataplane) and
  **management** (every host's own admin-plane IP, crowCloud's own
  control-plane traffic, and the gateway/DNS/NAT path out to the
  internet).
- **VyOS** is a single-purpose fabric router: BGP route-reflector for the
  underlay, OSPF speaker, NAT + DNS forwarder for the mgmt VLAN, and — via
  a baked-in Caddy instance — the ingress point for anything you `crow
  expose` later. It needs a second NIC (**uplink**) with real
  internet/LAN reachability; nothing else does. It's also the only path
  from the LAN to crowCloud's own control plane (API + web UI) — those
  live on the mgmt VLAN like everything else, so without a NAT rule on
  VyOS forwarding them, they're reachable only from something already on
  the fabric. Step 2 below bakes that forwarding in.
- **Proxmox hosts** only need the one trunk NIC. They don't reach the
  internet directly — DNS/default route go out through VyOS's mgmt-VLAN
  gateway.
- **crowCloud itself** isn't installed on bare metal. The first Proxmox
  host's post-install hook creates a small VM (the "seed") on itself,
  which stands up a single-node k3s + the crowCloud Helm chart, then
  registers that same physical host back as crowCloud's first provider.
  Every host after that just registers against the crowCloud instance
  already running in that VM.

## Prerequisites

**Per VyOS box:** exactly two NICs with a cable plugged into each — one
to the trunk switch port, one to your uplink. The image-building path
below (§2) detects which is which live at boot by probing for a DHCP
offer/ARP reply on each, so it needs *exactly* two candidates with link
up; a third cabled NIC makes detection refuse to guess.

**Per Proxmox box:** at least one NIC on the trunk port, plus enough
local disk for the OS (capped separately, see `--hdsize-gib` below) and
whatever you'll actually use for VM/storage pools.

**Switch:** a trunk port per box carrying both the underlay and
management VLANs tagged, MTU raised to match your chosen `trunk_mtu`
(9000/jumbo by default).

**On your workstation (wherever you run `crow`):**

| Tool | Needed for | Notes |
|---|---|---|
| `crow` CLI | everything below | see [Step 0](#step-0--install-the-crow-cli) |
| `proxmox-auto-install-assistant` | building the final Proxmox ISO | ships with Proxmox VE / Debian; see [Step 3](#step-3--build-and-bring-up-the-first-seed-proxmox-host) |
| A Proxmox VE installer ISO | building the final Proxmox ISO | download from Proxmox directly — `crow` never fetches this for you |
| Docker (`--privileged` support) | building the custom VyOS image | used to run VyOS's own `vyos-build` toolchain |
| `openssl` | hashing the Proxmox root password locally | almost certainly already present |
| `wireguard-tools` (`wg`) | admin VPN access (optional) | see [Step 2e](#2e-admin-vpn-access-optional) — only needed if you want VyOS to double as an admin VPN endpoint |

You don't need `proxmox-auto-install-assistant` or Docker on the box
you're installing — only on the machine running `crow iso ... build`.
Neither `iso proxmox build` nor `iso vyos build image` needs you to be
logged in to a crowCloud instance; both work from nothing.

## Worked example

Every command below uses one consistent example fabric so the flags line
up end to end. Swap in your own values, but keep the same shape:

| Purpose | Value |
|---|---|
| Underlay VLAN / network | `10` / `10.255.10.0/24` |
| Management VLAN / network | `20` / `10.255.20.0/24` |
| VyOS underlay IP (BGP route-reflector) | `10.255.10.1` |
| VyOS management IP (every host's gateway) | `10.255.20.1` |
| crowCloud control-plane IP (reserved, unused today) | `10.255.20.10` |
| Proxmox host 1 (seed) underlay / mgmt IP | `10.255.10.11` / `10.255.20.11` |
| Proxmox host 2 underlay / mgmt IP | `10.255.10.12` / `10.255.20.12` |
| BGP ASN | `65000` |
| OSPF area | `0` |
| Trunk MTU | `9000` |
| WireGuard VPN network (optional, admin access) | `10.255.30.0/24` |

The `10.255.20.10` row matters more than it looks: crowCloud's API will
end up reachable at `http://10.255.20.10:30081` once it's running (`30081`
is the Helm chart's default API NodePort — see
[`charts/crowcloud/values.yaml`](../charts/crowcloud/values.yaml)), and
every host you ever build — including the seed — gets that same address
baked in as `--crow-api-url`. Pick this IP before you build the seed's
image; the seed VM's own address is derived directly from it.

## Step 0 — Install the crow CLI

Grab a binary or `.deb` from [GitHub
Releases](https://github.com/GavinMce/crowCloud/releases), or build it
from source:

```bash
cargo install --path crates/crow-cli
```

No login or running crowCloud instance is required for anything in this
guide until [Step 4](#step-4--finish-setup-in-the-ui).

## Step 1 — Plan and save your fabric config

Values like the underlay VLAN, BGP ASN, and trunk MTU have to match
exactly across the VyOS box and every Proxmox host. Type them once and
every build command below picks them up automatically (an explicit flag
on any individual command still overrides just that one field):

```bash
crow iso fabric-configure
```

Answer the prompts using the [worked example](#worked-example) table
above (or your own values). This writes `~/.config/crow/config.json`,
which also holds the fleet secret generated in the next step.

## Step 2 — Build and bring up the VyOS route-reflector

### 2a. Render the image inputs

```bash
crow iso vyos build image \
  --hostname vyos-rr \
  --underlay-ip 10.255.10.1 \
  --mgmt-ip 10.255.20.1 \
  --loopback-ip 10.255.0.1 \
  --uplink-dhcp true \
  --ssh-pubkey ~/.ssh/id_ed25519.pub \
  --crow-api-mgmt-ip 10.255.20.10 \
  --crow-api-mgmt-port 30081 \
  --crow-frontend-mgmt-port 30080 \
  --out ./build/vyos
```

`--mgmt-network`/`--mgmt-network-prefix` and the rest of the fabric-wide
fields (underlay VLAN, BGP ASN, trunk MTU, …) aren't listed above — they
fall back to what you saved in Step 1 automatically. In an interactive
terminal you'll still get a prompt for each, pre-filled with that saved
value (press Enter to accept); pass the flag explicitly to skip the
prompt, which matters if you're scripting this non-interactively. The
image is SSH-key-only by construction (`--ssh-pubkey` is required); there
is no baked password.

If your uplink is static instead of DHCP, drop `--uplink-dhcp` and add
`--uplink-ip`/`--uplink-prefix`/`--uplink-gateway` instead.

The last three flags are what makes crowCloud's own API and web UI
reachable from the upstream LAN, not just from the mgmt VLAN — they bake
in two static NAT rules on VyOS's uplink, forwarding `:30081`
(API) and `:30080` (frontend) straight through to
`10.255.20.10`, the control-plane IP reserved in the [worked
example](#worked-example). This only works because that reservation
happened before this build — the rules are baked in now, before
crowCloud even exists yet. Omit all three to keep the control plane
mgmt-VLAN-only (reachable only from something already on the fabric, e.g.
a Proxmox host or a VPN/jump box bridged onto it).

This writes two files to `--out`:

- `crowcloud-fabric-init.sh` — the script that will end up baked into the
  image at `/usr/local/bin/crowcloud-fabric-init.sh`. It detects which
  NIC is the trunk vs. uplink live (no PCI address or interface name
  needed), installs bnx2 NIC firmware and Caddy from what's already on
  the image (no network access needed), and applies the same fabric
  config `iso vyos apply` would push over SSH.
- `crowcloud.toml` — the `vyos-build` flavor file wiring that script (and
  the firmware/Caddy payloads) into a custom ISO.

### 2b. Build the actual ISO

The command above prints the exact steps, which boil down to:

```bash
git clone -b rolling --single-branch https://github.com/vyos/vyos-build
cp ./build/vyos/crowcloud.toml vyos-build/data/build-flavors/crowcloud.toml
cd vyos-build
docker run --rm -it --privileged -v $(pwd):/vyos -w /vyos vyos/vyos-build:rolling \
  sudo ./build-vyos-image --architecture amd64 --build-by crowcloud crowcloud
```

### 2c. Flash and install

Flash the resulting ISO to a USB stick and boot the VyOS box from it.
VyOS has no unattended install mode, so you still walk through one
interactive `install image` session at the console — accept the
defaults, let it write to the box's disk, then remove the USB and
reboot.

### 2d. Bring up the fabric config — by hand

Once the box is back up (it'll have come up with whatever your uplink
config gave it — DHCP or the static address you set), SSH in and run the
baked-in script yourself. Nothing runs it automatically:

```bash
ssh vyos@<box-ip>
sudo bash /usr/local/bin/crowcloud-fabric-init.sh
```

It's baked in with no execute bit (a `vyos-build` limitation, not a
mistake), so invoke it via `bash`, not `./crowcloud-fabric-init.sh`. You'll
see numbered steps as it runs — interface detection, firmware/Caddy
install, then the actual VyOS `configure`/`set`/`commit` session — and a
final summary line once everything's applied. If anything fails partway,
the script prints exactly which step it was on and everything that
completed before it, so you know what to check before re-running it (it's
idempotent — safe to run again).

> **Prefer a simpler, non-image path?** If you'd rather install stock
> VyOS by hand and just push config to it (no Docker, no custom image, no
> live interface detection — you name the interfaces explicitly), use
> `crow iso vyos build config` to render a one-shot `configure.txt`
> instead, then either paste it into a VyOS `configure` session yourself
> or push it over SSH with `crow iso vyos apply --host <box-ip> --ssh-key
> ~/.ssh/id_ed25519 --script ./build/vyos/configure.txt`. Same
> fabric-config content either way, just without the firmware/Caddy/live-
> detection extras `build image` bakes in.

### 2e. Admin VPN access (optional)

Everything on the mgmt VLAN — SSH to any host, the crowCloud API/UI,
`kubectl`/`helm` against the fleet's cluster — is otherwise only reachable
from something already on the fabric. VyOS can double as a WireGuard VPN
endpoint for admins specifically (separate from anything tenant-facing —
this never routes to the internet, only into the fabric's own mgmt and
underlay VLANs), which is the recommended way to get that access instead
of exposing more ports through the uplink NAT rules from Step 2a.

Enable it by adding to Step 2a's `iso vyos build image` command:

```bash
  --wireguard-address 10.255.30.1 \
  --wireguard-address-prefix 24 \
  --wireguard-port 51820 \
```

(`10.255.30.0/24` here is a VPN-only subnet, distinct from underlay/mgmt —
add it to your fabric config in Step 1 via `crow iso fabric-configure`'s
WireGuard prompt so every build picks it up consistently.) This bakes a
WireGuard server onto VyOS with no peers yet — nothing changes about
reachability until you add one.

Add yourself as a peer once the box is up (this pushes the change live
over SSH — no rebuild/reflash needed, and safe to repeat any time you
want to add or rotate an admin):

```bash
crow iso vyos wireguard add-peer yourname \
  --client-address 10.255.30.2 \
  --host <vyos-uplink-ip> \
  --ssh-key ~/.ssh/id_ed25519
```

This generates your WireGuard keypair locally (the private key never
leaves this machine) and prints a ready-to-use client config — save it
and bring the tunnel up:

```bash
sudo wg-quick up ./yourname.conf
```

Once connected, SSH/`kubectl`/`helm`/the crowCloud API all work exactly
as if you were physically on the fabric — see
[`docs/development.md`](development.md) for what that unlocks. Remove a
peer the same way: `crow iso vyos wireguard remove-peer yourname --host
<vyos-uplink-ip> --ssh-key ~/.ssh/id_ed25519`.

## Step 3 — Build and bring up the first (seed) Proxmox host

This is the host that will create crowCloud's own VM for you. Every field
below that also appears in your fabric config falls back to it
automatically if omitted.

### 3a. Render the image inputs

```bash
crow iso proxmox build \
  --fqdn pve1.fleet.local \
  --admin-email you@example.com \
  --trunk-interface eno1 \
  --underlay-ip 10.255.10.11 \
  --mgmt-ip 10.255.20.11 \
  --crow-api-url http://10.255.20.10:30081 \
  --seed-ssh-pubkey ~/.ssh/id_ed25519.pub \
  --vyos-uplink-interface eth2 \
  --vyos-ssh-private-key ~/.ssh/id_ed25519 \
  --base-iso ~/Downloads/proxmox-ve_8.3-1.iso \
  --out ./build/pve1
```

You'll be prompted for the root password interactively (it's hashed
locally with `openssl passwd -6` and never touches disk or logs in
plaintext), plus anything else you left unset. A few notes on fields
above:

- `--crow-api-url` is the reserved control-plane address from the
  [worked example](#worked-example) — get the port right (`30081`, the
  API's NodePort), not `8080` (the container's internal port).
- `--seed-ssh-pubkey` gives you SSH access into the seed VM crowCloud
  will run in — Debian's stock cloud image has no password login and no
  key of its own, so skipping this leaves that VM inaccessible.
- `--vyos-uplink-interface`/`--vyos-ssh-private-key` are optional but
  worth setting on the seed specifically: together they let the
  Kubernetes operator auto-configure its VyOS connection (needed for
  `ExposedEndpoint`) instead of a manual `helm upgrade --set
  operator.vyos.*` afterward. The interface name is VyOS's physical
  uplink NIC (whatever `crowcloud-fabric-init.sh` resolved it to — check
  `ip a` on the VyOS box if unsure), and the private key is the one
  matching the public key you baked into the VyOS image in Step 2.
- Disk selection and `--zfs-raid` aren't shown above — omit both and it
  defaults to "first real disk found, capped at 150 GiB for the OS,
  everything else left free for storage pools" (see `--hdsize-gib`).
  Override with `--disk sda,sdb` or `--disk-filter ID_BUS=ata` if you
  need to target specific hardware.

This writes `answer.toml` (consumed by `proxmox-auto-install-assistant`
for the unattended base install) and `post-install-hook.sh` (the fabric
setup + crowCloud bootstrap script) to `--out`, then — since
`--base-iso` and `proxmox-auto-install-assistant` are both present —
builds `proxmox-auto.iso` in the same directory.

> Omit `--base-iso` (or pass `--render-only`) to just generate
> `answer.toml`/`post-install-hook.sh` without building an ISO — useful
> if you want to run `proxmox-auto-install-assistant` yourself, or don't
> have it installed on this machine.

### 3b. Flash and install

Flash `proxmox-auto.iso` to a USB stick and boot the box from it. Unlike
VyOS, this part *is* unattended — `answer.toml` drives the whole base
install (disk partitioning, network, root password) with no console
interaction. It reboots into the installed system on its own when done.

### 3c. Bring up the fabric config and bootstrap — by hand

The post-install hook isn't bundled into the ISO (Proxmox's
`--on-first-boot` mechanism didn't reliably trigger in practice), so copy
it over yourself once the box is up:

```bash
scp ./build/pve1/post-install-hook.sh root@<box-ip>:/root/
ssh root@<box-ip> bash /root/post-install-hook.sh
```

Like the VyOS script, this reports numbered progress and a final summary
(or, on failure, exactly which step broke and everything that succeeded
first). What it actually does, in order:

1. Installs FRR and brings up the trunk/VLANs at the fabric MTU.
2. Configures OSPF on the underlay so this host can reach VyOS.
3. Checks whether `--crow-api-url` (`http://10.255.20.10:30081`) is
   reachable. On a brand-new fleet, it isn't yet — **this host
   self-elects as the fleet seed**:
   - Fetches Debian 12's cloud image via Proxmox's own download-url API.
   - Creates a VM (`crowcloud-seed`) tagged onto the mgmt VLAN, with a
     serial console (`qm terminal <vmid>` gets you a login if something
     goes wrong).
   - Mints a Proxmox API token for crowCloud to use later.
   - Hands it cloud-init that runs `bootstrap.sh` unattended inside the
     guest — this installs k3s, Helm, CloudNativePG, and the crowCloud
     Helm chart (pulled from GHCR), then registers the *physical* Proxmox
     host you're SSH'd into right now as crowCloud's first provider, and
     inserts your fleet secret into the database so every future host
     using it can self-register with zero manual steps.
   - Assigns that VM the exact IP you reserved (`10.255.20.10`, parsed
     straight out of `--crow-api-url`).
   - Starts it and returns — **it does not wait for crowCloud to finish
     coming up inside the guest.**

### 3d. Watch it come up

The seed VM's own bootstrap takes a few minutes (k3s + Helm + CNPG +
crowCloud). From the Proxmox host:

```bash
qm terminal <vmid>          # console into the seed VM directly, or:
```

Once inside (or via `ssh debian@10.255.20.10` if you set
`--seed-ssh-pubkey`), tail its progress:

```bash
tail -f /var/log/crowcloud-bootstrap.log
```

It finishes by printing crowCloud's URL — the frontend's own NodePort
(`30080` by default), i.e. `http://10.255.20.10:30080`. The address you
baked in as `--crow-api-url` (`:30081`) is the *API*, for the CLI and for
other hosts' self-registration calls — not what you open in a browser.

## Step 4 — Finish setup in the UI

If you baked in `--crow-api-mgmt-ip`/`--crow-api-mgmt-port`/
`--crow-frontend-mgmt-port` in Step 2a, both are also reachable from the
upstream LAN off VyOS's own uplink address (whatever it got via DHCP, or
the static address you gave it) — check with `ip a` on the VyOS box, or
your DHCP server's lease list, if you don't already know it:

```
http://<vyos-uplink-ip>:30080   # web UI
http://<vyos-uplink-ip>:30081   # API
```

Otherwise, you'll need to be on the mgmt VLAN already (e.g. SSH'd into a
Proxmox host) to reach `http://10.255.20.10:30080` directly.

Open whichever URL applies and create the admin account. Nothing else is
required — your first Proxmox host is already registered as a provider,
courtesy of Step 3c.

To use the CLI instead:

```bash
crow login --server http://<vyos-uplink-ip-or-10.255.20.10>:30081
crow provider list
```

## Step 5 — Add more Proxmox hosts

This is the steady-state path for every host after the seed — same build
command, same fabric config, same cached fleet secret
(`crow-cli` reuses whatever it generated on first use automatically), but
now `--crow-api-url` actually resolves to something:

```bash
crow iso proxmox build \
  --fqdn pve2.fleet.local \
  --admin-email you@example.com \
  --trunk-interface eno1 \
  --underlay-ip 10.255.10.12 \
  --mgmt-ip 10.255.20.12 \
  --crow-api-url http://10.255.20.10:30081 \
  --base-iso ~/Downloads/proxmox-ve_8.3-1.iso \
  --out ./build/pve2
```

Flash, install (unattended, same as before), then copy over and run the
hook exactly as in [3c](#3c-bring-up-the-fabric-config-and-bootstrap--by-hand):

```bash
scp ./build/pve2/post-install-hook.sh root@<box-ip>:/root/
ssh root@<box-ip> bash /root/post-install-hook.sh
```

This time `--crow-api-url` *is* reachable, so the hook takes the other
branch: it calls crowCloud's self-registration endpoint directly (no seed
VM, no bootstrap.sh) and gets back one of two instructions, which it
carries out itself —

- **`create`** — no other node is known for this provider yet; runs
  `pvecm create`. (Only happens if you're re-registering after wiping
  `provider_nodes`; in the normal flow this was already done by the
  seed.)
- **`join`** — joins the existing Proxmox cluster via `pvecm add
  <join_host>`, where `join_host` is whichever other node most recently
  registered.

Confirm it landed:

```bash
crow provider list
```

Repeat this step for every additional host. Nothing about Steps 1–2
(fabric config, VyOS) needs to happen again — they're one-time, fleet-wide
setup.

## Fleet secret management

The fleet secret baked into every image is cached locally in
`~/.config/crow/config.json` and reused automatically by every `crow iso
proxmox build` you run from this machine — you don't need to pass
`--fleet-secret` unless you're intentionally using a different one (e.g.
building from a second workstation, or rotating).

To mint an additional secret or revoke one (once you're logged in via
`crow login`, admin account required), use the API directly — there's no
dedicated `crow` subcommand for this yet. `crow login` saves your session
token in `~/.config/crow/config.json`, which the snippets below pull out
with `jq`:

```bash
TOKEN="$(jq -r .token ~/.config/crow/config.json)"

# Mint -- shown once, same as a Proxmox API token
curl -X POST http://10.255.20.10:30081/api/v1/fleet-secrets \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"label": "second-workstation"}'

# Revoke
curl -X DELETE http://10.255.20.10:30081/api/v1/fleet-secrets/<id> \
  -H "Authorization: Bearer $TOKEN"
```

Multiple secrets can be valid at once — useful for rotating: mint a new
one, switch new builds to it, then revoke the old one once every image
still using it has been replaced.

## Updating crowCloud after setup

Everything above brings up a specific released version. For the ongoing
development cycle afterward — shipping a real release vs. iterating
against this fleet directly (e.g. testing an operator change without
cutting a release each time) — see
[`docs/development.md`](development.md).

## Troubleshooting

- **VyOS interface detection refuses to run ("Expected exactly 2 cabled
  interfaces...")** — something else has link up besides the trunk and
  uplink (a third NIC, a management/IPMI port sharing the same bank).
  Unplug it or use `crow iso vyos build config` instead, which takes
  explicit interface names.
- **`crowcloud-fabric-init.sh: Permission denied`** — it's baked in with
  no execute bit by design (see Step 2d). Run it as `bash
  crowcloud-fabric-init.sh`, not `./crowcloud-fabric-init.sh`.
- **A hook script fails partway through** — both scripts print exactly
  which numbered step failed and everything that completed before it.
  Fix the underlying issue and re-run the same command; every step in
  both scripts is written to be safe to repeat.
- **Seed VM never reaches the target IP** — `crow-api-url`'s host must be
  an IPv4 literal, not a DNS name (there's no DNS server for the guest to
  resolve against yet at that point in bootstrap); the post-install hook
  checks this and refuses to continue otherwise.
- **A later host gets `pvecm add` pointed at a dead node** — `join_host`
  is "most recently registered other node", not a liveness check (no
  heartbeat tracking exists yet). If that node is actually down, join
  against a known-good one manually instead:
  `pvecm add <healthy-node-ip>`.
- **Want a persistent log instead of watching the hook run live?** —
  neither script writes one on its own; both are meant to be watched
  interactively over SSH. Pipe it yourself if you want a copy: `bash
  post-install-hook.sh 2>&1 | tee post-install.log`.

## Command reference

| Command | Purpose |
|---|---|
| `crow iso fabric-configure` | Save fabric-wide values (VLANs, ASN, MTU, …) reused by every build below |
| `crow iso vyos build image` | Render a self-configuring custom VyOS image (firmware + Caddy + fabric-init baked in) |
| `crow iso vyos build config` | Render a one-shot `configure.txt` for an already-installed VyOS box |
| `crow iso vyos apply` | Push a rendered `configure.txt` to a live VyOS box over SSH |
| `crow iso proxmox build` | Render `answer.toml` + the post-install hook, and build the final ISO if `--base-iso` is given |
| `crow provider list` | Confirm a host registered successfully |

Run any command with `--help` for its full flag list, or with no flags at
all for an interactive walkthrough.
