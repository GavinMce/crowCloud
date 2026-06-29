use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct VmCmd {
    #[command(subcommand)]
    pub command: VmSubcommand,
}

#[derive(Subcommand)]
pub enum VmSubcommand {
    /// Create a new VM
    Create(CreateArgs),
    /// List VMs
    List,
    /// Show VM details
    Get { name: String },
    /// Delete a VM
    Delete { name: String },
    /// Start a stopped VM
    Start { name: String },
    /// Stop a running VM
    Stop { name: String },
    /// Open an SSH session to a VM
    Ssh { name: String },
    /// Expose a VM port externally
    Expose(ExposeArgs),
    /// Remove external exposure
    Unexpose { name: String, #[arg(long)] domain: Option<String> },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    #[arg(long, default_value = "2")]
    pub cpu: u32,
    #[arg(long, default_value = "4", help = "Memory in GiB")]
    pub mem: u32,
    #[arg(long, default_value = "50", help = "Disk in GiB")]
    pub disk: u64,
    #[arg(long)]
    pub image: String,
    #[arg(long, help = "Block until provisioned")]
    pub wait: bool,
}

#[derive(Args)]
pub struct ExposeArgs {
    pub name: String,
    #[arg(long)]
    pub port: u16,
    #[arg(long, help = "HTTP domain (required for http expose)")]
    pub domain: Option<String>,
    #[arg(long, default_value = "http", help = "http | tcp | udp")]
    pub protocol: String,
    #[arg(long, help = "Public port for TCP/UDP expose")]
    pub public_port: Option<u16>,
    #[arg(long)]
    pub tls: bool,
}

pub async fn run(_cmd: VmCmd) -> Result<()> {
    todo!("vm commands")
}
