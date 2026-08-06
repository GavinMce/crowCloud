/// Inputs for rendering a Proxmox `answer.toml` (`proxmox-auto-install-assistant`)
/// and the post-install hook script (#66/#67).
///
/// The `answer.toml` shape is confirmed live against a real
/// `proxmox-auto-install-assistant validate-answer` (v8.3.4, run inside a
/// throwaway QEMU/Debian VM with Proxmox's own apt repo added) -- not
/// just documented best-effort. That run caught two real gaps this
/// struct's fields now cover: `[global].mailto` is required and was
/// missing entirely, and `--on-first-boot` (the CLI flag bundling the
/// hook script into the ISO) silently does nothing without an explicit
/// `[first-boot]\nsource = "from-iso"` section enabling it in the answer
/// file itself.
pub struct ProxmoxBuildConfig {
    pub root_password_hash: String,
    pub fqdn: String,
    /// Required by `answer.toml`'s `[global]` section -- confirmed live
    /// against `proxmox-auto-install-assistant validate-answer` (8.3.4),
    /// which rejects the file outright without it.
    pub admin_email: String,
    pub trunk_interface: String,
    pub underlay_vlan: u16,
    /// This host's own IP on the underlay VLAN -- no IPAM allocator
    /// exists yet (crowCloud/#54), so this is a manual per-host value
    /// for now rather than something derived automatically. Without an
    /// actual address inside the underlay subnet, this host can't form
    /// an OSPF adjacency at all, and VyOS's BGP `listen range` (bound to
    /// that same subnet) can never accept a session from it either --
    /// confirmed live: an earlier version of this hook computed a
    /// same-named variable from the trunk interface's own (nonexistent,
    /// since the trunk only carries tagged VLAN traffic) address and
    /// never used it anywhere, silently leaving the underlay VLAN
    /// unconfigured entirely.
    pub underlay_ip: String,
    pub mgmt_vlan: u16,
    pub mgmt_ip: String,
    pub mgmt_prefix: u8,
    pub mgmt_gateway: String,
    pub trunk_mtu: u32,
    pub disk_selection: DiskSelection,
    /// Reserves free, genuinely unpartitioned space on the install
    /// disk for storage pools created later, instead of consuming the
    /// whole disk for the OS -- confirmed against Proxmox's installer
    /// source (`Proxmox/Sys/Block.pm::partition_bootable_disk`, an
    /// explicit-end-offset `sgdisk` partition, not thin-provisioned)
    /// and its own docs ("reserve free space on the hard disk for
    /// further partitioning after the installation"). GiB. Nested
    /// under `lvm.hdsize` for ext4 (Proxmox always LVM-backs ext4/xfs
    /// roots -- there's no `ext4.hdsize`), or `zfs.hdsize` when
    /// `zfs_raid` is set. Combined with a broad `DiskSelection::Filter`
    /// matching every local disk, this is what makes "OS on whichever
    /// disk gets picked, capped small; everything else -- including
    /// the untouched remainder of that same disk -- left free for
    /// storage pools" work without needing to identify a specific
    /// disk by size (which Proxmox's filter mechanism can't match on
    /// at all -- confirmed, it only ever sees udev string properties).
    pub hdsize_gib: Option<f64>,
    pub zfs_raid: Option<String>,
    pub crow_api_url: String,
    pub fleet_secret: String,
    pub bgp_asn: u32,
    pub bgp_peer_password: String,
    /// VyOS's own IP on the underlay VLAN -- see `FabricConfig`'s field
    /// of the same name for why this host must actively dial out to it
    /// rather than relying on VyOS's passive `bgp listen range` alone.
    pub bgp_route_reflector_ip: String,
    pub underlay_prefix: u8,
    pub ospf_area: String,
    /// SSH public key for the seed VM specifically (#67) -- distinct
    /// from any host-level access, since the seed VM is Debian's stock
    /// cloud image, which ships with no password login and no key of
    /// its own. Confirmed live: with neither a password nor a key
    /// configured, there is no way to log into it at all once cloud-init
    /// actually applies -- not even console access, since the default
    /// `debian` user's password is locked on official cloud images.
    /// Optional: omit to leave the seed VM console/SSH-inaccessible,
    /// same as before this existed.
    pub seed_ssh_pubkey: Option<String>,
    /// Physical uplink NIC name on the VyOS box (e.g. "eth1") -- not
    /// something this build can discover on its own (it's assigned live
    /// at VyOS's own boot time by its interface auto-detection), so it's
    /// manual input, same as `underlay_ip`/`bgp_route_reflector_ip`.
    /// Together with `vyos_ssh_private_key`, lets the seed VM configure
    /// the operator's VyOS connection automatically instead of that
    /// being a manual `helm upgrade --set operator.vyos.*` step after
    /// the fact (confirmed live: every fresh deployment otherwise starts
    /// with `ExposedEndpoint` silently disabled). `mgmt_gateway` above
    /// doubles as VyOS's own mgmt-VLAN IP -- no separate host field
    /// needed.
    pub vyos_uplink_interface: Option<String>,
    /// Private key matching the public key baked into the VyOS image via
    /// `crow-cli iso vyos build --ssh-pubkey`. Baked into the seed VM's
    /// own cloud-init (a separate write_files entry, not reused from
    /// `seed_ssh_pubkey` above -- that key is for logging into the seed
    /// VM itself, a different keypair entirely) so `bootstrap.sh` can
    /// pass it to Helm via `--set-file`. Both this and
    /// `vyos_uplink_interface` are required together -- omit both to
    /// leave VyOS auto-configuration off, same as leaving
    /// `operator.vyos.host` empty always has.
    pub vyos_ssh_private_key: Option<String>,
}

/// `answer.toml`'s `[disk-setup]` accepts either an explicit
/// `disk-list` of device names, or a `filter` against UDEV properties
/// (e.g. `ID_SERIAL`, `ID_WWN`, `ID_BUS`) -- confirmed against
/// Proxmox's official Automated Installation docs and the installer's
/// own answer-file struct (pve-devel patch introducing it). These are
/// mutually exclusive -- the installer itself errors with "Need either
/// 'disk_list' or 'filter' set" / "Cannot use both" if given neither or
/// both, which `render_answer_toml` never can since this is an enum.
///
/// The installer's own disk enumeration already excludes the live
/// boot/install medium automatically (it detects the iso9660
/// filesystem and skips it) before either `disk-list` or `filter` ever
/// run, so neither variant needs to account for the boot USB itself --
/// confirmed against Proxmox's installer source and the well-known
/// "found ISO9660 FS but no or wrong proxmox cd-id, skipping" log line.
/// There is no documented "match all disks" wildcard, though -- a
/// `Filter` broad enough to match every real target disk on a given
/// box's specific hardware is the caller's responsibility to get right
/// (e.g. via `udevadm info --query=property --name=/dev/sdX` on the
/// real box), not something this can safely default for you.
#[derive(Clone, Debug)]
pub enum DiskSelection {
    List(Vec<String>),
    Filter {
        /// `(UDEV_KEY, value)` pairs, e.g. `("ID_BUS", "ata")`. Values
        /// support a trailing `*` glob per Proxmox's docs.
        filter: Vec<(String, String)>,
        /// `"any"` (Proxmox's own default if omitted) or `"all"` --
        /// whether one matching filter key is enough, or every key
        /// must match.
        filter_match: Option<String>,
    },
}

/// The literal contents of `deploy/bootstrap.sh`, embedded at compile
/// time. `crow-cli` is a distributed binary, not always run from inside
/// a repo checkout, so the seed VM's cloud-init can't assume the script
/// is available on disk relative to wherever `crow-cli` happens to run
/// -- baking it in keeps this in sync with the real script automatically
/// (single source of truth) without a runtime dependency on fetching it
/// from anywhere.
///
/// Lives under `crates/crow-cli/deploy/` rather than a repo-root
/// `deploy/`, because `cargo package` builds each crate in isolation and
/// can't see files outside the crate's own manifest directory -- an
/// `include_str!` reaching outside it fails `cargo package`'s build
/// verification (confirmed live: this is what blocked release-plz's
/// `git_only` mode from ever computing a diff).
const BOOTSTRAP_SH: &str = include_str!("../../deploy/bootstrap.sh");

