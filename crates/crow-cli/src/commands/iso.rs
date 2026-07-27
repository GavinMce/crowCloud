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
    #[arg(long, value_delimiter = ',')]
    pub dns_servers: Option<Vec<String>>,
}

#[derive(Args)]
pub struct VyosCmd {
    #[command(subcommand)]
    pub command: VyosSubcommand,
}

#[derive(Subcommand)]
pub enum VyosSubcommand {
    Build(Box<VyosBuildArgs>),
    /// Apply a rendered configure.txt to a live VyOS device over SSH
    /// (#66) -- VyOS has no unattended install mode, so a router still
    /// needs one interactive `install image` session per box; this
    /// automates the fabric-config step that comes after that
    Apply(VyosApplyArgs),
    /// Render a vyos-build flavor that bakes fabric config directly
    /// into a custom VyOS image (#63) -- the resulting ISO configures
    /// itself on boot, no `iso vyos apply` step needed for a redeploy
    /// of hardware whose PCI addressing is already known. Still
    /// doesn't touch the `install image` step itself -- VyOS has no
    /// unattended install mode regardless of image customization
    Flavor(Box<VyosFlavorArgs>),
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
    /// PCI bus address of the fabric trunk NIC (e.g. 0000:01:00.0),
    /// read via `readlink -f /sys/class/net/<iface>/device` on the
    /// target box -- stable across a NIC swap in the same physical
    /// slot, unlike the kernel-assigned interface name
    #[arg(long)]
    pub trunk_pci: Option<String>,
    #[arg(long)]
    pub uplink_pci: Option<String>,
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
    /// Directory to write the rendered fabric-init script, cron entry,
    /// and vyos-build flavor TOML into
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
    pub root_password: String,
    #[arg(long)]
    pub fqdn: String,
    /// Required by answer.toml's [global] section -- confirmed live
    /// against proxmox-auto-install-assistant, no default offered since
    /// there's no sensible one
    #[arg(long)]
    pub admin_email: String,
    #[arg(long)]
    pub trunk_interface: String,
    /// Falls back to the shared fabric config (`iso fabric-configure`)
    /// if omitted
    #[arg(long)]
    pub underlay_vlan: Option<u16>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub mgmt_vlan: Option<u16>,
    #[arg(long)]
    pub mgmt_ip: String,
    #[arg(long, default_value_t = 24)]
    pub mgmt_prefix: u8,
    /// This host's default gateway on the mgmt VLAN -- falls back to
    /// the shared fabric config's `mgmt_gateway` (VyOS's own mgmt IP)
    /// if omitted
    #[arg(long)]
    pub mgmt_gateway: Option<String>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub trunk_mtu: Option<u32>,
    #[arg(long, value_delimiter = ',')]
    pub disk: Vec<String>,
    #[arg(long)]
    pub zfs_raid: Option<String>,
    /// Where the post-install hook looks for a reachable crowCloud
    /// instance before self-electing as the fleet seed (#67)
    #[arg(long)]
    pub crow_api_url: String,
    /// Baked into the image as the self-registration credential.
    /// Defaults to the locally cached fleet secret, generating one on
    /// first use if none exists yet -- no crowCloud login required
    /// (#67's bootstrap case)
    #[arg(long)]
    pub fleet_secret: Option<String>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub bgp_asn: Option<u32>,
    /// Falls back to the shared fabric config if omitted
    #[arg(long)]
    pub bgp_peer_password: Option<String>,
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
            VyosSubcommand::Build(args) => build_vyos(*args),
            VyosSubcommand::Apply(args) => apply_vyos(args).await,
            VyosSubcommand::Flavor(args) => flavor_vyos(*args),
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
    let bgp_peer_password = wiz::prompt_secret(
        args.bgp_peer_password
            .or(existing.as_ref().map(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
    )?;
    let dns_servers = wiz::prompt_list(
        args.dns_servers
            .or(existing.as_ref().map(|f| f.dns_servers.clone())),
        "DNS forwarders for mgmt-VLAN hosts (comma-separated)",
        &["8.8.8.8".to_string(), "8.8.4.4".to_string()],
    )?;

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
        dns_servers,
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
        "Underlay loopback-facing IP (this router's own)",
        None,
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
    let bgp_peer_password = wiz::prompt_secret(
        args.bgp_peer_password
            .or(fabric.as_ref().map(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
    )?;
    let dns_servers = wiz::prompt_list(
        args.dns_servers
            .or(fabric.as_ref().map(|f| f.dns_servers.clone())),
        "DNS forwarders for mgmt-VLAN hosts (comma-separated)",
        &["8.8.8.8".to_string(), "8.8.4.4".to_string()],
    )?;
    let allow_password_auth = wiz::prompt_bool(
        args.allow_password_auth,
        "Keep SSH password auth enabled alongside the key? (recovery fallback, not the default)",
        false,
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
    let trunk_pci = wiz::prompt(
        args.trunk_pci,
        "Trunk NIC PCI bus address (e.g. 0000:01:00.0 -- readlink -f /sys/class/net/<iface>/device on the target box)",
        None,
    )?;
    let uplink_pci = wiz::prompt(args.uplink_pci, "Uplink NIC PCI bus address", None)?;
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
        "Underlay loopback-facing IP (this router's own)",
        None,
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
    let bgp_peer_password = wiz::prompt_secret(
        args.bgp_peer_password
            .or(fabric.as_ref().map(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
    )?;
    let dns_servers = wiz::prompt_list(
        args.dns_servers
            .or(fabric.as_ref().map(|f| f.dns_servers.clone())),
        "DNS forwarders for mgmt-VLAN hosts (comma-separated)",
        &["8.8.8.8".to_string(), "8.8.4.4".to_string()],
    )?;
    let allow_password_auth = wiz::prompt_bool(
        args.allow_password_auth,
        "Keep SSH password auth enabled alongside the key? (recovery fallback, not the default)",
        false,
    )?;

    let cfg = vyos_flavor_iso::VyosFlavorConfig {
        base: vyos_iso::VyosBuildConfig {
            hostname,
            // Ignored by the flavor renderer -- it resolves the real
            // interface names at boot from trunk_pci/uplink_pci instead.
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
        },
        trunk_pci,
        uplink_pci,
    };

    std::fs::create_dir_all(&args.out)?;
    let script_path = args.out.join("crowcloud-fabric-init.sh");
    std::fs::write(
        &script_path,
        vyos_flavor_iso::render_fabric_init_script(&cfg),
    )?;
    let cron_path = args.out.join("crowcloud-fabric-init.cron");
    std::fs::write(&cron_path, vyos_flavor_iso::render_cron_entry())?;
    let flavor_path = args.out.join("crowcloud.toml");
    std::fs::write(&flavor_path, vyos_flavor_iso::render_flavor_toml(&cfg))?;

    println!("Wrote {}", script_path.display());
    println!("Wrote {}", cron_path.display());
    println!("Wrote {}", flavor_path.display());
    println!(
        "\nTo build the ISO, run (requires Docker with privileged-container support):\n\
         \n\
         git clone -b rolling --single-branch https://github.com/vyos/vyos-build\n\
         cp {flavor} vyos-build/data/build-flavors/crowcloud.toml\n\
         cd vyos-build\n\
         docker run --rm -it --privileged -v $(pwd):/vyos -w /vyos vyos/vyos-build:rolling \\\n\
         \x20 sudo ./build-vyos-image --architecture amd64 --build-by crowcloud crowcloud",
        flavor = flavor_path.display()
    );

    Ok(())
}

fn build_proxmox(args: ProxmoxBuildArgs) -> Result<()> {
    use crate::iso::vyos_wizard as wiz;
    let fabric = Config::load()?.fabric;

    let root_password_hash = hash_password(&args.root_password)?;
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
    let bgp_peer_password = wiz::prompt_secret(
        args.bgp_peer_password
            .or(fabric.as_ref().map(|f| f.bgp_peer_password.clone())),
        "BGP peer-group password",
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

    let cfg = proxmox_iso::ProxmoxBuildConfig {
        root_password_hash,
        fqdn: args.fqdn,
        admin_email: args.admin_email,
        trunk_interface: args.trunk_interface,
        underlay_vlan,
        mgmt_vlan,
        mgmt_ip: args.mgmt_ip,
        mgmt_prefix: args.mgmt_prefix,
        mgmt_gateway,
        trunk_mtu,
        disk_list: args.disk,
        zfs_raid: args.zfs_raid,
        crow_api_url: args.crow_api_url,
        fleet_secret,
        bgp_asn,
        bgp_peer_password,
        underlay_prefix,
        ospf_area,
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
             image build. Install it and re-run, or use {} and {} manually.",
            answer_path.display(),
            hook_path.display()
        );
        return Ok(());
    }

    let output_iso = args.out.join("proxmox-auto.iso");
    // `--on-first-boot` bundles a script that PVE 8.1+'s auto-install
    // runs once, automatically, on the installed system's first boot --
    // this is what makes a single USB stick self-contained end to end
    // (base install + fabric setup + self-registration), no manual
    // delivery step after install. NOTE: exact flag name/behavior is
    // not verified against a live `proxmox-auto-install-assistant` in
    // this environment (not installed here) -- if this errors as an
    // unrecognized flag, that's the first thing to check against
    // whatever version is actually installed.
    let status = Command::new("proxmox-auto-install-assistant")
        .arg("prepare-iso")
        .arg(&base_iso)
        .arg("--fetch-from")
        .arg("iso")
        .arg("--answer-file")
        .arg(&answer_path)
        .arg("--on-first-boot")
        .arg(&hook_path)
        .arg("--output")
        .arg(&output_iso)
        .status()
        .context("running proxmox-auto-install-assistant")?;

    if !status.success() {
        bail!("proxmox-auto-install-assistant exited with {status}");
    }

    println!("Built {}", output_iso.display());
    println!(
        "This ISO is fully self-contained -- boot it from USB and the \
         installed system applies fabric config and self-registers on \
         its own first boot, no manual delivery of {} required.",
        hook_path.display()
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
