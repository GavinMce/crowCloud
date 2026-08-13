use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::iso::fabric::FabricConfig;
use crate::iso::{proxmox as proxmox_iso, vyos as vyos_iso, vyos_flavor as vyos_flavor_iso};

#[derive(Args)]
pub struct IsoCmd {
    #[command(subcommand)]
    pub command: IsoSubcommand,
}

#[derive(Subcommand)]
pub enum IsoSubcommand {
    /// Build a pre-baked VyOS image (#66)
    Vyos(VyosCmd),
    /// Build a pre-baked Proxmox VE image (#66)
    Proxmox(Box<ProxmoxCmd>),
    /// Configure fabric-wide network settings once, shared by every
    /// `iso vyos`/`iso proxmox` build command afterwards (#66) --
    /// values like the BGP peer password or VLAN IDs have to match
    /// exactly across every host, so this is the single place they get
    /// typed instead of once per build
    FabricConfigure(Box<FabricConfigureArgs>),
}

#[derive(Args)]
pub struct FabricConfigureArgs {
    #[arg(long)]
    pub underlay_vlan: Option<u16>,
    #[arg(long)]
    pub underlay_network: Option<String>,
    #[arg(long)]
    pub underlay_network_prefix: Option<u8>,
    #[arg(long)]
    pub mgmt_vlan: Option<u16>,
    #[arg(long)]
    pub mgmt_network: Option<String>,
    #[arg(long)]
    pub mgmt_network_prefix: Option<u8>,
    /// VyOS's own IP on the mgmt VLAN -- also every Proxmox host's
    /// default gateway on that VLAN
    #[arg(long)]
    pub mgmt_gateway: Option<String>,
    #[arg(long)]
    pub trunk_mtu: Option<u32>,
    #[arg(long)]
    pub ospf_area: Option<String>,
    #[arg(long)]
    pub bgp_asn: Option<u32>,
    #[arg(long)]
    pub bgp_peer_password: Option<String>,
    /// VyOS's own IP on the underlay VLAN -- the BGP route-reflector
    /// every Proxmox host actively peers with (VyOS's own listen range
    /// is passive-only, so this can't be discovered automatically)
    #[arg(long)]
    pub bgp_route_reflector_ip: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub dns_servers: Option<Vec<String>>,
    /// Subnet for admin WireGuard VPN peers -- leave unset if you don't
    /// want VyOS's WireGuard server configured at all
    #[arg(long)]
    pub wireguard_network: Option<String>,
    #[arg(long)]
    pub wireguard_network_prefix: Option<u8>,
}

#[derive(Args)]
pub struct VyosCmd {
    #[command(subcommand)]
    pub command: VyosSubcommand,
}

#[derive(Subcommand)]
pub enum VyosSubcommand {
    /// Render fabric config, either as a one-shot script or baked into
    /// a custom image (#66)
    Build(VyosBuildCmd),
    /// Apply a rendered configure.txt to a live VyOS device over SSH
    /// (#66) -- VyOS has no unattended install mode, so a router still
    /// needs one interactive `install image` session per box; this
    /// automates the fabric-config step that comes after that
    Apply(VyosApplyArgs),
    /// Manage admin WireGuard VPN peers on a live VyOS device -- separate
    /// from `build`/`apply` since adding/removing an admin is an ongoing
    /// operation, not a one-time server setup
    Wireguard(WireguardCmd),
}

#[derive(Args)]
pub struct WireguardCmd {
    #[command(subcommand)]
    pub command: WireguardSubcommand,
}

#[derive(Subcommand)]
pub enum WireguardSubcommand {
    /// Add an admin as a WireGuard peer -- generates their client
    /// keypair locally (the private key never leaves this machine) and
    /// pushes only the public key to VyOS
    AddPeer(WireguardAddPeerArgs),
    /// Remove an admin's WireGuard peer
    RemovePeer(WireguardRemovePeerArgs),
}

#[derive(Args)]
pub struct WireguardAddPeerArgs {
    /// Name for this peer (e.g. the admin's own name) -- must be unique
    /// on this VyOS device
    pub name: String,
    /// This admin's own VPN tunnel address -- an IP inside the fabric's
    /// wireguard_network. No allocator exists yet, same "pick one by
    /// hand" posture underlay_ip/mgmt_ip already have for hosts.
    #[arg(long)]
    pub client_address: String,
    /// VyOS's uplink IP or hostname, reachable from this machine
    #[arg(long)]
    pub host: String,
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    #[arg(long, default_value = "vyos")]
    pub user: String,
    /// Private key matching the public key baked into the image via
    /// `iso vyos build --ssh-pubkey`
    #[arg(long)]
    pub ssh_key: PathBuf,
    #[arg(long)]
    pub insecure_skip_host_key_check: bool,
    /// VyOS's WireGuard listen port -- must match what `iso vyos build`
    /// configured
    #[arg(long, default_value_t = 51820)]
    pub wireguard_port: u16,
}

#[derive(Args)]
pub struct WireguardRemovePeerArgs {
    pub name: String,
    /// VyOS's uplink IP or hostname, reachable from this machine
    #[arg(long)]
    pub host: String,
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    #[arg(long, default_value = "vyos")]
    pub user: String,
    #[arg(long)]
    pub ssh_key: PathBuf,
    #[arg(long)]
    pub insecure_skip_host_key_check: bool,
}

#[derive(Args)]
pub struct VyosBuildCmd {
    #[command(subcommand)]
    pub command: VyosBuildSubcommand,
}

#[derive(Subcommand)]
pub enum VyosBuildSubcommand {
    /// Render configure.txt -- a one-shot script meant to be applied
    /// to an already-installed, already-running VyOS box (by hand, or
    /// via `iso vyos apply`). Interfaces are identified by static
    /// kernel-assigned name
    Config(Box<VyosBuildArgs>),
    /// Render the inputs to bake a custom, self-configuring VyOS image
    /// via vyos-build (#63) -- once flashed, the box applies its
    /// fabric config on every boot with no `iso vyos apply` step
    /// needed afterward. Interfaces are identified by PCI bus address
    /// instead of kernel name, resolved dynamically at boot, so it
    /// survives a NIC swap. Neither this nor `config` touches VyOS's
    /// own `install image` step -- there's no unattended install mode
    /// regardless of image customization
    Image(Box<VyosFlavorArgs>),
}

#[derive(Args)]
pub struct VyosApplyArgs {
    /// Device management IP or hostname
    #[arg(long)]
    pub host: String,
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    #[arg(long, default_value = "vyos")]
    pub user: String,
    /// Private key matching the public key baked into the image via
    /// `iso vyos build --ssh-pubkey` -- this only supports key-based
    /// auth, matching that command's key-only default
    #[arg(long)]
    pub ssh_key: PathBuf,
    /// Path to a rendered configure.txt (from `iso vyos build`'s `--out`)
    #[arg(long)]
    pub script: PathBuf,
    /// Skip host key verification entirely instead of trust-on-first-use
    /// (accept-new). Only for throwaway/lab devices -- accept-new still
    /// catches a host key that unexpectedly changed on a known device
    #[arg(long)]
    pub insecure_skip_host_key_check: bool,
}

