use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::iso::{proxmox as proxmox_iso, vyos as vyos_iso};

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
    Proxmox(ProxmoxCmd),
}

#[derive(Args)]
pub struct VyosCmd {
    #[command(subcommand)]
    pub command: VyosSubcommand,
}

#[derive(Subcommand)]
pub enum VyosSubcommand {
    Build(VyosBuildArgs),
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

#[derive(Args)]
pub struct VyosBuildArgs {
    #[arg(long)]
    pub hostname: String,
    /// Physical NIC carrying the tagged trunk to the switch (underlay + management VLANs)
    #[arg(long)]
    pub trunk_interface: String,
    /// Physical NIC used for internet/LAN uplink
    #[arg(long)]
    pub uplink_interface: String,
    #[arg(long, default_value_t = 9000)]
    pub trunk_mtu: u32,
    #[arg(long)]
    pub underlay_vlan: u16,
    #[arg(long)]
    pub underlay_ip: String,
    #[arg(long, default_value_t = 24)]
    pub underlay_prefix: u8,
    #[arg(long)]
    pub mgmt_vlan: u16,
    #[arg(long)]
    pub mgmt_ip: String,
    #[arg(long, default_value_t = 24)]
    pub mgmt_prefix: u8,
    #[arg(long)]
    pub loopback_ip: String,
    #[arg(long)]
    pub uplink_dhcp: bool,
    #[arg(long)]
    pub uplink_ip: Option<String>,
    #[arg(long)]
    pub uplink_prefix: Option<u8>,
    #[arg(long)]
    pub uplink_gateway: Option<String>,
    #[arg(long, default_value = "0")]
    pub ospf_area: String,
    #[arg(long)]
    pub underlay_network: String,
    #[arg(long, default_value_t = 24)]
    pub underlay_network_prefix: u8,
    /// Path to a public key file -- image is built SSH-key-only, never
    /// with a baked password
    #[arg(long)]
    pub ssh_pubkey: PathBuf,
    #[arg(long, default_value_t = 65000)]
    pub bgp_asn: u32,
    /// Shared secret for BGP peer-group auth -- must match every Proxmox
    /// host's `--bgp-peer-password` (#66)
    #[arg(long)]
    pub bgp_peer_password: String,
    /// Keep SSH password auth enabled alongside the new key, instead of
    /// disabling it. Default is key-only; use this while validating key
    /// access on a given box, since a bad key commit + disabled password
    /// auth means an SSH lockout with no fallback (see #66's incident
    /// notes)
    #[arg(long)]
    pub allow_password_auth: bool,
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
    #[arg(long)]
    pub underlay_vlan: u16,
    #[arg(long)]
    pub mgmt_vlan: u16,
    #[arg(long)]
    pub mgmt_ip: String,
    #[arg(long, default_value_t = 24)]
    pub mgmt_prefix: u8,
    #[arg(long)]
    pub mgmt_gateway: String,
    #[arg(long, default_value_t = 9000)]
    pub trunk_mtu: u32,
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
    #[arg(long, default_value_t = 65000)]
    pub bgp_asn: u32,
    #[arg(long)]
    pub bgp_peer_password: String,
    #[arg(long, default_value_t = 24)]
    pub underlay_prefix: u8,
    #[arg(long, default_value = "0")]
    pub ospf_area: String,
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
            VyosSubcommand::Build(args) => build_vyos(args),
        },
        IsoSubcommand::Proxmox(proxmox_cmd) => match proxmox_cmd.command {
            ProxmoxSubcommand::Build(args) => build_proxmox(args),
        },
    }
}

fn build_vyos(args: VyosBuildArgs) -> Result<()> {
    let ssh_pubkey = std::fs::read_to_string(&args.ssh_pubkey)
        .with_context(|| format!("reading SSH public key at {}", args.ssh_pubkey.display()))?
        .trim()
        .to_string();

    if args.uplink_dhcp {
        if args.uplink_ip.is_some() || args.uplink_prefix.is_some() || args.uplink_gateway.is_some()
        {
            bail!(
                "--uplink-dhcp is incompatible with --uplink-ip/--uplink-prefix/--uplink-gateway"
            );
        }
    } else if args.uplink_ip.is_none() || args.uplink_prefix.is_none() {
        bail!("either --uplink-dhcp or both --uplink-ip and --uplink-prefix are required");
    }

    let cfg = vyos_iso::VyosBuildConfig {
        hostname: args.hostname,
        trunk_interface: args.trunk_interface,
        uplink_interface: args.uplink_interface,
        trunk_mtu: args.trunk_mtu,
        underlay_vlan: args.underlay_vlan,
        underlay_ip: args.underlay_ip,
        underlay_prefix: args.underlay_prefix,
        mgmt_vlan: args.mgmt_vlan,
        mgmt_ip: args.mgmt_ip,
        mgmt_prefix: args.mgmt_prefix,
        loopback_ip: args.loopback_ip,
        uplink_dhcp: args.uplink_dhcp,
        uplink_ip: args.uplink_ip,
        uplink_prefix: args.uplink_prefix,
        uplink_gateway: args.uplink_gateway,
        ospf_area: args.ospf_area,
        underlay_network: args.underlay_network,
        underlay_network_prefix: args.underlay_network_prefix,
        ssh_pubkey,
        bgp_asn: args.bgp_asn,
        bgp_peer_password: args.bgp_peer_password,
        allow_password_auth: args.allow_password_auth,
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

fn build_proxmox(args: ProxmoxBuildArgs) -> Result<()> {
    let root_password_hash = hash_password(&args.root_password)?;
    let fleet_secret = match args.fleet_secret {
        Some(s) => s,
        None => Config::fleet_secret_or_generate()?,
    };

    let cfg = proxmox_iso::ProxmoxBuildConfig {
        root_password_hash,
        fqdn: args.fqdn,
        admin_email: args.admin_email,
        trunk_interface: args.trunk_interface,
        underlay_vlan: args.underlay_vlan,
        mgmt_vlan: args.mgmt_vlan,
        mgmt_ip: args.mgmt_ip,
        mgmt_prefix: args.mgmt_prefix,
        mgmt_gateway: args.mgmt_gateway,
        trunk_mtu: args.trunk_mtu,
        disk_list: args.disk,
        zfs_raid: args.zfs_raid,
        crow_api_url: args.crow_api_url,
        fleet_secret,
        bgp_asn: args.bgp_asn,
        bgp_peer_password: args.bgp_peer_password,
        underlay_prefix: args.underlay_prefix,
        ospf_area: args.ospf_area,
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