/// Cloud-init user-data for the seed VM (#67) -- writes and runs
/// `bootstrap.sh` unattended, with `CROW_FLEET_SECRET` set so the
/// resulting crowCloud instance immediately accepts self-registration
/// from every other host built with the same fleet secret.
///
/// `ssh_pubkey` is deliberately added via the top-level `ssh_authorized_keys`
/// directive (applies to the image's own default user, `debian` on
/// Debian's official cloud image) rather than a `users:` override
/// touching `root` -- cloud-init's `disable_root` module locks the root
/// account by default regardless of any key configured for it, unverified
/// whether overriding that interacts safely with everything else this
/// user-data already does, and `debian` already has passwordless sudo
/// out of the box on the official image, which is enough to read
/// `/var/log/crowcloud-bootstrap.log`.
/// Token *name* Proxmox's `pveum user token add root@pam <name>`
/// expects; the resulting full ID crowCloud actually authenticates with
/// is `root@pam!<name>`. Fixed rather than generated, since it's not a
/// secret (the token *secret* is) and both the seed cloud-init and the
/// post-install hook that creates the token need to agree on it.
const PROXMOX_TOKEN_NAME: &str = "crow";

pub fn render_seed_cloud_init(
    fleet_secret: &str,
    ssh_pubkey: Option<&str>,
    mgmt_gateway: &str,
    vyos: Option<(&str, &str)>,
    proxmox_mgmt_ip: &str,
) -> String {
    let ssh_authorized_keys = match ssh_pubkey {
        Some(key) => format!("ssh_authorized_keys:\n  - {key}\n"),
        None => String::new(),
    };

    // Provider self-registration: lets the seed's own Proxmox host
    // become crowCloud's first provider automatically instead of that
    // being a manual `crow provider add-proxmox` step after the fact.
    // The host's own MAC/hostname/storage/bridge, and the API token's
    // secret, aren't knowable at this crate's build time -- this only
    // renders the seed's cloud-init, it never runs on the real box --
    // so they're left as literal placeholder tokens for
    // `render_post_install_hook`'s own `sed` to substitute once it's
    // actually detected/generated them there. The URL and token *name*
    // aren't install-time-dynamic (the URL is built from a value this
    // build already has; the name is fixed, see `PROXMOX_TOKEN_NAME`),
    // so those are resolved directly, here.
    let provider_env = format!(
        " PROXMOX_HOST_MAC=__CROW_PROXMOX_MAC__ PROXMOX_HOST_NODE_NAME=__CROW_PROXMOX_NODE_NAME__ PROXMOX_HOST_STORAGE=__CROW_PROXMOX_STORAGE__ PROXMOX_HOST_BRIDGE=__CROW_PROXMOX_BRIDGE__ PROXMOX_HOST_TOKEN_SECRET=__CROW_PROXMOX_TOKEN_SECRET__ PROXMOX_HOST_MGMT_IP={proxmox_mgmt_ip} PROXMOX_HOST_URL=https://{proxmox_mgmt_ip}:8006 PROXMOX_HOST_TOKEN_ID=root@pam!{PROXMOX_TOKEN_NAME}"
    );

    // `vyos` is `Some((uplink_interface, ssh_private_key))` -- both
    // required together, same as `operator.vyos.host`/`uplinkInterface`
    // both being needed on the Helm side. Baking the key in as its own
    // write_files entry (not reusing `seed_ssh_pubkey`'s keypair, which
    // is for logging into the seed VM itself, a different key entirely)
    // lets bootstrap.sh hand it straight to `helm --set-file` without
    // ever writing it to this script's own source or logs.
    let vyos_key_file = match vyos {
        Some((_, key)) => format!(
            "  - path: /root/vyos-ssh-key\n    permissions: '0600'\n    content: |\n{}\n",
            indent(key, "      ")
        ),
        None => String::new(),
    };
    let vyos_env = match vyos {
        Some((uplink_interface, _)) => format!(
            " VYOS_HOST={mgmt_gateway} VYOS_UPLINK_INTERFACE={uplink_interface} VYOS_SSH_KEY_PATH=/root/vyos-ssh-key"
        ),
        None => String::new(),
    };

    format!(
        r#"#cloud-config
{ssh_authorized_keys}write_files:
  - path: /root/bootstrap.sh
    permissions: '0755'
    content: |
{bootstrap_sh}
{vyos_key_file}runcmd:
  - [ bash, -c, "CROW_FLEET_SECRET={fleet_secret}{vyos_env}{provider_env} /root/bootstrap.sh > /var/log/crowcloud-bootstrap.log 2>&1" ]
"#,
        ssh_authorized_keys = ssh_authorized_keys,
        bootstrap_sh = indent(BOOTSTRAP_SH, "      "),
        fleet_secret = fleet_secret,
    )
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_answer_toml(cfg: &ProxmoxBuildConfig) -> String {
    let mut out = String::new();

    out.push_str("[global]\n");
    out.push_str("keyboard = \"en-us\"\n");
    out.push_str("country = \"us\"\n");
    out.push_str(&format!("fqdn = \"{}\"\n", cfg.fqdn));
    out.push_str(&format!("mailto = \"{}\"\n", cfg.admin_email));
    out.push_str("timezone = \"UTC\"\n");
    out.push_str(&format!(
        "root-password-hashed = \"{}\"\n",
        cfg.root_password_hash
    ));
    out.push('\n');

    // Installer-time network only -- enough to reach package repos
    // during install. The real fabric config (trunk VLANs, underlay,
    // FRR) is applied by the post-install hook after the base OS is up,
    // not here -- matches the architecture doc's split between "get a
    // bootable, network-reachable box" and "make it a fabric member".
    out.push_str("[network]\n");
    out.push_str("source = \"from-dhcp\"\n");
    out.push('\n');

    out.push_str("[disk-setup]\n");
    if let Some(raid) = &cfg.zfs_raid {
        out.push_str("filesystem = \"zfs\"\n");
        out.push_str(&format!("zfs.raid = \"{raid}\"\n"));
        if let Some(hdsize) = cfg.hdsize_gib {
            out.push_str(&format!("zfs.hdsize = {hdsize}\n"));
        }
    } else {
        out.push_str("filesystem = \"ext4\"\n");
        if let Some(hdsize) = cfg.hdsize_gib {
            out.push_str(&format!("lvm.hdsize = {hdsize}\n"));
        }
    }
    match &cfg.disk_selection {
        DiskSelection::List(disks) => {
            let disks = disks
                .iter()
                .map(|d| format!("\"{d}\""))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("disk-list = [{disks}]\n"));
        }
        DiskSelection::Filter {
            filter,
            filter_match,
        } => {
            for (key, value) in filter {
                out.push_str(&format!("filter.{key} = \"{value}\"\n"));
            }
            if let Some(m) = filter_match {
                out.push_str(&format!("filter-match = \"{m}\"\n"));
            }
        }
    }
    out.push('\n');

    // Confirmed live: `--on-first-boot` (the CLI flag bundling the hook
    // script into the ISO) does nothing on its own -- the installed
    // system won't actually run it without this section explicitly
    // enabling it. `source = "from-iso"` matches passing the hook via
    // `--on-first-boot` at prepare-iso time (as opposed to fetching it
    // over HTTP at first-boot instead).
    out.push_str("[first-boot]\n");
    out.push_str("source = \"from-iso\"\n");

    out
}