#[derive(Args)]
pub struct VyosFlavorArgs {
    #[arg(long)]
    pub hostname: Option<String>,
    #[arg(long)]
    pub trunk_mtu: Option<u32>,
    #[arg(long, requires = "trunk_duplex")]
    pub trunk_speed: Option<String>,
    #[arg(long, requires = "trunk_speed")]
    pub trunk_duplex: Option<String>,
    #[arg(long)]
    pub underlay_vlan: Option<u16>,
    #[arg(long)]
    pub underlay_ip: Option<String>,
    #[arg(long)]
    pub underlay_prefix: Option<u8>,
    #[arg(long)]
    pub mgmt_vlan: Option<u16>,
    #[arg(long)]
    pub mgmt_ip: Option<String>,
    #[arg(long)]
    pub mgmt_prefix: Option<u8>,
    #[arg(long)]
    pub mgmt_network: Option<String>,
    #[arg(long)]
    pub mgmt_network_prefix: Option<u8>,
    #[arg(long)]
    pub loopback_ip: Option<String>,
    #[arg(long)]
    pub uplink_dhcp: Option<bool>,
    #[arg(long)]
    pub uplink_ip: Option<String>,
    #[arg(long)]
    pub uplink_prefix: Option<u8>,
    #[arg(long)]
    pub uplink_gateway: Option<String>,
    #[arg(long)]
    pub ospf_area: Option<String>,
    #[arg(long)]
    pub underlay_network: Option<String>,
    #[arg(long)]
    pub underlay_network_prefix: Option<u8>,
    #[arg(long)]
    pub ssh_pubkey: Option<PathBuf>,
    #[arg(long)]
    pub bgp_asn: Option<u32>,
    #[arg(long)]
    pub bgp_peer_password: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub dns_servers: Option<Vec<String>>,
    #[arg(long)]
    pub allow_password_auth: Option<bool>,
    /// crowCloud control plane's mgmt-VLAN IP -- when set together with
    /// --crow-api-mgmt-port and/or --crow-frontend-mgmt-port, bakes in a
    /// NAT rule per port forwarding it from the uplink straight to this
    /// IP, so the control plane is reachable from the upstream LAN (both
    /// on an ongoing basis for admin/CLI access, and during bootstrap,
    /// before it's up enough to configure an ExposedEndpoint for itself).
    /// Leave unset if there's no crowCloud instance on this fabric yet.
    #[arg(long)]
    pub crow_api_mgmt_ip: Option<String>,
    #[arg(long, requires = "crow_api_mgmt_ip")]
    pub crow_api_mgmt_port: Option<u16>,
    /// Same idea as --crow-api-mgmt-port, for the web frontend -- forwarded
    /// to the same --crow-api-mgmt-ip (frontend and API are served from
    /// the same node)
    #[arg(long, requires = "crow_api_mgmt_ip")]
    pub crow_frontend_mgmt_port: Option<u16>,
    /// Enables VyOS's admin WireGuard VPN server -- leave all three
    /// wireguard-* flags unset to skip it entirely. Falls back to the
    /// fabric config's wireguard_network/wireguard_network_prefix if
    /// omitted.
    #[arg(long)]
    pub wireguard_port: Option<u16>,
    #[arg(long)]
    pub wireguard_address: Option<String>,
    #[arg(long)]
    pub wireguard_address_prefix: Option<u8>,
    /// Directory to write the rendered fabric-init script and
    /// vyos-build flavor TOML into
    #[arg(long, default_value = "./build")]
    pub out: PathBuf,
}

#[derive(Args)]
pub struct ProxmoxCmd {
    #[command(subcommand)]
    pub command: ProxmoxSubcommand,
}

#[derive(Subcommand)]
pub enum ProxmoxSubcommand {
    Build(ProxmoxBuildArgs),
}

/// Every field without a sensible fallback is `Option` -- omitted on
/// the command line, it's prompted for interactively instead of clap
/// erroring "required argument missing". Running with zero flags is a
/// full wizard walkthrough; running with every flag is unchanged from
/// before interactive mode existed (#66).
#[derive(Args)]
pub struct VyosBuildArgs {
    #[arg(long)]
    pub hostname: Option<String>,
    /// Physical NIC carrying the tagged trunk to the switch (underlay + management VLANs)
    #[arg(long)]
    pub trunk_interface: Option<String>,
    /// Physical NIC used for internet/LAN uplink
    #[arg(long)]
    pub uplink_interface: Option<String>,
    #[arg(long)]
    pub trunk_mtu: Option<u32>,
    /// Pin the trunk to a fixed speed instead of auto-negotiation --
    /// requires --trunk-duplex too
    #[arg(long, requires = "trunk_duplex")]
    pub trunk_speed: Option<String>,
    #[arg(long, requires = "trunk_speed")]
    pub trunk_duplex: Option<String>,
    #[arg(long)]
    pub underlay_vlan: Option<u16>,
    #[arg(long)]
    pub underlay_ip: Option<String>,
    #[arg(long)]
    pub underlay_prefix: Option<u8>,
    #[arg(long)]
    pub mgmt_vlan: Option<u16>,
    #[arg(long)]
    pub mgmt_ip: Option<String>,
    #[arg(long)]
    pub mgmt_prefix: Option<u8>,
    /// Network address of the mgmt subnet (not this router's own IP)
    /// -- used for the NAT masquerade and DNS-forwarding allow-from
    /// rules that give mgmt-VLAN hosts internet egress
    #[arg(long)]
    pub mgmt_network: Option<String>,
    #[arg(long)]
    pub mgmt_network_prefix: Option<u8>,
    #[arg(long)]
    pub loopback_ip: Option<String>,
    #[arg(long)]
    pub uplink_dhcp: Option<bool>,
    #[arg(long)]
    pub uplink_ip: Option<String>,
    #[arg(long)]
    pub uplink_prefix: Option<u8>,
    #[arg(long)]
    pub uplink_gateway: Option<String>,
    #[arg(long)]
    pub ospf_area: Option<String>,
    #[arg(long)]
    pub underlay_network: Option<String>,
    #[arg(long)]
    pub underlay_network_prefix: Option<u8>,
    /// Path to a public key file -- image is built SSH-key-only, never
    /// with a baked password
    #[arg(long)]
    pub ssh_pubkey: Option<PathBuf>,
    #[arg(long)]
    pub bgp_asn: Option<u32>,
    /// Shared secret for BGP peer-group auth -- must match every Proxmox
    /// host's `--bgp-peer-password` (#66)
    #[arg(long)]
    pub bgp_peer_password: Option<String>,
    /// Recursive DNS forwarders for mgmt-VLAN hosts. Not every public
    /// resolver is reachable from every network (confirmed live: 9.9.9.9
    /// and 1.1.1.1 both timed out on one real deployment while 8.8.8.8
    /// worked), so this is overridable rather than a single hardcoded
    /// default baked in unconditionally
    #[arg(long, value_delimiter = ',')]
    pub dns_servers: Option<Vec<String>>,
    /// Keep SSH password auth enabled alongside the new key, instead of
    /// disabling it. Default is key-only; use this while validating key
    /// access on a given box, since a bad key commit + disabled password
    /// auth means an SSH lockout with no fallback (see #66's incident
    /// notes)
    #[arg(long)]
    pub allow_password_auth: Option<bool>,
    /// crowCloud control plane's mgmt-VLAN IP -- when set together with
    /// --crow-api-mgmt-port and/or --crow-frontend-mgmt-port, bakes in a
    /// NAT rule per port forwarding it from the uplink straight to this
    /// IP, so the control plane is reachable from the upstream LAN (both
    /// on an ongoing basis for admin/CLI access, and during bootstrap,
    /// before it's up enough to configure an ExposedEndpoint for itself).
    /// Leave unset if there's no crowCloud instance on this fabric yet.
    #[arg(long)]
    pub crow_api_mgmt_ip: Option<String>,
    #[arg(long, requires = "crow_api_mgmt_ip")]
    pub crow_api_mgmt_port: Option<u16>,
    /// Same idea as --crow-api-mgmt-port, for the web frontend -- forwarded
    /// to the same --crow-api-mgmt-ip (frontend and API are served from
    /// the same node)
    #[arg(long, requires = "crow_api_mgmt_ip")]
    pub crow_frontend_mgmt_port: Option<u16>,
    /// Enables VyOS's admin WireGuard VPN server -- leave all three
    /// wireguard-* flags unset to skip it entirely. Falls back to the
    /// fabric config's wireguard_network/wireguard_network_prefix if
    /// omitted.
    #[arg(long)]
    pub wireguard_port: Option<u16>,
    #[arg(long)]
    pub wireguard_address: Option<String>,
    #[arg(long)]
    pub wireguard_address_prefix: Option<u8>,
    /// Directory to write the rendered configure-script into
    #[arg(long, default_value = "./build")]
    pub out: PathBuf,
    /// Skip invoking `vyos-build` -- just render the config script
    #[arg(long)]
    pub render_only: bool,
}

