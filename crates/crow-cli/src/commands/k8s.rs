use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct K8sCmd {
    #[command(subcommand)]
    pub command: K8sSubcommand,
}

#[derive(Subcommand)]
pub enum K8sSubcommand {
    /// Provision a new K8s cluster
    Create(CreateArgs),
    /// List clusters
    List,
    /// Show cluster details
    Get { name: String },
    /// Delete a cluster
    Delete { name: String },
    /// Scale worker nodes
    Scale {
        name: String,
        #[arg(long)]
        workers: u32,
    },
    /// Print or merge kubeconfig
    Kubeconfig(KubeconfigArgs),
    /// Expose cluster ingress externally
    Expose(ExposeArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    #[arg(long, default_value = "k3s", help = "k3s | rke2")]
    pub distribution: String,
    #[arg(long, default_value = "v1.31.0")]
    pub version: String,
    #[arg(long, default_value = "1", help = "1 = single, 3 = HA")]
    pub cp_count: u32,
    #[arg(long, default_value = "2")]
    pub cp_cpu: u32,
    #[arg(long, default_value = "4")]
    pub cp_mem: u32,
    #[arg(long, default_value = "2")]
    pub workers: u32,
    #[arg(long, default_value = "4")]
    pub worker_cpu: u32,
    #[arg(long, default_value = "8")]
    pub worker_mem: u32,
    #[arg(long, help = "MetalLB IP range, e.g. 192.168.1.150-192.168.1.180")]
    pub lb_pool: Option<String>,
    #[arg(long, help = "Block until cluster is Ready")]
    pub wait: bool,
}

#[derive(Args)]
pub struct KubeconfigArgs {
    pub name: String,
    #[arg(long, help = "Merge into ~/.kube/config")]
    pub merge: bool,
}

#[derive(Args)]
pub struct ExposeArgs {
    pub name: String,
    #[arg(long, help = "Wildcard domain, e.g. *.apps.example.com")]
    pub domain: String,
    #[arg(long)]
    pub tls: bool,
}

pub async fn run(_cmd: K8sCmd) -> Result<()> {
    todo!("k8s commands")
}
