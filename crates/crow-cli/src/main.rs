use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod output;

#[derive(Parser)]
#[command(name = "crow", about = "crowCloud CLI", version)]
pub struct Cli {
    /// Output format
    #[arg(short, long, global = true, default_value = "table")]
    pub output: String,

    /// crowCloud server URL (overrides config)
    #[arg(long, global = true, env = "CROW_SERVER")]
    pub server: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Authenticate with a crowCloud server
    Login(commands::auth::LoginArgs),
    /// Manage CLI context (project, resource group)
    Context(commands::context::ContextCmd),
    /// Manage projects
    Project(commands::project::ProjectCmd),
    /// Manage resource groups
    Rg(commands::rg::RgCmd),
    /// Manage virtual machines
    Vm(commands::vm::VmCmd),
    /// Manage Kubernetes clusters
    K8s(commands::k8s::K8sCmd),
    /// Manage databases
    Db(commands::db::DbCmd),
    /// Manage object stores
    Store(commands::store::StoreCmd),
    /// Manage exposed endpoints
    Expose(commands::expose::ExposeCmd),
    /// Manage custom domains
    Domain(commands::domain::DomainCmd),
    /// Manage infrastructure providers
    Provider(commands::provider::ProviderCmd),
    /// Manage VPS tunnel endpoint
    Tunnel(commands::tunnel::TunnelCmd),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::dispatch(cli).await
}