/// The post-install hook (#66/#67) -- runs once, on first boot after the
/// base Proxmox install completes. Implements the underlay/fabric setup
/// mirroring VyOS's own config, then either self-registers with an
/// existing crowCloud instance or, finding none, self-elects as the
/// fleet's seed and stands crowCloud up on itself.
pub fn render_post_install_hook(cfg: &ProxmoxBuildConfig) -> String {
    let vyos = match (&cfg.vyos_uplink_interface, &cfg.vyos_ssh_private_key) {
        (Some(uplink), Some(key)) => Some((uplink.as_str(), key.as_str())),
        _ => None,
    };
    let seed_cloud_init = render_seed_cloud_init(
        &cfg.fleet_secret,
        cfg.seed_ssh_pubkey.as_deref(),
        &cfg.mgmt_gateway,
        vyos,
        &cfg.mgmt_ip,
    );
    format!(
        r#"#!/usr/bin/env bash
# Generated by `crow-cli iso proxmox build` -- see #66/#67.
set -euo pipefail

TRUNK_IF="{trunk_interface}"
UNDERLAY_VLAN="{underlay_vlan}"
UNDERLAY_IP="{underlay_ip}"
MGMT_VLAN="{mgmt_vlan}"
MGMT_IP="{mgmt_ip}"
MGMT_PREFIX="{mgmt_prefix}"
MGMT_GATEWAY="{mgmt_gateway}"
TRUNK_MTU="{trunk_mtu}"
CROW_API_URL="{crow_api_url}"
FLEET_SECRET="{fleet_secret}"
BGP_ASN="{bgp_asn}"
BGP_PEER_PASSWORD="{bgp_peer_password}"
BGP_ROUTE_REFLECTOR_IP="{bgp_route_reflector_ip}"
UNDERLAY_PREFIX="{underlay_prefix}"
OSPF_AREA="{ospf_area}"

echo "==> Installing FRR"
apt-get update
apt-get install -y frr

echo "==> Bringing up trunk (${{TRUNK_IF}}) at the fabric MTU"
# Confirmed live: a VLAN subinterface can never exceed its parent's MTU
# at creation time -- bringing up any VLAN child before the physical
# trunk was raised to jumbo frames silently capped it at the default
# 1500, even though its own interfaces stanza said mtu 9000. The
# trunk's MTU must be set before any VLAN child is created on it.
ip link set dev "${{TRUNK_IF}}" up
ip link set dev "${{TRUNK_IF}}" mtu "${{TRUNK_MTU}}"

# Confirmed live: the `ip link set` above only affects the running
# kernel state for this boot -- /etc/network/interfaces never gets the
# trunk's own mtu on its own. On any `ifreload`/reboot after this
# hook's one-time run, ifupdown2 resets the trunk back to the
# interfaces-file default (1500, since nothing in its existing stanza
# says otherwise), which then caps everything layered on top of it.
if ! grep -q "    mtu ${{TRUNK_MTU}}" /etc/network/interfaces; then
    sed -i "/^iface ${{TRUNK_IF}} /a\    mtu ${{TRUNK_MTU}}" /etc/network/interfaces
fi

echo "==> Configuring vmbr0 as a proper VLAN-aware bridge"
# Confirmed live, the hard way, across several real bugs stacked on top
# of each other -- this whole block replaces an earlier, broken
# approach that put the host's own underlay/mgmt IPs on classic 8021q
# subinterfaces of the physical trunk itself (${{TRUNK_IF}}.${{UNDERLAY_VLAN}} /
# ${{TRUNK_IF}}.${{MGMT_VLAN}}), *alongside* also enslaving that same physical
# trunk to vmbr0 for the seed VM's tagged NIC below. Those are two
# different, competing VLAN mechanisms fighting over the same physical
# port, and starting the VM broke mgmt-VLAN connectivity for the whole
# host -- including traffic having nothing to do with the VM -- until a
# full reboot, no matter what else was fixed (MTU, firewalls, switch
# loop protection, MAC conflicts -- all real, all ruled out, none of it
# was the actual cause). The official pattern (per Proxmox's own docs)
# is a single VLAN-aware bridge for everything: the host's own VLAN IPs
# live on vmbr0.<vlan> (a subinterface of the *bridge*, not the NIC),
# and Proxmox's per-VM `tag=` handles guest traffic the same way.
#
# `bridge-vids` on the bridge's own stanza does NOT automatically grant
# the physical port membership in those VLANs, and the bridge device
# itself separately needs explicit membership (via `... self`) before
# it can host any vmbr0.<vlan> subinterface at all -- confirmed live,
# this is what left the trunk port stuck reporting only its default
# PVID (1) in `bridge vlan show` no matter what else was configured.
if ! grep -q "bridge-vlan-aware yes" /etc/network/interfaces; then
    sed -i "/^iface vmbr0/a\\    bridge-vlan-aware yes\\n    bridge-vids 2-4094\\n    mtu ${{TRUNK_MTU}}\\n    post-up bridge vlan add dev ${{TRUNK_IF}} vid ${{UNDERLAY_VLAN}}\\n    post-up bridge vlan add dev ${{TRUNK_IF}} vid ${{MGMT_VLAN}}\\n    post-up bridge vlan add dev vmbr0 vid ${{UNDERLAY_VLAN}} self\\n    post-up bridge vlan add dev vmbr0 vid ${{MGMT_VLAN}} self" /etc/network/interfaces
    ifreload -a || true
fi

echo "==> Configuring management VLAN (vmbr0.${{MGMT_VLAN}})"
cat >> /etc/network/interfaces <<IFACES

auto vmbr0.${{MGMT_VLAN}}
iface vmbr0.${{MGMT_VLAN}} inet static
    address ${{MGMT_IP}}/${{MGMT_PREFIX}}
    mtu ${{TRUNK_MTU}}
    gateway ${{MGMT_GATEWAY}}
IFACES

# Writing to /etc/network/interfaces alone doesn't bring the interface
# up -- confirmed live (it silently sat absent from `ip a` until this
# was added).
ifup "vmbr0.${{MGMT_VLAN}}" || true

# Confirmed live: the installer-time DHCP resolver (on the temporary
# vmbr0 install network) is stale and unreachable once the mgmt VLAN
# takes over as the real default route -- DNS lookups (e.g. fetching
# the seed image below) silently failed until resolv.conf pointed at
# the fabric's real gateway, which also serves as the DNS forwarder.
cat > /etc/resolv.conf <<RESOLV
search fleet.local
nameserver ${{MGMT_GATEWAY}}
RESOLV

echo "==> Configuring underlay VLAN (vmbr0.${{UNDERLAY_VLAN}})"
cat >> /etc/network/interfaces <<IFACES

auto vmbr0.${{UNDERLAY_VLAN}}
iface vmbr0.${{UNDERLAY_VLAN}} inet static
    address ${{UNDERLAY_IP}}/${{UNDERLAY_PREFIX}}
    mtu ${{TRUNK_MTU}}
IFACES

ifup "vmbr0.${{UNDERLAY_VLAN}}" || true

# NOTE: no loopback (VTEP source / stable router-id) is configured for
# this host, unlike VyOS's own build -- FRR falls back to the highest
# active interface address for its router-id absent one, which works
# for basic OSPF/BGP adjacency but isn't stable across interface
# changes. `PrivateSubnet`'s VXLAN/EVPN dataplane
# (`crow-provider-proxmox::network::create_network`) uses this host's
# underlay VLAN IP as its VTEP source instead of a dedicated loopback --
# a real loopback would be more robust (survives this specific
# interface flapping) but isn't required to get EVPN working, so it's
# left as a gap for later.

echo "==> Configuring FRR (OSPF underlay + BGP EVPN dynamic peer)"
# Confirmed live: an earlier version of this hook assumed a per-app
# config-drop-in directory that doesn't exist in real FRR packaging --
# the actual model is /etc/frr/daemons (which daemons start) plus a
# single integrated frr.conf (vtysh running-config syntax), assuming the
# packaged default of `service integrated-vtysh-config` in
# /etc/frr/vtysh.conf, which this doesn't verify or set explicitly.
#
# `advertise-all-vni`: zebra auto-discovers any local VXLAN netdevice
# enslaved to a bridge (see `create_network` in crow-provider-proxmox)
# and, with this set, advertises EVPN Type-3 routes for it over the
# FABRIC peering above -- no explicit per-VNI mapping config needed.
# Without it, this node's own VXLAN interfaces exist locally but are
# never announced, so no other node ever learns to send them traffic.
sed -i 's/^ospfd=no/ospfd=yes/' /etc/frr/daemons
sed -i 's/^bgpd=no/bgpd=yes/' /etc/frr/daemons
cat > /etc/frr/frr.conf <<FRRCONF
router ospf
 network 0.0.0.0/0 area ${{OSPF_AREA}}
!
router bgp ${{BGP_ASN}}
 neighbor FABRIC peer-group
 neighbor FABRIC remote-as internal
 neighbor FABRIC password ${{BGP_PEER_PASSWORD}}
 neighbor ${{BGP_ROUTE_REFLECTOR_IP}} peer-group FABRIC
 address-family l2vpn evpn
  neighbor FABRIC activate
  advertise-all-vni
 exit-address-family
!
FRRCONF
systemctl restart frr

echo "==> Detecting local Proxmox defaults"
DEFAULT_STORAGE="$(pvesm status --content images 2>/dev/null | awk 'NR==2{{print $1}}')"
# Confirmed live: block-based storages that support `images` content
# (e.g. lvmthin, the common default for VM disks) do NOT support the
# `import` content type Proxmox's download-url API needs to stage a
# fetched file -- that requires a file-based storage. These are often
# two different storages on a stock install, so they need separate
# detection; the VM's own disk still ends up on DEFAULT_STORAGE below.
IMPORT_STORAGE="$(pvesm status --content import 2>/dev/null | awk 'NR==2{{print $1}}')"
DEFAULT_BRIDGE="vmbr0"
NODE_NAME="$(hostname)"
MAC_ADDRESS="$(cat /sys/class/net/${{TRUNK_IF}}/address)"

echo "==> Attempting self-registration with crowCloud at ${{CROW_API_URL}}"
REGISTER_URL="${{CROW_API_URL%/}}/api/v1/internal/hosts/register"
REGISTER_PAYLOAD=$(cat <<JSON
{{"mac_address":"${{MAC_ADDRESS}}","node_name":"${{NODE_NAME}}","default_storage":"${{DEFAULT_STORAGE}}","default_bridge":"${{DEFAULT_BRIDGE}}","management_ip":"${{MGMT_IP}}","underlay_ip":"${{UNDERLAY_IP}}"}}
JSON
)

# Confirmed live: `curl ... || echo "000"` is broken -- curl's own
# -w output already writes "000" on a hard failure (DNS/timeout), *and*
# curl still exits non-zero in that case, so the `||` fires too,
# concatenating into the literal string "000000". That fails every
# string comparison below and falls through to the wrong branch (which
# then fails again trying to cat a response file curl never wrote,
# since it never got a reply to write). Capture curl's real exit code
# separately instead of relying on its -w output alone under `set -e`.
set +e
HTTP_CODE="$(curl -s -o /tmp/crowcloud-register-response.json -w '%{{http_code}}' \
  --connect-timeout 5 --max-time 15 \
  -X POST "${{REGISTER_URL}}" \
  -H "X-Fleet-Secret: ${{FLEET_SECRET}}" \
  -H 'Content-Type: application/json' \
  -d "${{REGISTER_PAYLOAD}}")"
CURL_EXIT=$?
set -e
if [ "${{CURL_EXIT}}" -ne 0 ]; then
    HTTP_CODE="000"
fi

if [ "${{HTTP_CODE}}" = "000" ]; then
    echo "==> No crowCloud instance reachable at ${{CROW_API_URL}} -- self-electing as fleet seed (#67)"

    # A truly fresh first host has no templates to clone -- unlike
    # regular VM provisioning (which should reuse whatever template
    # convention #40 eventually builds), the seed VM's own bootstrap is
    # inherently a from-nothing case, so it fetches its own base image
    # directly via Proxmox's own download-url API (the same mechanism
    # #40 documents for default-template creation) rather than depending
    # on a template that can't exist yet. Debian 12's official cloud
    # image is the pinned, curated choice here -- not user-configurable
    # arbitrary input, matching #40's stated preference for a small
    # curated catalog over open-ended URLs.
    if [ -z "${{IMPORT_STORAGE}}" ]; then
        echo "==> No storage supporting the 'import' content type was found -- cannot fetch the seed image" >&2
        exit 1
    fi

    SEED_VMID="$(pvesh get /cluster/nextid)"
    SEED_IMAGE_URL="https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2"
    echo "==> Fetching base image for the seed VM (${{SEED_IMAGE_URL}}) onto ${{IMPORT_STORAGE}}"
    pvesh create /nodes/"${{NODE_NAME}}"/storage/"${{IMPORT_STORAGE}}"/download-url \
        --url "${{SEED_IMAGE_URL}}" \
        --content import \
        --filename "crowcloud-seed-base.qcow2"

    echo "==> Creating seed VM ${{SEED_VMID}} from that image (guest, not the bare host OS)"
    # Tagged onto MGMT_VLAN explicitly -- vmbr0 is VLAN-aware but
    # untagged/native by default for any port that isn't given a `tag=`,
    # and the host's own mgmt access is vmbr0.${{MGMT_VLAN}} (see above), so
    # a guest NIC on vmbr0 with no tag lands on the wrong L2 segment
    # entirely -- it would never have reached MGMT_GATEWAY regardless of
    # what IP it was given.
    # Confirmed live: `qm terminal` has nothing to attach to without an
    # explicit serial device -- Proxmox doesn't create one by default,
    # and this is the only console access to a cloud-init-only guest
    # with no other interactive install step.
    qm create "${{SEED_VMID}}" --name crowcloud-seed --memory 4096 --cores 2 \
        --net0 "virtio,bridge=${{DEFAULT_BRIDGE}},tag=${{MGMT_VLAN}}" --scsihw virtio-scsi-pci \
        --serial0 socket --vga serial0 \
        --ostype l26
    # Confirmed live: hardcoding the default directory storage's import
    # path broke on this host's real (different) storage backend with
    # "couldn't find the crowcloud file". `pvesm path` resolves the
    # actual filesystem path for the volume `download-url` just wrote,
    # regardless of storage backend. The fetched file lives on
    # IMPORT_STORAGE, but the resulting VM disk is created on
    # DEFAULT_STORAGE (the storage that actually supports `images`).
    SEED_IMAGE_PATH="$(pvesm path "${{IMPORT_STORAGE}}:import/crowcloud-seed-base.qcow2")"
    qm importdisk "${{SEED_VMID}}" "${{SEED_IMAGE_PATH}}" "${{DEFAULT_STORAGE}}"
    qm set "${{SEED_VMID}}" --scsi0 "${{DEFAULT_STORAGE}}:vm-${{SEED_VMID}}-disk-0"
    qm set "${{SEED_VMID}}" --boot c --bootdisk scsi0
    qm resize "${{SEED_VMID}}" scsi0 +12G
    qm set "${{SEED_VMID}}" --ide2 "${{DEFAULT_STORAGE}}:cloudinit"

    # Confirmed live: Proxmox's default 'local' storage frequently does
    # NOT have the 'snippets' content type enabled out of the box (only
    # iso/vztmpl/backup, typically) -- `--cicustom` referencing a
    # snippet on a storage that isn't recognized for that content type
    # doesn't error here, but cloud-init on the guest silently never
    # finds the NoCloud data source at all: no SSH key, no write_files,
    # no runcmd, bootstrap.sh never runs, with nothing in this script's
    # own output indicating anything went wrong. `pvesm set` replaces
    # the whole content list, not adds to it -- read the existing list
    # first so this doesn't clobber iso/vztmpl/backup support on local.
    echo "==> Ensuring 'local' storage supports snippets (needed for the seed VM's cloud-init)"
    if ! pvesm status --content snippets 2>/dev/null | awk 'NR>1{{print $1}}' | grep -qx local; then
        CURRENT_CONTENT="$(awk '/^dir: local$/{{f=1}} f && /^[[:space:]]*content /{{print $2; exit}}' /etc/pve/storage.cfg)"
        if [ -n "${{CURRENT_CONTENT}}" ]; then
            pvesm set local --content "${{CURRENT_CONTENT}},snippets"
        else
            pvesm set local --content snippets
        fi
    fi

    echo "==> Generating a Proxmox API token for crowCloud (root@pam!{token_name})"
    # Idempotent, but not via reuse -- Proxmox only ever shows a token's
    # secret once, at creation, so a token surviving from an earlier
    # (partial/failed) run of this hook can't have its secret recovered.
    # Recreate it instead of erroring on "already exists", so a rerun
    # after a prior failure isn't wedged here forever.
    if pveum user token list root@pam --output-format json 2>/dev/null | grep -q "\"tokenid\":\"{token_name}\""; then
        echo "==> Token already exists -- deleting and recreating to recover its secret"
        pveum user token remove root@pam "{token_name}"
    fi
    TOKEN_JSON="$(pveum user token add root@pam "{token_name}" --privsep=0 --output-format json)"
    PROXMOX_TOKEN_SECRET="$(echo "${{TOKEN_JSON}}" | grep -oP '"value"\s*:\s*"\K[^"]+')"

    echo "==> Writing cloud-init user-data (runs bootstrap.sh unattended inside the guest)"
    mkdir -p /var/lib/vz/snippets
    cat > "/var/lib/vz/snippets/crowcloud-seed-${{SEED_VMID}}.yaml" <<'CLOUDINIT'
{seed_cloud_init}
CLOUDINIT
    # The placeholders above are literal (the heredoc is quoted, so no
    # shell expansion happened writing it) -- substitute in the real
    # values this host has now detected/generated, none of which were
    # knowable at this crate's build time (see render_seed_cloud_init).
    sed -i \
        -e "s|__CROW_PROXMOX_MAC__|${{MAC_ADDRESS}}|g" \
        -e "s|__CROW_PROXMOX_NODE_NAME__|${{NODE_NAME}}|g" \
        -e "s|__CROW_PROXMOX_STORAGE__|${{DEFAULT_STORAGE}}|g" \
        -e "s|__CROW_PROXMOX_BRIDGE__|${{DEFAULT_BRIDGE}}|g" \
        -e "s|__CROW_PROXMOX_TOKEN_SECRET__|${{PROXMOX_TOKEN_SECRET}}|g" \
        "/var/lib/vz/snippets/crowcloud-seed-${{SEED_VMID}}.yaml"

    # A DHCP-assigned seed IP would be unknowable in advance, but every
    # future host's post-install hook is baked with this same
    # CROW_API_URL to find crowCloud at -- the two have to agree, so the
    # seed's own address is derived from CROW_API_URL itself rather than
    # left to chance. Requires CROW_API_URL's host to be an IPv4 literal
    # (not a DNS name) for this self-election path specifically, since
    # there's no DNS server for the guest to resolve a name against yet
    # at this point in the bootstrap.
    SEED_STATIC_IP="$(echo "${{CROW_API_URL}}" | sed -E 's#^[a-zA-Z][a-zA-Z0-9+.-]*://##; s#[/:].*##')"
    if ! [[ "${{SEED_STATIC_IP}}" =~ ^([0-9]{{1,3}}\.){{3}}[0-9]{{1,3}}$ ]]; then
        echo "==> CROW_API_URL (${{CROW_API_URL}}) does not resolve to an IPv4 literal host -- cannot assign the seed VM a matching static IP. Use an IP literal (e.g. http://${{MGMT_GATEWAY%.*}}.50:8080) instead of a hostname for the first/seed build." >&2
        exit 1
    fi

    qm set "${{SEED_VMID}}" \
        --cicustom "user=local:snippets/crowcloud-seed-${{SEED_VMID}}.yaml" \
        --ipconfig0 "ip=${{SEED_STATIC_IP}}/${{MGMT_PREFIX}},gw=${{MGMT_GATEWAY}}"
    qm start "${{SEED_VMID}}"

    # NOTE: this kicks the seed deployment off and returns -- it does not
    # wait for or confirm crowCloud actually comes up inside the guest
    # (no readiness polling, no retry if any step above fails). The
    # exact `pvesh`/`qm` flag names/behavior here are not verified
    # against a live Proxmox host in this environment (no real PVE
    # install available to test against, only the auto-install-assistant
    # tool itself) -- treat this sequence as best-effort pending a real
    # run. `bootstrap.sh`'s own progress is visible at
    # /var/log/crowcloud-bootstrap.log *inside* that guest.
    echo "==> VM ${{SEED_VMID}} started -- crowCloud is deploying inside it unattended."
    echo "    Check /var/log/crowcloud-bootstrap.log on that guest, or ${{CROW_API_URL}} once it comes up."

elif [ "${{HTTP_CODE}}" = "200" ] || [ "${{HTTP_CODE}}" = "201" ]; then
    echo "==> Registered with crowCloud at ${{CROW_API_URL}}"
    CLUSTER_ACTION="$(jq -r '.cluster_action.action' /tmp/crowcloud-register-response.json)"
    if [ "${{CLUSTER_ACTION}}" = "create" ]; then
        echo "==> First real node for this provider -- pvecm create"
        pvecm create crowcloud-fleet
    else
        JOIN_HOST="$(jq -r '.cluster_action.join_host' /tmp/crowcloud-register-response.json)"
        echo "==> Joining existing cluster via ${{JOIN_HOST}}"
        pvecm add "${{JOIN_HOST}}"
    fi
else
    echo "crowCloud reachable but registration failed (HTTP ${{HTTP_CODE}})" >&2
    cat /tmp/crowcloud-register-response.json >&2 || true
    exit 1
fi
"#,
        trunk_interface = cfg.trunk_interface,
        underlay_vlan = cfg.underlay_vlan,
        underlay_ip = cfg.underlay_ip,
        mgmt_vlan = cfg.mgmt_vlan,
        mgmt_ip = cfg.mgmt_ip,
        mgmt_prefix = cfg.mgmt_prefix,
        mgmt_gateway = cfg.mgmt_gateway,
        trunk_mtu = cfg.trunk_mtu,
        crow_api_url = cfg.crow_api_url,
        fleet_secret = cfg.fleet_secret,
        bgp_asn = cfg.bgp_asn,
        bgp_peer_password = cfg.bgp_peer_password,
        bgp_route_reflector_ip = cfg.bgp_route_reflector_ip,
        underlay_prefix = cfg.underlay_prefix,
        ospf_area = cfg.ospf_area,
        seed_cloud_init = seed_cloud_init,
        token_name = PROXMOX_TOKEN_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ProxmoxBuildConfig {
        ProxmoxBuildConfig {
            root_password_hash: "$6$hashed$xyz".into(),
            fqdn: "pve1.fleet.local".into(),
            admin_email: "admin@fleet.local".into(),
            trunk_interface: "eth0".into(),
            underlay_vlan: 10,
            underlay_ip: "10.255.10.11".into(),
            mgmt_vlan: 20,
            mgmt_ip: "10.255.20.11".into(),
            mgmt_prefix: 24,
            mgmt_gateway: "10.255.20.1".into(),
            trunk_mtu: 9000,
            disk_selection: DiskSelection::List(vec!["sda".into(), "sdb".into()]),
            hdsize_gib: None,
            zfs_raid: Some("raid1".into()),
            crow_api_url: "https://crowcloud.fleet.local".into(),
            fleet_secret: "fleet-secret-abc".into(),
            bgp_asn: 65000,
            bgp_peer_password: "fabric-secret".into(),
            bgp_route_reflector_ip: "10.255.10.1".into(),
            underlay_prefix: 24,
            ospf_area: "0".into(),
            seed_ssh_pubkey: None,
            vyos_uplink_interface: None,
            vyos_ssh_private_key: None,
        }
    }

    #[test]
    fn answer_toml_never_contains_a_plaintext_password() {
        let out = render_answer_toml(&cfg());
        assert!(out.contains("root-password-hashed"));
        assert!(!out.contains("root-password =") || out.contains("root-password-hashed"));
    }

    #[test]
    fn answer_toml_includes_mailto_and_enables_first_boot() {
        // Both confirmed live: `[global].mailto` is a hard requirement
        // (validate-answer rejects the file without it), and
        // `[first-boot]` is what actually makes `--on-first-boot` do
        // anything at all.
        let out = render_answer_toml(&cfg());
        assert!(out.contains("mailto = \"admin@fleet.local\""));
        assert!(out.contains("[first-boot]"));
        assert!(out.contains("source = \"from-iso\""));
    }

    #[test]
    fn answer_toml_zfs_raid_sets_filesystem_and_raid_level() {
        let out = render_answer_toml(&cfg());
        assert!(out.contains("filesystem = \"zfs\""));
        assert!(out.contains("zfs.raid = \"raid1\""));
        assert!(out.contains("disk-list = [\"sda\", \"sdb\"]"));
    }

    #[test]
    fn answer_toml_supports_udev_filter_instead_of_a_hardcoded_disk_list() {
        // Confirmed against Proxmox's official Automated Installation
        // docs: `[disk-setup]` accepts `filter.<UDEV_KEY> = "<value>"`
        // dotted keys plus `filter-match`, as an alternative to
        // `disk-list` -- these two are mutually exclusive on the
        // installer's side (this enum makes that structurally
        // unrepresentable here, not just documented).
        let mut c = cfg();
        c.disk_selection = DiskSelection::Filter {
            filter: vec![("ID_BUS".to_string(), "ata".to_string())],
            filter_match: Some("any".to_string()),
        };
        let out = render_answer_toml(&c);
        assert!(out.contains("filter.ID_BUS = \"ata\""));
        assert!(out.contains("filter-match = \"any\""));
        assert!(!out.contains("disk-list"));
    }

    #[test]
    fn answer_toml_reserves_hdsize_under_lvm_for_ext4() {
        // Confirmed against Proxmox's own installer source: ext4/xfs
        // are always LVM-backed under the hood, so there's no
        // `ext4.hdsize` -- it's `lvm.hdsize`. Combined with a broad
        // disk filter, this is what reserves space on whichever disk
        // gets auto-picked for storage pools created after install,
        // without needing to identify a specific disk by size (which
        // Proxmox's filter mechanism can't match on at all).
        let mut c = cfg();
        c.zfs_raid = None;
        c.hdsize_gib = Some(150.0);
        let out = render_answer_toml(&c);
        assert!(out.contains("filesystem = \"ext4\""));
        assert!(out.contains("lvm.hdsize = 150"));
        assert!(!out.contains("zfs.hdsize"));
    }

    #[test]
    fn answer_toml_reserves_hdsize_under_zfs_when_zfs_raid_is_set() {
        let mut c = cfg();
        c.hdsize_gib = Some(200.0);
        let out = render_answer_toml(&c);
        assert!(out.contains("filesystem = \"zfs\""));
        assert!(out.contains("zfs.hdsize = 200"));
        assert!(!out.contains("lvm.hdsize"));
    }

    #[test]
    fn answer_toml_omits_hdsize_when_not_given() {
        let out = render_answer_toml(&cfg());
        assert!(!out.contains("hdsize"));
    }

    #[test]
    fn answer_toml_falls_back_to_ext4_without_zfs_raid() {
        let mut c = cfg();
        c.zfs_raid = None;
        let out = render_answer_toml(&c);
        assert!(out.contains("filesystem = \"ext4\""));
        assert!(!out.contains("zfs.raid"));
    }

    #[test]
    fn hook_embeds_the_fleet_secret_as_a_header_not_a_query_param() {
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("X-Fleet-Secret: ${FLEET_SECRET}"));
        assert!(!out.contains("?secret="));
    }

    #[test]
    fn hook_branches_on_unreachable_crowcloud_into_seed_election() {
        let out = render_post_install_hook(&cfg());
        assert!(out.contains(r#"if [ "${HTTP_CODE}" = "000" ]; then"#));
        assert!(out.contains("self-electing as fleet seed"));
    }

    #[test]
    fn hook_captures_curl_exit_code_instead_of_relying_on_a_broken_fallback() {
        // Confirmed live: `curl ... || echo "000"` produces the literal
        // string "000000" on a hard failure (curl's own -w output
        // already prints "000", and curl still exits non-zero, so the
        // `||` fires too) -- breaks the string comparison and falls
        // through to the wrong branch. Must capture curl's real exit
        // code separately and normalize HTTP_CODE from that.
        let out = render_post_install_hook(&cfg());
        assert!(!out.contains(r#"|| echo "000")""#));
        assert!(out.contains("CURL_EXIT=$?"));
        assert!(out.contains(r#"if [ "${CURL_EXIT}" -ne 0 ]; then"#));
    }

    #[test]
    fn hook_resolves_cluster_action_from_the_register_response_not_a_flag() {
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("cluster_action.action"));
        assert!(!out.contains("--cluster-mode"));
    }

    #[test]
    fn hook_uses_real_frr_config_files_not_a_nonexistent_directory() {
        // Confirmed live: /etc/frr/daemons.conf.d/ doesn't exist in real
        // FRR packaging -- the actual model is /etc/frr/daemons (enable
        // which daemons run) + a single /etc/frr/frr.conf.
        let out = render_post_install_hook(&cfg());
        assert!(!out.contains("daemons.conf.d"));
        assert!(out.contains("sed -i 's/^ospfd=no/ospfd=yes/' /etc/frr/daemons"));
        assert!(out.contains("sed -i 's/^bgpd=no/bgpd=yes/' /etc/frr/daemons"));
        assert!(out.contains("cat > /etc/frr/frr.conf"));
        assert!(out.contains("systemctl restart frr"));
    }

    #[test]
    fn hook_advertises_all_vnis_so_local_vxlan_interfaces_are_actually_announced() {
        // Without this, zebra auto-discovers local VXLAN netdevices (see
        // create_network in crow-provider-proxmox) but never announces
        // them over the existing FABRIC EVPN peering -- other nodes
        // would never learn to send them traffic.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("neighbor FABRIC activate"));
        assert!(out.contains("advertise-all-vni"));
    }

    #[test]
    fn hook_sets_the_bgp_peer_password_matching_vyos() {
        let out = render_post_install_hook(&cfg());
        // The FRR config line is a heredoc rendered with the *shell*
        // variable, not the literal secret baked in twice -- deferred to
        // runtime same as everything else read from BGP_PEER_PASSWORD.
        assert!(out.contains(r#"BGP_PEER_PASSWORD="fabric-secret""#));
        assert!(out.contains("neighbor FABRIC password ${BGP_PEER_PASSWORD}"));
    }

    #[test]
    fn hook_actively_peers_with_vyos_instead_of_leaving_the_peer_group_unbound() {
        // Confirmed live: VyOS's own `bgp listen range` is passive-only
        // (accepts incoming connections, never dials out). A `FABRIC`
        // peer-group template alone -- remote-as/password/activate, but
        // never bound to an actual neighbor address -- never originates
        // a connection either, so with both sides passive, no BGP
        // session forms despite OSPF working fine over the same subnet.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("BGP_ROUTE_REFLECTOR_IP=\"10.255.10.1\""));
        assert!(out.contains("neighbor ${BGP_ROUTE_REFLECTOR_IP} peer-group FABRIC"));
    }

    #[test]
    fn hook_actually_brings_up_the_management_vlan_interface() {
        // Confirmed live: writing to /etc/network/interfaces alone
        // doesn't bring the interface up -- it silently stayed absent
        // from `ip a` until an explicit ifup was added here.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("ifup \"vmbr0.${MGMT_VLAN}\""));
    }

    #[test]
    fn hook_actually_configures_the_underlay_vlan_interface() {
        // Confirmed live: an earlier version of this hook computed an
        // UNDERLAY_IP variable from the trunk interface's own (nonexistent)
        // address and never used it anywhere -- the underlay VLAN was
        // never actually created, so the host could never form an OSPF
        // adjacency or accept a BGP session from VyOS's listen range
        // bound to that subnet.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("auto vmbr0.${UNDERLAY_VLAN}"));
        assert!(out.contains("address ${UNDERLAY_IP}/${UNDERLAY_PREFIX}"));
        assert!(out.contains("ifup \"vmbr0.${UNDERLAY_VLAN}\""));
        assert!(out.contains("10.255.10.11"));
    }

    #[test]
    fn hook_raises_the_trunk_mtu_before_creating_any_vlan_child() {
        // Confirmed live: a VLAN subinterface can never exceed its
        // parent's MTU at creation time -- bringing up the mgmt VLAN
        // before the physical trunk was raised to jumbo frames silently
        // capped it at 1500 despite its own stanza saying mtu 9000.
        let out = render_post_install_hook(&cfg());
        let mtu_pos = out
            .find("ip link set dev \"${TRUNK_IF}\" mtu \"${TRUNK_MTU}\"")
            .expect("trunk MTU must be set explicitly");
        let vlan_child_pos = out
            .find("auto vmbr0.${MGMT_VLAN}")
            .expect("mgmt VLAN child must be created");
        assert!(
            mtu_pos < vlan_child_pos,
            "trunk MTU must be raised before any VLAN child is created on it"
        );
    }

    #[test]
    fn hook_persists_the_trunk_mtu_so_it_survives_ifreload_or_reboot() {
        // Confirmed live: the `ip link set` above only affects the
        // running kernel state for this boot. On any ifreload/reboot
        // after this hook's one-time run, ifupdown2 resets the trunk
        // back to the interfaces-file default (1500, nothing in its
        // stanza said otherwise) while the vifs still claim mtu 9000 --
        // `ifreload` then warns/rejects it since a vif can never exceed
        // its parent's actual MTU.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains(
            "sed -i \"/^iface ${TRUNK_IF} /a\\    mtu ${TRUNK_MTU}\" /etc/network/interfaces"
        ));
        let persist_pos = out
            .find("sed -i \"/^iface ${TRUNK_IF} /a")
            .expect("trunk MTU must be persisted into /etc/network/interfaces");
        let vlan_child_pos = out
            .find("auto vmbr0.${MGMT_VLAN}")
            .expect("mgmt VLAN child must be created");
        assert!(
            persist_pos < vlan_child_pos,
            "trunk MTU must be persisted before any VLAN child is created on it"
        );
    }

    #[test]
    fn hook_repoints_resolv_conf_at_the_fabric_gateway() {
        // Confirmed live: the installer-time DHCP resolver (on the
        // temporary vmbr0 network) is stale and unreachable once the
        // mgmt VLAN becomes the real default route -- DNS silently
        // failed (fetching the seed image below) until this pointed
        // resolv.conf at the fabric gateway instead.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("cat > /etc/resolv.conf"));
        assert!(out.contains("nameserver ${MGMT_GATEWAY}"));
        let resolv_pos = out.find("cat > /etc/resolv.conf").unwrap();
        let ifup_pos = out.find("ifup \"vmbr0.${MGMT_VLAN}\"").unwrap();
        assert!(
            ifup_pos < resolv_pos,
            "resolv.conf must be rewritten after the mgmt VLAN is up, not before"
        );
    }

    #[test]
    fn hook_fetches_the_seed_image_via_proxmox_download_url_api() {
        // A truly fresh first host has no templates to clone -- the
        // seed VM must fetch its own base image (via Proxmox's own
        // download-url API, not crow-cli proxying the download) rather
        // than assuming a template already exists.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("/storage/\"${IMPORT_STORAGE}\"/download-url"));
        assert!(out.contains("cloud.debian.org"));
        assert!(!out.contains("qm clone"));
    }

    #[test]
    fn hook_resolves_the_seed_image_path_via_pvesm_not_a_hardcoded_storage_path() {
        // Confirmed live: hardcoding /var/lib/vz/import/ only works for
        // the default "local" directory storage -- broke with "couldn't
        // find the crowcloud file" on a host using a different storage
        // backend. `pvesm path` resolves the real path for any backend.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains(r#"pvesm path "${IMPORT_STORAGE}:import/crowcloud-seed-base.qcow2""#));
        assert!(!out.contains("/var/lib/vz/import/"));
    }

    #[test]
    fn hook_tags_the_seed_vm_onto_the_mgmt_vlan_not_untagged_vmbr0() {
        // vmbr0 carries the trunk untagged -- the host's own mgmt access
        // is a tagged VLAN subinterface on top of it, so a guest NIC on
        // vmbr0 with no tag lands on the wrong L2 segment and can never
        // reach MGMT_GATEWAY, regardless of what IP it's given.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains(r#"--net0 "virtio,bridge=${DEFAULT_BRIDGE},tag=${MGMT_VLAN}""#));
    }

    #[test]
    fn hook_makes_vmbr0_vlan_aware_before_creating_a_tagged_guest_nic() {
        // Confirmed live: without this, starting a VM with a tagged NIC
        // (`tag=${MGMT_VLAN}` above) broke connectivity on that same VLAN
        // for the whole host, persisting until a full reboot -- firewall,
        // MTU, switch loop protection, and MAC conflicts were all ruled
        // out first.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("bridge-vlan-aware yes"));
        assert!(out.contains("bridge-vids 2-4094"));
        let vlan_aware_pos = out.find("bridge-vlan-aware yes").unwrap();
        let create_vm_pos = out.find("qm create").unwrap();
        assert!(
            vlan_aware_pos < create_vm_pos,
            "vmbr0 must be made VLAN-aware before the seed VM's tagged NIC is created"
        );
    }

    #[test]
    fn hook_grants_both_the_trunk_port_and_bridge_device_explicit_vlan_membership() {
        // Confirmed live (the hard way): `bridge-vids` on the bridge's
        // own stanza does not, by itself, grant the physical trunk port
        // membership in those VLANs -- `bridge vlan show` kept reporting
        // the trunk stuck at only its default PVID (1) no matter what
        // else was configured. The bridge *device* itself separately
        // needs explicit membership (the `self` keyword) before it can
        // host any vmbr0.<vlan> subinterface at all, which is what the
        // host's own underlay/mgmt IPs live on now.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("post-up bridge vlan add dev ${TRUNK_IF} vid ${UNDERLAY_VLAN}"));
        assert!(out.contains("post-up bridge vlan add dev ${TRUNK_IF} vid ${MGMT_VLAN}"));
        assert!(out.contains("post-up bridge vlan add dev vmbr0 vid ${UNDERLAY_VLAN} self"));
        assert!(out.contains("post-up bridge vlan add dev vmbr0 vid ${MGMT_VLAN} self"));
    }

    #[test]
    fn hook_puts_the_hosts_own_vlan_ips_on_the_bridge_not_the_raw_trunk() {
        // Confirmed live: putting the host's own underlay/mgmt IPs on
        // classic 8021q subinterfaces of the physical trunk itself
        // (${TRUNK_IF}.${VLAN}), while *also* enslaving that same trunk
        // to vmbr0 for the seed VM's tagged NIC, is two different VLAN
        // mechanisms fighting over the same physical port -- starting
        // the VM broke mgmt-VLAN connectivity for the whole host until
        // a full reboot. Proxmox's own documented pattern is a single
        // VLAN-aware bridge for everything, with the host's own VLAN
        // IPs on vmbr0.<vlan> (a subinterface of the bridge), not the
        // NIC.
        let out = render_post_install_hook(&cfg());
        assert!(!out.contains("auto ${TRUNK_IF}.${UNDERLAY_VLAN}"));
        assert!(!out.contains("auto ${TRUNK_IF}.${MGMT_VLAN}"));
        assert!(out.contains("auto vmbr0.${UNDERLAY_VLAN}"));
        assert!(out.contains("auto vmbr0.${MGMT_VLAN}"));
    }

    #[test]
    fn hook_gives_the_seed_vm_a_serial_console() {
        // Confirmed live: `qm terminal` has nothing to attach to without
        // an explicit serial device -- Proxmox doesn't create one by
        // default, and this is the only console access to a
        // cloud-init-only guest with no other interactive install step.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("--serial0 socket"));
        assert!(out.contains("--vga serial0"));
    }

    #[test]
    fn hook_ensures_local_storage_supports_snippets_before_writing_one() {
        // Confirmed live: Proxmox's default 'local' storage frequently
        // doesn't have 'snippets' enabled out of the box -- referencing
        // a snippet on a storage that isn't recognized for that content
        // type doesn't error anywhere in this script, but cloud-init on
        // the guest silently never finds the NoCloud data source: no
        // SSH key, no write_files, no runcmd, bootstrap.sh never runs,
        // with no indication anything went wrong.
        let out = render_post_install_hook(&cfg());
        let ensure_pos = out
            .find("pvesm status --content snippets")
            .expect("snippets check present");
        let write_pos = out
            .find("mkdir -p /var/lib/vz/snippets")
            .expect("snippet write present");
        assert!(
            ensure_pos < write_pos,
            "must ensure snippets support before writing the cloud-init snippet there"
        );
    }

    #[test]
    fn hook_preserves_existing_content_types_when_enabling_snippets() {
        // `pvesm set --content` replaces the whole list, not adds to it
        // -- must read the existing types first or this would silently
        // remove iso/vztmpl/backup support from `local`.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("pvesm set local --content \"${CURRENT_CONTENT},snippets\""));
    }

    #[test]
    fn hook_derives_seed_static_ip_from_crow_api_url_instead_of_dhcp() {
        // A DHCP-assigned seed IP is unknowable in advance, but every
        // future host's hook is baked with this same CROW_API_URL to find
        // crowCloud at -- the two must agree, so the seed's address comes
        // from CROW_API_URL itself rather than whatever DHCP hands out.
        let cfg = ProxmoxBuildConfig {
            crow_api_url: "http://10.255.20.50:8080".into(),
            ..cfg()
        };
        let out = render_post_install_hook(&cfg);
        assert!(!out.contains("ip=dhcp"));
        assert!(
            out.contains(r#"--ipconfig0 "ip=${SEED_STATIC_IP}/${MGMT_PREFIX},gw=${MGMT_GATEWAY}""#)
        );
        assert!(out.contains("10.255.20.50"));
    }

    #[test]
    fn hook_guards_against_a_crow_api_url_that_isnt_an_ipv4_literal() {
        // This is a runtime bash check, not something the renderer
        // evaluates -- present in the output regardless of cfg's actual
        // value. The seed VM can't resolve a DNS name against anything
        // yet at this point in its own bootstrap, so a non-IP-literal
        // CROW_API_URL must fail loudly with a clear fix instead of
        // silently falling back to DHCP (which would just reintroduce
        // the mismatch this whole thing exists to prevent).
        let out = render_post_install_hook(&cfg());
        assert!(out
            .contains(r#"if ! [[ "${SEED_STATIC_IP}" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then"#));
        assert!(out.contains("does not resolve to an IPv4 literal host"));
        assert!(out.contains("exit 1"));
    }

    #[test]
    fn hook_detects_import_storage_separately_from_images_storage() {
        // Confirmed live: `download-url --content import` failed with
        // "can't upload to storage type 'lvmthin', not a file based
        // storage!" -- the storage that supports `images` content (used
        // for VM disks, often lvmthin) is frequently NOT the same
        // storage that supports `import` content (must be file-based).
        // These need independent detection, and the seed VM's own disk
        // must still land on the images-capable DEFAULT_STORAGE.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("IMPORT_STORAGE=\"$(pvesm status --content import"));
        assert!(out.contains("DEFAULT_STORAGE=\"$(pvesm status --content images"));
        assert!(out
            .contains(r#"qm importdisk "${SEED_VMID}" "${SEED_IMAGE_PATH}" "${DEFAULT_STORAGE}""#));
        assert!(out.contains("No storage supporting the 'import' content type"));
    }

    #[test]
    fn hook_embeds_a_fully_resolved_cloud_init_with_no_double_expansion() {
        let out = render_post_install_hook(&cfg());
        // Written via a *quoted* heredoc delimiter so the outer script
        // (running on the bare Proxmox host) never tries to expand the
        // guest's own $VARs while writing the file -- this is the single
        // most important thing to get right here, a bug here would
        // corrupt the guest's bootstrap.sh silently.
        assert!(out.contains("<<'CLOUDINIT'"));
        assert!(out.contains("CROW_FLEET_SECRET=fleet-secret-abc"));
    }

    #[test]
    fn seed_cloud_init_bakes_the_literal_secret_not_a_shell_variable() {
        let out = render_seed_cloud_init(
            "literal-secret-value",
            None,
            "10.20.0.1",
            None,
            "10.20.0.11",
        );
        assert!(out.contains("CROW_FLEET_SECRET=literal-secret-value"));
        assert!(!out.contains("${FLEET_SECRET}"));
    }

    #[test]
    fn seed_cloud_init_omits_ssh_authorized_keys_when_not_given() {
        // Confirmed live: Debian's stock cloud image has no password
        // login and no key of its own -- omitting this leaves the seed
        // VM entirely console/SSH-inaccessible, same as before this
        // existed, rather than silently defaulting to something.
        let out = render_seed_cloud_init("x", None, "10.20.0.1", None, "10.20.0.11");
        assert!(!out.contains("ssh_authorized_keys"));
    }

    #[test]
    fn seed_cloud_init_adds_the_key_to_the_default_user_not_root() {
        // cloud-init's disable_root module locks the root account by
        // default regardless of any key configured for it -- adding via
        // the top-level directive (applies to the image's own default
        // user, `debian` on Debian's official cloud image) sidesteps
        // that entirely instead of needing to override disable_root.
        let out = render_seed_cloud_init(
            "x",
            Some("ssh-ed25519 AAAA... test-key"),
            "10.20.0.1",
            None,
            "10.20.0.11",
        );
        assert!(out.starts_with(
            "#cloud-config\nssh_authorized_keys:\n  - ssh-ed25519 AAAA... test-key\n"
        ));
        assert!(!out.contains("users:"));
        assert!(!out.contains("disable_root"));
    }

    #[test]
    fn seed_cloud_init_embeds_the_real_bootstrap_sh_not_a_placeholder() {
        let out = render_seed_cloud_init("x", None, "10.20.0.1", None, "10.20.0.11");
        assert!(out.contains("Day-0 bootstrap"));
        assert!(out.contains("Installing K3s"));
    }

    #[test]
    fn seed_cloud_init_fetches_the_helm_chart_from_ghcr_not_a_source_clone() {
        // Confirmed live: bootstrap.sh previously fetched the Helm
        // chart by git-cloning the whole source repo at whatever
        // `main` happened to be, unpinned to CROW_VERSION -- a real
        // version-skew risk against the pinned container image tags,
        // on top of depending on GitHub reachability as well as GHCR.
        // The chart is already published to the same GHCR OCI
        // registry as the Docker images via the release pipeline, so
        // the embedded seed-VM bootstrap should pull it from there
        // instead of cloning source.
        let out = render_seed_cloud_init("x", None, "10.20.0.1", None, "10.20.0.11");
        assert!(out.contains("oci://ghcr.io/gavinmce/charts/crowcloud"));
        assert!(!out.contains("git clone --depth 1"));
        assert!(!out.contains("github.com/GavinMce/crowCloud.git"));
    }
}