#[derive(Args)]
pub struct ProxmoxBuildArgs {
    /// Plaintext root password -- hashed locally via `openssl passwd -6`
    /// before it ever touches disk; never stored or logged in plaintext
    #[arg(long)]
    pub root_password: Option<String>,
    #[arg(long)]
    pub fqdn: Option<String>,
    /// Required by answer.toml's [global] section -- confirmed live
    /// against proxmox-auto-install-assistant, no default offered since
    /// there's no sensible one
    #[arg(long)]
    pub admin_email: Option<String>,
    #[arg(long)]
    pub trunk_interface: Option<String>,
    /// This host's own IP on the underlay VLAN. No IPAM allocator
    /// exists yet (#54), so this is manual per-host input for now --
    /// without it, this host can't form an OSPF adjacency or accept a
    /// BGP session from VyOS's dynamic listen range at all.
    #[arg(long)]
    pub underlay_ip: Option<String>,
    /// Falls back to the shared fabric config (`iso fabric-configure`)
    /// if omitted
    #[arg(long)]
    pub underlay_vlan: Option<u16>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub mgmt_vlan: Option<u16>,
    #[arg(long)]
    pub mgmt_ip: Option<String>,
    #[arg(long)]
    pub mgmt_prefix: Option<u8>,
    /// This host's default gateway on the mgmt VLAN -- falls back to
    /// the shared fabric config's `mgmt_gateway` (VyOS's own mgmt IP)
    /// if omitted
    #[arg(long)]
    pub mgmt_gateway: Option<String>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub trunk_mtu: Option<u32>,
    /// Advanced override -- explicit disk device names (e.g. sda,sdb),
    /// mutually exclusive with --disk-filter. Not asked in the wizard;
    /// omit both this and --disk-filter and it defaults to "first real
    /// disk found" automatically (confirmed safe against Proxmox's own
    /// disk-enumeration source: already excludes loop/dm/md/ram/
    /// optical devices and the boot/live medium regardless of bus type)
    #[arg(long, value_delimiter = ',', conflicts_with = "disk_filter")]
    pub disk: Option<Vec<String>>,
    /// Advanced override -- match disks by UDEV property instead of
    /// explicit names (e.g. ID_BUS=ata), mutually exclusive with
    /// --disk. Not asked in the wizard; verify against `udevadm info
    /// --query=property --name=/dev/sdX` on the real target hardware
    /// before using a custom filter
    #[arg(long, value_delimiter = ',')]
    pub disk_filter: Option<Vec<String>>,
    /// "any" (default) or "all" -- whether one matching filter key is
    /// enough, or every key must match. Only meaningful with --disk-filter
    #[arg(long)]
    pub disk_filter_match: Option<String>,
    /// GiB for the OS disk -- the rest of whatever disk gets picked
    /// (plus every other disk present) is left genuinely unpartitioned
    /// for storage pools created later
    #[arg(long)]
    pub hdsize_gib: Option<f64>,
    #[arg(long)]
    pub zfs_raid: Option<String>,
    /// Where the post-install hook looks for a reachable crowCloud
    /// instance before self-electing as the fleet seed (#67)
    #[arg(long)]
    pub crow_api_url: Option<String>,
    /// Baked into the image as the self-registration credential.
    /// Defaults to the locally cached fleet secret, generating one on
    /// first use if none exists yet -- no crowCloud login required
    /// (#67's bootstrap case)
    #[arg(long)]
    pub fleet_secret: Option<String>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub bgp_asn: Option<u32>,
    /// VyOS's own underlay IP -- the BGP route-reflector this host
    /// actively peers with. Falls back to the shared fabric config if
    /// omitted
    #[arg(long)]
    pub bgp_route_reflector_ip: Option<String>,
    /// Falls back to the shared fabric config's `underlay_network_prefix`
    /// if omitted
    #[arg(long)]
    pub underlay_prefix: Option<u8>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub ospf_area: Option<String>,
    /// A locally-provided Proxmox VE ISO -- never auto-downloaded
    #[arg(long)]
    pub base_iso: Option<PathBuf>,
    /// SSH public key path for the seed VM specifically (#67) -- not
    /// asked in the wizard (advanced/optional, like --disk-filter):
    /// Debian's stock cloud image has no password login and no key of
    /// its own, so omitting this leaves the seed VM entirely
    /// console/SSH-inaccessible once cloud-init applies.
    #[arg(long)]
    pub seed_ssh_pubkey: Option<PathBuf>,
    /// Physical uplink NIC name on the VyOS box (e.g. "eth1") -- not
    /// asked in the wizard (advanced/optional): required together with
    /// `--vyos-ssh-private-key` to let the seed VM auto-configure the
    /// operator's VyOS connection. Omit both to leave that a manual
    /// `helm upgrade --set operator.vyos.*` step after the fact, same
    /// as before this existed.
    #[arg(long, requires = "vyos_ssh_private_key")]
    pub vyos_uplink_interface: Option<String>,
    /// Private key path matching the public key baked into the VyOS
    /// image via `crow-cli iso vyos build --ssh-pubkey`. Required
    /// together with `--vyos-uplink-interface`.
    #[arg(long, requires = "vyos_uplink_interface")]
    pub vyos_ssh_private_key: Option<PathBuf>,
    #[arg(long, default_value = "./build")]
    pub out: PathBuf,
    /// Skip invoking `proxmox-auto-install-assistant` -- just render
    /// answer.toml and the post-install hook
    #[arg(long)]
    pub render_only: bool,
}

pub async fn run(cmd: IsoCmd) -> Result<()> {
    match cmd.command {
        IsoSubcommand::Vyos(vyos_cmd) => match vyos_cmd.command {
            VyosSubcommand::Build(build_cmd) => match build_cmd.command {
                VyosBuildSubcommand::Config(args) => build_vyos(*args),
                VyosBuildSubcommand::Image(args) => flavor_vyos(*args),
            },
            VyosSubcommand::Apply(args) => apply_vyos(args).await,
            VyosSubcommand::Wireguard(wg_cmd) => match wg_cmd.command {
                WireguardSubcommand::AddPeer(args) => add_wireguard_peer(args).await,
                WireguardSubcommand::RemovePeer(args) => remove_wireguard_peer(args).await,
            },
        },
        IsoSubcommand::Proxmox(proxmox_cmd) => match proxmox_cmd.command {
            ProxmoxSubcommand::Build(args) => build_proxmox(args),
        },
        IsoSubcommand::FabricConfigure(args) => fabric_configure(*args),
    }
}

fn fabric_configure(args: FabricConfigureArgs) -> Result<()> {
    use crate::iso::vyos_wizard as wiz;

    let existing = Config::load()?.fabric;

    let underlay_vlan = wiz::prompt(
        args.underlay_vlan,
        "Underlay VLAN ID",
        existing.as_ref().map(|f| f.underlay_vlan),
    )?;
    let underlay_network = wiz::prompt(
        args.underlay_network,
        "Underlay network address",
        existing.as_ref().map(|f| f.underlay_network.clone()),
    )?;
    let underlay_network_prefix = wiz::prompt(
        args.underlay_network_prefix,
        "Underlay network prefix length",
        existing
            .as_ref()
            .map(|f| f.underlay_network_prefix)
            .or(Some(24)),
    )?;
    let mgmt_vlan = wiz::prompt(
        args.mgmt_vlan,
        "Management VLAN ID",
        existing.as_ref().map(|f| f.mgmt_vlan),
    )?;
    let mgmt_network = wiz::prompt(
        args.mgmt_network,
        "Management subnet network address",
        existing.as_ref().map(|f| f.mgmt_network.clone()),
    )?;
    let mgmt_network_prefix = wiz::prompt(
        args.mgmt_network_prefix,
        "Management subnet prefix length",
        existing
            .as_ref()
            .map(|f| f.mgmt_network_prefix)
            .or(Some(24)),
    )?;
    let mgmt_gateway = wiz::prompt(
        args.mgmt_gateway,
        "Management gateway IP (VyOS's own IP on the mgmt VLAN)",
        existing.as_ref().map(|f| f.mgmt_gateway.clone()),
    )?;
    let trunk_mtu = wiz::prompt(
        args.trunk_mtu,
        "Trunk MTU",
        existing.as_ref().map(|f| f.trunk_mtu).or(Some(9000)),
    )?;
    let ospf_area = wiz::prompt(
        args.ospf_area,
        "OSPF area",
        existing
            .as_ref()
            .map(|f| f.ospf_area.clone())
            .or(Some("0".to_string())),
    )?;
    let bgp_asn = wiz::prompt(
        args.bgp_asn,
        "BGP ASN",
        existing.as_ref().map(|f| f.bgp_asn).or(Some(65000)),
    )?;
    let bgp_peer_password = wiz::prompt_secret_optional(
        args.bgp_peer_password
            .or(existing.as_ref().and_then(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
    )?;
    let bgp_route_reflector_ip = wiz::prompt(
        args.bgp_route_reflector_ip,
        "VyOS's own underlay IP (the BGP route-reflector every Proxmox host peers with)",
        existing.as_ref().map(|f| f.bgp_route_reflector_ip.clone()),
    )?;
    let dns_servers = wiz::prompt_list(
        args.dns_servers
            .or(existing.as_ref().map(|f| f.dns_servers.clone())),
        "DNS forwarders for mgmt-VLAN hosts (comma-separated)",
        Some(&["8.8.8.8".to_string(), "8.8.4.4".to_string()]),
    )?;
    let wireguard_network = wiz::prompt_optional(
        args.wireguard_network
            .or(existing.as_ref().and_then(|f| f.wireguard_network.clone())),
        "Subnet for admin WireGuard VPN peers (leave blank to skip VPN entirely)",
    )?;
    let wireguard_network_prefix = if wireguard_network.is_some() {
        Some(wiz::prompt(
            args.wireguard_network_prefix,
            "WireGuard VPN subnet prefix length",
            existing
                .as_ref()
                .and_then(|f| f.wireguard_network_prefix)
                .or(Some(24)),
        )?)
    } else {
        None
    };

    let mut cfg = Config::load()?;
    cfg.fabric = Some(FabricConfig {
        underlay_vlan,
        underlay_network,
        underlay_network_prefix,
        mgmt_vlan,
        mgmt_network,
        mgmt_network_prefix,
        mgmt_gateway,
        trunk_mtu,
        ospf_area,
        bgp_asn,
        bgp_peer_password,
        bgp_route_reflector_ip,
        dns_servers,
        wireguard_network,
        wireguard_network_prefix,
    });
    cfg.save()?;

    println!("Saved fabric config to {}", Config::path().display());
    println!(
        "`iso vyos build`, `iso vyos flavor`, and `iso proxmox build` will use these values \
         automatically -- pass a flag explicitly on any of them to override just that one field."
    );
    Ok(())
}

async fn apply_vyos(args: VyosApplyArgs) -> Result<()> {
    let commands = crate::iso::vyos_apply::read_commands_from_script(&args.script)?;
    println!(
        "Applying {} commands from {} to {}@{}:{} ...",
        commands.len(),
        args.script.display(),
        args.user,
        args.host,
        args.port
    );

    let cfg = crate::iso::vyos_apply::VyosApplyConfig {
        host: args.host,
        port: args.port,
        user: args.user,
        ssh_key: args.ssh_key,
        commands,
        strict_host_key_checking: !args.insecure_skip_host_key_check,
    };

    crate::iso::vyos_apply::apply(&cfg).await?;
    println!("Applied and saved.");
    Ok(())
}

/// Directory local per-admin WireGuard private keys are cached in --
/// alongside `Config::path()` itself, not under `--out` (a typically
/// throwaway/rebuildable build directory): losing a client private key
/// means that admin has to be re-added from scratch, so it belongs with
/// the other persistent local secrets this CLI already caches
/// (`fleet_secret`, the WireGuard server key).
fn wireguard_client_key_dir() -> Result<PathBuf> {
    let dir = Config::path()
        .parent()
        .context("Config::path() must have a parent directory")?
        .join("wireguard");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

async fn add_wireguard_peer(args: WireguardAddPeerArgs) -> Result<()> {
    let cfg = Config::load()?;
    let fabric = cfg.fabric.as_ref().context(
        "no fabric config found -- run `crow iso fabric-configure` first, so this command \
         knows which networks (mgmt/underlay) to route through the tunnel",
    )?;
    let server_private_key = cfg.wireguard_server_private_key.clone().context(
        "no WireGuard server key cached locally -- run `crow iso vyos build` with \
         --wireguard-address set first (on this machine, or copy its \
         ~/.config/crow/config.json here)",
    )?;

    let client_private_key = crate::iso::wireguard::genkey()?;
    let client_public_key = crate::iso::wireguard::pubkey(&client_private_key)?;
    let key_path = wireguard_client_key_dir()?.join(format!("{}.key", args.name));
    std::fs::write(&key_path, &client_private_key)?;
    println!("Wrote {} (keep this private)", key_path.display());

    let commands = crate::iso::wireguard::render_add_peer(
        &args.name,
        &client_public_key,
        &args.client_address,
    );
    let apply_cfg = crate::iso::vyos_apply::VyosApplyConfig {
        host: args.host.clone(),
        port: args.port,
        user: args.user,
        ssh_key: args.ssh_key,
        commands,
        strict_host_key_checking: !args.insecure_skip_host_key_check,
    };
    crate::iso::vyos_apply::apply(&apply_cfg).await?;
    println!("Pushed peer '{}' to VyOS.", args.name);

    let server_public_key = crate::iso::wireguard::pubkey(&server_private_key)?;
    println!(
        "\nClient config for '{name}' -- save as {name}.conf and `wg-quick up ./{name}.conf`:\n\n\
         [Interface]\n\
         PrivateKey = {client_private_key}\n\
         Address = {client_address}/32\n\
         \n\
         [Peer]\n\
         PublicKey = {server_public_key}\n\
         Endpoint = {host}:{wireguard_port}\n\
         AllowedIPs = {mgmt_network}/{mgmt_prefix},{underlay_network}/{underlay_prefix}\n\
         PersistentKeepalive = 25",
        name = args.name,
        client_address = args.client_address,
        host = args.host,
        wireguard_port = args.wireguard_port,
        mgmt_network = fabric.mgmt_network,
        mgmt_prefix = fabric.mgmt_network_prefix,
        underlay_network = fabric.underlay_network,
        underlay_prefix = fabric.underlay_network_prefix,
    );
    Ok(())
}

async fn remove_wireguard_peer(args: WireguardRemovePeerArgs) -> Result<()> {
    let commands = crate::iso::wireguard::render_remove_peer(&args.name);
    let apply_cfg = crate::iso::vyos_apply::VyosApplyConfig {
        host: args.host,
        port: args.port,
        user: args.user,
        ssh_key: args.ssh_key,
        commands,
        strict_host_key_checking: !args.insecure_skip_host_key_check,
    };
    crate::iso::vyos_apply::apply(&apply_cfg).await?;
    println!(
        "Removed peer '{}' from VyOS. Its local private key file (if any, under {}) \
         was left in place -- delete it by hand if you're sure it's no longer needed.",
        args.name,
        wireguard_client_key_dir()?.display()
    );
    Ok(())
}

/// Shared by `build_vyos` and `flavor_vyos` -- both need the exact same
/// prompts, plus generating/caching the server key and printing its
/// public half, so unlike the smaller two-field mgmt-port blocks nearby
/// this is a real shared helper rather than duplicated inline.
/// `address` presence after prompting is what enables WireGuard at all
/// (matches `crow_api_mgmt_ip`'s own "leave blank to skip" convention);
/// `Ok((None, None, None, None))` means skipped.
#[allow(clippy::type_complexity)]
fn resolve_wireguard_config(
    port: Option<u16>,
    address: Option<String>,
    address_prefix: Option<u8>,
    fabric: Option<&FabricConfig>,
) -> Result<(Option<u16>, Option<String>, Option<u8>, Option<String>)> {
    use crate::iso::vyos_wizard as wiz;

    let network_hint = fabric
        .and_then(|f| f.wireguard_network.as_ref())
        .map(|n| format!(" (fabric's configured VPN subnet: {n})"))
        .unwrap_or_default();
    let address = wiz::prompt_optional(
        address,
        &format!(
            "WireGuard VPN server address, this router's own{network_hint} -- leave blank to \
             skip admin VPN access entirely"
        ),
    )?;
    let Some(address) = address else {
        return Ok((None, None, None, None));
    };

    let port = wiz::prompt(port, "WireGuard VPN listen port", Some(51820))?;
    let prefix = wiz::prompt(
        address_prefix,
        "WireGuard VPN subnet prefix length",
        fabric.and_then(|f| f.wireguard_network_prefix).or(Some(24)),
    )?;

    let private_key = Config::wireguard_server_key_or_generate()?;
    let public_key = crate::iso::wireguard::pubkey(&private_key)?;
    println!(
        "WireGuard server public key (needed for every `iso vyos wireguard add-peer` \
         client config): {public_key}"
    );

    Ok((Some(port), Some(address), Some(prefix), Some(private_key)))
}

fn build_vyos(args: VyosBuildArgs) -> Result<()> {
    use crate::iso::vyos_wizard as wiz;
    let fabric = Config::load()?.fabric;

    let hostname = wiz::prompt(args.hostname, "Hostname", None)?;
    let trunk_interface = wiz::prompt(
        args.trunk_interface,
        "Trunk interface (fabric NIC, e.g. eth1)",
        None,
    )?;
    let uplink_interface = wiz::prompt(
        args.uplink_interface,
        "Uplink interface (internet/LAN NIC, e.g. eth2)",
        None,
    )?;
    let trunk_mtu = wiz::prompt(
        args.trunk_mtu,
        "Trunk MTU",
        fabric.as_ref().map(|f| f.trunk_mtu).or(Some(9000)),
    )?;

    let (trunk_speed, trunk_duplex) = match (args.trunk_speed, args.trunk_duplex) {
        (Some(s), Some(d)) => (Some(s), Some(d)),
        _ => {
            if wiz::prompt_bool(
                None,
                "Pin the trunk to a fixed speed instead of auto-negotiation?",
                false,
            )? {
                (
                    Some(wiz::prompt(
                        None,
                        "Trunk speed (10/100/1000/2500/5000/10000/...)",
                        Some("1000".to_string()),
                    )?),
                    Some(wiz::prompt(
                        None,
                        "Trunk duplex (full/half)",
                        Some("full".to_string()),
                    )?),
                )
            } else {
                (None, None)
            }
        }
    };

    let underlay_vlan = wiz::prompt(
        args.underlay_vlan,
        "Underlay VLAN ID",
        fabric.as_ref().map(|f| f.underlay_vlan),
    )?;
    let underlay_ip = wiz::prompt(
        args.underlay_ip,
        "Underlay loopback-facing IP (this router's own) -- also the BGP \
         route-reflector address every Proxmox host will peer with",
        fabric.as_ref().map(|f| f.bgp_route_reflector_ip.clone()),
    )?;
    let underlay_prefix = wiz::prompt(
        args.underlay_prefix,
        "Underlay prefix length",
        fabric
            .as_ref()
            .map(|f| f.underlay_network_prefix)
            .or(Some(24)),
    )?;
    let mgmt_vlan = wiz::prompt(
        args.mgmt_vlan,
        "Management VLAN ID",
        fabric.as_ref().map(|f| f.mgmt_vlan),
    )?;
    // VyOS's own mgmt IP *is* the fabric's mgmt_gateway (every Proxmox
    // host's default gateway on that VLAN points at it), so the stored
    // fabric config's mgmt_gateway is exactly the right default here.
    let mgmt_ip = wiz::prompt(
        args.mgmt_ip,
        "Management IP (this router's own)",
        fabric.as_ref().map(|f| f.mgmt_gateway.clone()),
    )?;
    let mgmt_prefix = wiz::prompt(args.mgmt_prefix, "Management prefix length", Some(24))?;
    let mgmt_network = wiz::prompt(
        args.mgmt_network,
        "Management subnet network address (not this router's own IP)",
        fabric.as_ref().map(|f| f.mgmt_network.clone()),
    )?;
    let mgmt_network_prefix = wiz::prompt(
        args.mgmt_network_prefix,
        "Management subnet prefix length",
        fabric.as_ref().map(|f| f.mgmt_network_prefix).or(Some(24)),
    )?;
    let loopback_ip = wiz::prompt(
        args.loopback_ip,
        "Loopback IP (VTEP source / BGP router-id)",
        None,
    )?;

    let uplink_dhcp = wiz::prompt_bool(args.uplink_dhcp, "Use DHCP for the uplink?", false)?;
    let (uplink_ip, uplink_prefix, uplink_gateway) = if uplink_dhcp {
        if args.uplink_ip.is_some() || args.uplink_prefix.is_some() || args.uplink_gateway.is_some()
        {
            bail!(
                "--uplink-dhcp is incompatible with --uplink-ip/--uplink-prefix/--uplink-gateway"
            );
        }
        (None, None, None)
    } else {
        (
            Some(wiz::prompt(args.uplink_ip, "Uplink IP", None)?),
            Some(wiz::prompt(
                args.uplink_prefix,
                "Uplink prefix length",
                Some(24),
            )?),
            wiz::prompt_optional(args.uplink_gateway, "Uplink gateway")?,
        )
    };

    let ospf_area = wiz::prompt(
        args.ospf_area,
        "OSPF area",
        fabric
            .as_ref()
            .map(|f| f.ospf_area.clone())
            .or(Some("0".to_string())),
    )?;
    let underlay_network = wiz::prompt(
        args.underlay_network,
        "Underlay network address",
        fabric.as_ref().map(|f| f.underlay_network.clone()),
    )?;
    let underlay_network_prefix = wiz::prompt(
        args.underlay_network_prefix,
        "Underlay network prefix length",
        fabric
            .as_ref()
            .map(|f| f.underlay_network_prefix)
            .or(Some(24)),
    )?;

    let ssh_pubkey_path = wiz::prompt_path(args.ssh_pubkey, "Path to SSH public key file")?;
    let ssh_pubkey = std::fs::read_to_string(&ssh_pubkey_path)
        .with_context(|| format!("reading SSH public key at {}", ssh_pubkey_path.display()))?
        .trim()
        .to_string();

    let bgp_asn = wiz::prompt(
        args.bgp_asn,
        "BGP ASN",
        fabric.as_ref().map(|f| f.bgp_asn).or(Some(65000)),
    )?;
    let bgp_peer_password = wiz::prompt_secret_optional(
        args.bgp_peer_password
            .or(fabric.as_ref().and_then(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
    )?;
    let dns_servers = wiz::prompt_list(
        args.dns_servers
            .or(fabric.as_ref().map(|f| f.dns_servers.clone())),
        "DNS forwarders for mgmt-VLAN hosts (comma-separated)",
        Some(&["8.8.8.8".to_string(), "8.8.4.4".to_string()]),
    )?;
    let allow_password_auth = wiz::prompt_bool(
        args.allow_password_auth,
        "Keep SSH password auth enabled alongside the key? (recovery fallback, not the default)",
        false,
    )?;
    let crow_api_mgmt_ip = wiz::prompt_optional(
        args.crow_api_mgmt_ip,
        "crowCloud control plane mgmt IP (leave blank if none yet -- skips both forwarding rules below entirely)",
    )?;
    let crow_api_mgmt_port = if crow_api_mgmt_ip.is_some() {
        Some(wiz::prompt(
            args.crow_api_mgmt_port,
            "crowCloud API port to forward from the uplink -- the Helm \
             chart's default API NodePort is 30081, not the API container's own internal 8080",
            Some(30081),
        )?)
    } else {
        None
    };
    let crow_frontend_mgmt_port = if crow_api_mgmt_ip.is_some() {
        Some(wiz::prompt(
            args.crow_frontend_mgmt_port,
            "crowCloud web frontend port to forward from the uplink -- the Helm chart's default \
             frontend NodePort is 30080",
            Some(30080),
        )?)
    } else {
        None
    };
    let (wireguard_port, wireguard_address, wireguard_address_prefix, wireguard_private_key) =
        resolve_wireguard_config(
            args.wireguard_port,
            args.wireguard_address,
            args.wireguard_address_prefix,
            fabric.as_ref(),
        )?;

    let cfg = vyos_iso::VyosBuildConfig {
        hostname,
        trunk_interface,
        uplink_interface,
        trunk_mtu,
        trunk_speed,
        trunk_duplex,
        underlay_vlan,
        underlay_ip,
        underlay_prefix,
        mgmt_vlan,
        mgmt_ip,
        mgmt_prefix,
        mgmt_network,
        mgmt_network_prefix,
        loopback_ip,
        uplink_dhcp,
        uplink_ip,
        uplink_prefix,
        uplink_gateway,
        ospf_area,
        underlay_network,
        underlay_network_prefix,
        ssh_pubkey,
        bgp_asn,
        bgp_peer_password,
        dns_servers,
        allow_password_auth,
        crow_api_mgmt_ip,
        crow_api_mgmt_port,
        crow_frontend_mgmt_port,
        wireguard_port,
        wireguard_address,
        wireguard_address_prefix,
        wireguard_private_key,
    };

    std::fs::create_dir_all(&args.out)?;
    let script_path = args.out.join("configure.txt");
    std::fs::write(&script_path, vyos_iso::render_configure_script(&cfg))?;
    println!("Wrote VyOS configure script to {}", script_path.display());

    if args.render_only {
        return Ok(());
    }

    if which("vyos-build").is_none() {
        println!(
            "vyos-build not found on PATH -- skipping image build. \
             Apply {} manually via `configure < {}` on a fresh VyOS install, \
             or install vyos-build to produce a baked image (#63).",
            script_path.display(),
            script_path.display()
        );
        return Ok(());
    }

    bail!(
        "vyos-build was found on PATH, but this tool doesn't yet drive its \
         flavor system end-to-end -- render_only produced {}, which needs \
         to be wired into a vyos-build flavor by hand for now (#66 tracks \
         finishing this integration)",
        script_path.display()
    );
}

fn flavor_vyos(args: VyosFlavorArgs) -> Result<()> {
    use crate::iso::vyos_wizard as wiz;
    let fabric = Config::load()?.fabric;

    let hostname = wiz::prompt(args.hostname, "Hostname", None)?;
    let trunk_mtu = wiz::prompt(
        args.trunk_mtu,
        "Trunk MTU",
        fabric.as_ref().map(|f| f.trunk_mtu).or(Some(9000)),
    )?;

    let (trunk_speed, trunk_duplex) = match (args.trunk_speed, args.trunk_duplex) {
        (Some(s), Some(d)) => (Some(s), Some(d)),
        _ => {
            if wiz::prompt_bool(
                None,
                "Pin the trunk to a fixed speed instead of auto-negotiation?",
                false,
            )? {
                (
                    Some(wiz::prompt(
                        None,
                        "Trunk speed (10/100/1000/2500/5000/10000/...)",
                        Some("1000".to_string()),
                    )?),
                    Some(wiz::prompt(
                        None,
                        "Trunk duplex (full/half)",
                        Some("full".to_string()),
                    )?),
                )
            } else {
                (None, None)
            }
        }
    };

    let underlay_vlan = wiz::prompt(
        args.underlay_vlan,
        "Underlay VLAN ID",
        fabric.as_ref().map(|f| f.underlay_vlan),
    )?;
    let underlay_ip = wiz::prompt(
        args.underlay_ip,
        "Underlay loopback-facing IP (this router's own) -- also the BGP \
         route-reflector address every Proxmox host will peer with",
        fabric.as_ref().map(|f| f.bgp_route_reflector_ip.clone()),
    )?;
    let underlay_prefix = wiz::prompt(
        args.underlay_prefix,
        "Underlay prefix length",
        fabric
            .as_ref()
            .map(|f| f.underlay_network_prefix)
            .or(Some(24)),
    )?;
    let mgmt_vlan = wiz::prompt(
        args.mgmt_vlan,
        "Management VLAN ID",
        fabric.as_ref().map(|f| f.mgmt_vlan),
    )?;
    // VyOS's own mgmt IP *is* the fabric's mgmt_gateway (every Proxmox
    // host's default gateway on that VLAN points at it).
    let mgmt_ip = wiz::prompt(
        args.mgmt_ip,
        "Management IP (this router's own)",
        fabric.as_ref().map(|f| f.mgmt_gateway.clone()),
    )?;
    let mgmt_prefix = wiz::prompt(args.mgmt_prefix, "Management prefix length", Some(24))?;
    let mgmt_network = wiz::prompt(
        args.mgmt_network,
        "Management subnet network address (not this router's own IP)",
        fabric.as_ref().map(|f| f.mgmt_network.clone()),
    )?;
    let mgmt_network_prefix = wiz::prompt(
        args.mgmt_network_prefix,
        "Management subnet prefix length",
        fabric.as_ref().map(|f| f.mgmt_network_prefix).or(Some(24)),
    )?;
    let loopback_ip = wiz::prompt(
        args.loopback_ip,
        "Loopback IP (VTEP source / BGP router-id)",
        None,
    )?;

    let uplink_dhcp = wiz::prompt_bool(args.uplink_dhcp, "Use DHCP for the uplink?", false)?;
    let (uplink_ip, uplink_prefix, uplink_gateway) = if uplink_dhcp {
        if args.uplink_ip.is_some() || args.uplink_prefix.is_some() || args.uplink_gateway.is_some()
        {
            bail!(
                "--uplink-dhcp is incompatible with --uplink-ip/--uplink-prefix/--uplink-gateway"
            );
        }
        (None, None, None)
    } else {
        (
            Some(wiz::prompt(args.uplink_ip, "Uplink IP", None)?),
            Some(wiz::prompt(
                args.uplink_prefix,
                "Uplink prefix length",
                Some(24),
            )?),
            wiz::prompt_optional(args.uplink_gateway, "Uplink gateway")?,
        )
    };

    let ospf_area = wiz::prompt(
        args.ospf_area,
        "OSPF area",
        fabric
            .as_ref()
            .map(|f| f.ospf_area.clone())
            .or(Some("0".to_string())),
    )?;
    let underlay_network = wiz::prompt(
        args.underlay_network,
        "Underlay network address",
        fabric.as_ref().map(|f| f.underlay_network.clone()),
    )?;
    let underlay_network_prefix = wiz::prompt(
        args.underlay_network_prefix,
        "Underlay network prefix length",
        fabric
            .as_ref()
            .map(|f| f.underlay_network_prefix)
            .or(Some(24)),
    )?;

    let ssh_pubkey_path = wiz::prompt_path(args.ssh_pubkey, "Path to SSH public key file")?;
    let ssh_pubkey = std::fs::read_to_string(&ssh_pubkey_path)
        .with_context(|| format!("reading SSH public key at {}", ssh_pubkey_path.display()))?
        .trim()
        .to_string();

    let bgp_asn = wiz::prompt(
        args.bgp_asn,
        "BGP ASN",
        fabric.as_ref().map(|f| f.bgp_asn).or(Some(65000)),
    )?;
    let bgp_peer_password = wiz::prompt_secret_optional(
        args.bgp_peer_password
            .or(fabric.as_ref().and_then(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
    )?;
    let dns_servers = wiz::prompt_list(
        args.dns_servers
            .or(fabric.as_ref().map(|f| f.dns_servers.clone())),
        "DNS forwarders for mgmt-VLAN hosts (comma-separated)",
        Some(&["8.8.8.8".to_string(), "8.8.4.4".to_string()]),
    )?;
    let allow_password_auth = wiz::prompt_bool(
        args.allow_password_auth,
        "Keep SSH password auth enabled alongside the key? (recovery fallback, not the default)",
        false,
    )?;
    let crow_api_mgmt_ip = wiz::prompt_optional(
        args.crow_api_mgmt_ip,
        "crowCloud control plane mgmt IP (leave blank if none yet -- skips both forwarding rules below entirely)",
    )?;
    let crow_api_mgmt_port = if crow_api_mgmt_ip.is_some() {
        Some(wiz::prompt(
            args.crow_api_mgmt_port,
            "crowCloud API port to forward from the uplink -- the Helm \
             chart's default API NodePort is 30081, not the API container's own internal 8080",
            Some(30081),
        )?)
    } else {
        None
    };
    let crow_frontend_mgmt_port = if crow_api_mgmt_ip.is_some() {
        Some(wiz::prompt(
            args.crow_frontend_mgmt_port,
            "crowCloud web frontend port to forward from the uplink -- the Helm chart's default \
             frontend NodePort is 30080",
            Some(30080),
        )?)
    } else {
        None
    };
    let (wireguard_port, wireguard_address, wireguard_address_prefix, wireguard_private_key) =
        resolve_wireguard_config(
            args.wireguard_port,
            args.wireguard_address,
            args.wireguard_address_prefix,
            fabric.as_ref(),
        )?;

    let cfg = vyos_flavor_iso::VyosFlavorConfig {
        base: vyos_iso::VyosBuildConfig {
            hostname,
            // Ignored by the flavor renderer -- it detects the real
            // interface names live at boot instead.
            trunk_interface: String::new(),
            uplink_interface: String::new(),
            trunk_mtu,
            trunk_speed,
            trunk_duplex,
            underlay_vlan,
            underlay_ip,
            underlay_prefix,
            mgmt_vlan,
            mgmt_ip,
            mgmt_prefix,
            mgmt_network,
            mgmt_network_prefix,
            loopback_ip,
            uplink_dhcp,
            uplink_ip,
            uplink_prefix,
            uplink_gateway,
            ospf_area,
            underlay_network,
            underlay_network_prefix,
            ssh_pubkey,
            bgp_asn,
            bgp_peer_password,
            dns_servers,
            allow_password_auth,
            crow_api_mgmt_ip,
            crow_api_mgmt_port,
            crow_frontend_mgmt_port,
            wireguard_port,
            wireguard_address,
            wireguard_address_prefix,
            wireguard_private_key,
        },
    };

    std::fs::create_dir_all(&args.out)?;
    let script_path = args.out.join("crowcloud-fabric-init.sh");
    std::fs::write(
        &script_path,
        vyos_flavor_iso::render_fabric_init_script(&cfg),
    )?;
    let flavor_path = args.out.join("crowcloud.toml");
    std::fs::write(&flavor_path, vyos_flavor_iso::render_flavor_toml(&cfg))?;

    println!("Wrote {}", script_path.display());
    println!("Wrote {}", flavor_path.display());
    println!(
        "\nTo build the ISO, run (requires Docker with privileged-container support):\n\
         \n\
         git clone -b rolling --single-branch https://github.com/vyos/vyos-build\n\
         cp {flavor} vyos-build/data/build-flavors/crowcloud.toml\n\
         cd vyos-build\n\
         docker run --rm -it --privileged -v $(pwd):/vyos -w /vyos vyos/vyos-build:rolling \\\n\
         \x20 sudo ./build-vyos-image --architecture amd64 --build-by crowcloud crowcloud\n\
         \n\
         Nothing runs this automatically -- once the built image is installed and \
         the box is up, SSH in and run it by hand (it's baked in with no execute \
         bit, so invoke it via `bash`, not `./`):\n\
         \n\
         \x20 ssh vyos@<box>\n\
         \x20 sudo bash /usr/local/bin/crowcloud-fabric-init.sh",
        flavor = flavor_path.display()
    );

    Ok(())
}

/// `--disk` and `--disk-filter` are mutually exclusive (enforced by
/// clap's `conflicts_with` for the flag case); when neither is given,
/// prompts interactively for which mode to use rather than defaulting
/// to one, since a filter broad/narrow enough to correctly select
/// every intended disk depends entirely on the target hardware --
/// there's no safe default to fall back to (see `DiskSelection`'s doc
/// comment).
/// `--disk`/`--disk-filter` stay available for scripting/edge cases,
/// but the wizard no longer asks which mode to use or what criteria to
/// type -- when neither flag is given it defaults straight to
/// `DEVNAME = "*"`, i.e. "the first real disk". Confirmed safe against
/// Proxmox's own disk-enumeration source (`Proxmox::Sys::Block::hd_list()`):
/// the candidate pool it draws from already excludes loop/dm/md/ram/
/// optical devices (by name), requires `DEVTYPE=disk` (excludes
/// partitions), and excludes the boot/live medium via filesystem-type
/// detection (`ID_FS_TYPE=iso9660`) -- independent of bus type, so
/// there's no need to filter out USB specifically. The one thing this
/// can't control is *which* real disk gets picked if more than one is
/// present (not size- or identity-aware) -- that's what `hdsize`
/// (prompted separately, always) is for: it doesn't matter which disk
/// lands the OS, since its footprint is capped and every other disk
/// is left untouched regardless.
fn resolve_disk_selection(
    disk: Option<Vec<String>>,
    disk_filter: Option<Vec<String>>,
    disk_filter_match: Option<String>,
) -> Result<proxmox_iso::DiskSelection> {
    if let Some(disks) = disk {
        return Ok(proxmox_iso::DiskSelection::List(disks));
    }
    if let Some(pairs) = disk_filter {
        return Ok(proxmox_iso::DiskSelection::Filter {
            filter: parse_disk_filter_pairs(&pairs)?,
            filter_match: disk_filter_match,
        });
    }
    Ok(proxmox_iso::DiskSelection::Filter {
        filter: vec![("DEVNAME".to_string(), "*".to_string())],
        filter_match: Some("any".to_string()),
    })
}

fn parse_disk_filter_pairs(pairs: &[String]) -> Result<Vec<(String, String)>> {
    pairs
        .iter()
        .map(|pair| {
            let (key, value) = pair.split_once('=').with_context(|| {
                format!("invalid disk filter '{pair}' -- expected KEY=value, e.g. ID_BUS=ata")
            })?;
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

fn build_proxmox(args: ProxmoxBuildArgs) -> Result<()> {
    use crate::iso::vyos_wizard as wiz;
    let fabric = Config::load()?.fabric;

    let root_password = wiz::prompt_secret(args.root_password, "Root password")?;
    let root_password_hash = hash_password(&root_password)?;
    let fqdn = wiz::prompt(args.fqdn, "FQDN", None)?;
    let admin_email = wiz::prompt(args.admin_email, "Admin email", None)?;
    let trunk_interface = wiz::prompt(
        args.trunk_interface,
        "Trunk interface (fabric NIC, e.g. eno1)",
        None,
    )?;
    let underlay_ip = wiz::prompt(
        args.underlay_ip,
        "Underlay IP (this host's own -- no allocator exists yet, so pick \
         one inside the fabric's underlay subnet by hand, e.g. 10.10.0.11)",
        None,
    )?;
    let mgmt_ip = wiz::prompt(args.mgmt_ip, "Management IP (this host's own)", None)?;
    let mgmt_prefix = wiz::prompt(args.mgmt_prefix, "Management prefix length", Some(24))?;
    let disk_selection =
        resolve_disk_selection(args.disk, args.disk_filter, args.disk_filter_match)?;
    let hdsize_gib = Some(wiz::prompt(
        args.hdsize_gib,
        "GiB for the OS disk (the rest of whatever disk gets picked, plus every other \
         disk, is left untouched for storage pools created later)",
        Some(150.0),
    )?);
    let crow_api_url = wiz::prompt(
        args.crow_api_url,
        "crowCloud API URL (where the post-install hook checks reachability before self-electing as fleet seed)",
        None,
    )?;
    let fleet_secret = match args.fleet_secret {
        Some(s) => s,
        None => Config::fleet_secret_or_generate()?,
    };

    let underlay_vlan = wiz::prompt(
        args.underlay_vlan,
        "Underlay VLAN ID",
        fabric.as_ref().map(|f| f.underlay_vlan),
    )?;
    let mgmt_vlan = wiz::prompt(
        args.mgmt_vlan,
        "Management VLAN ID",
        fabric.as_ref().map(|f| f.mgmt_vlan),
    )?;
    let mgmt_gateway = wiz::prompt(
        args.mgmt_gateway,
        "Management gateway IP (VyOS's own IP on the mgmt VLAN)",
        fabric.as_ref().map(|f| f.mgmt_gateway.clone()),
    )?;
    let trunk_mtu = wiz::prompt(
        args.trunk_mtu,
        "Trunk MTU",
        fabric.as_ref().map(|f| f.trunk_mtu).or(Some(9000)),
    )?;
    let bgp_asn = wiz::prompt(
        args.bgp_asn,
        "BGP ASN",
        fabric.as_ref().map(|f| f.bgp_asn).or(Some(65000)),
    )?;
    let bgp_route_reflector_ip = wiz::prompt(
        args.bgp_route_reflector_ip,
        "VyOS's own underlay IP (the BGP route-reflector this host will peer with)",
        fabric.as_ref().map(|f| f.bgp_route_reflector_ip.clone()),
    )?;
    let underlay_prefix = wiz::prompt(
        args.underlay_prefix,
        "Underlay prefix length",
        fabric
            .as_ref()
            .map(|f| f.underlay_network_prefix)
            .or(Some(24)),
    )?;
    let ospf_area = wiz::prompt(
        args.ospf_area,
        "OSPF area",
        fabric
            .as_ref()
            .map(|f| f.ospf_area.clone())
            .or(Some("0".to_string())),
    )?;
    let seed_ssh_pubkey = args
        .seed_ssh_pubkey
        .map(|path| {
            std::fs::read_to_string(&path)
                .with_context(|| format!("reading seed SSH public key at {}", path.display()))
                .map(|s| s.trim().to_string())
        })
        .transpose()?;
    let vyos_ssh_private_key = args
        .vyos_ssh_private_key
        .map(|path| {
            std::fs::read_to_string(&path)
                .with_context(|| format!("reading VyOS SSH private key at {}", path.display()))
        })
        .transpose()?;

    let cfg = proxmox_iso::ProxmoxBuildConfig {
        root_password_hash,
        fqdn,
        admin_email,
        trunk_interface,
        underlay_vlan,
        underlay_ip,
        mgmt_vlan,
        mgmt_ip,
        mgmt_prefix,
        mgmt_gateway,
        trunk_mtu,
        disk_selection,
        hdsize_gib,
        zfs_raid: args.zfs_raid,
        crow_api_url,
        fleet_secret,
        bgp_asn,
        bgp_route_reflector_ip,
        underlay_prefix,
        ospf_area,
        seed_ssh_pubkey,
        vyos_uplink_interface: args.vyos_uplink_interface,
        vyos_ssh_private_key,
    };

    std::fs::create_dir_all(&args.out)?;
    let answer_path = args.out.join("answer.toml");
    let hook_path = args.out.join("post-install-hook.sh");
    std::fs::write(&answer_path, proxmox_iso::render_answer_toml(&cfg))?;
    std::fs::write(&hook_path, proxmox_iso::render_post_install_hook(&cfg))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))?;
    }
    println!("Wrote {}", answer_path.display());
    println!("Wrote {}", hook_path.display());

    if args.render_only {
        return Ok(());
    }

    let Some(base_iso) = args.base_iso else {
        bail!("--base-iso is required to build an image (omit and pass --render-only to just generate config)");
    };
    if !base_iso.exists() {
        bail!("--base-iso {} does not exist", base_iso.display());
    }

    if which("proxmox-auto-install-assistant").is_none() {
        println!(
            "proxmox-auto-install-assistant not found on PATH -- skipping \
             image build. Install it and re-run, or use {} manually.",
            answer_path.display(),
        );
        return Ok(());
    }

    let output_iso = args.out.join("proxmox-auto.iso");
    // No `--on-first-boot` -- it didn't reliably fire in practice, so
    // the post-install hook is no longer bundled as an automatic
    // first-boot step (see `proxmox::render_answer_toml`'s doc comment).
    // This ISO only automates the base install; `hook_path` still needs
    // to be copied onto the box and run by hand afterward.
    let status = Command::new("proxmox-auto-install-assistant")
        .arg("prepare-iso")
        .arg(&base_iso)
        .arg("--fetch-from")
        .arg("iso")
        .arg("--answer-file")
        .arg(&answer_path)
        .arg("--output")
        .arg(&output_iso)
        .status()
        .context("running proxmox-auto-install-assistant")?;

    if !status.success() {
        bail!("proxmox-auto-install-assistant exited with {status}");
    }

    println!("Built {}", output_iso.display());
    println!(
        "This ISO automates the base install only. Once it's up, copy {} \
         onto the box and run it by hand over SSH:\n\
         \x20   scp {} root@<box>:/root/\n\
         \x20   ssh root@<box> bash /root/{}",
        hook_path.display(),
        hook_path.display(),
        hook_path.file_name().unwrap().to_string_lossy()
    );
    Ok(())
}

fn hash_password(plaintext: &str) -> Result<String> {
    let output = Command::new("openssl")
        .arg("passwd")
        .arg("-6")
        .arg("-stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(plaintext.as_bytes())?;
            }
            child.wait_with_output()
        })
        .context("hashing root password via `openssl passwd -6`")?;

    if !output.status.success() {
        bail!("openssl passwd -6 exited with {}", output.status);
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(bin))
            .find(|p| p.is_file())
    })
}
