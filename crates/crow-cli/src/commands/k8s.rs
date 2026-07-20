use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::{require_project, CrowClient};

#[derive(Args)]
pub struct K8sCmd {
    /// Project (defaults to context)
    #[arg(long, global = true)]
    pub project: Option<String>,
    #[command(subcommand)]
    pub command: K8sSubcommand,
}

#[derive(Subcommand)]
pub enum K8sSubcommand {
    /// Provision a new K3s cluster
    Create(Box<CreateArgs>),
    /// List clusters in the current project
    List,
    /// Show cluster details
    Get { name: String },
    /// Delete a cluster
    Delete { name: String },
    /// Print the cluster's kubeconfig (only available once bootstrapping finishes)
    Kubeconfig { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// Provider name or UUID
    #[arg(long)]
    pub provider: String,
    /// Which of the host's adopted nodes to provision on
    #[arg(long)]
    pub node: String,
    /// Proxmox template VMID (same convention as `crow vm create --image`)
    #[arg(long)]
    pub image: String,
    /// IpPool the control plane's static address is requested from — required,
    /// unlike a plain VM's optional pool
    #[arg(long = "ip-pool")]
    pub ip_pool: String,
    /// Empty installs K3s's current stable release
    #[arg(long, default_value = "")]
    pub k3s_version: String,
    #[arg(long, default_value = "2")]
    pub control_plane_cpu: u32,
    #[arg(long, default_value = "4")]
    pub control_plane_memory_gib: u32,
    #[arg(long, default_value = "40")]
    pub control_plane_disk_gib: u32,
    #[arg(long, default_value = "2")]
    pub worker_count: u32,
    #[arg(long, default_value = "2")]
    pub worker_cpu: u32,
    #[arg(long, default_value = "4")]
    pub worker_memory_gib: u32,
    #[arg(long, default_value = "40")]
    pub worker_disk_gib: u32,
    #[arg(long, default_value = "10.42.0.0/16")]
    pub pod_cidr: String,
    #[arg(long, default_value = "10.43.0.0/16")]
    pub service_cidr: String,
    /// Cilium LB-IPAM range for LoadBalancer services, e.g. 10.0.202.200/29
    #[arg(long)]
    pub lb_pool_cidr: Option<String>,
    /// Install kube-prometheus-stack (off by default — memory-hungry)
    #[arg(long)]
    pub monitoring: bool,
}

#[derive(Serialize)]
struct CreateK8sClusterBody {
    resource_type: &'static str,
    name: String,
    provider_id: String,
    node: String,
    image: String,
    ip_pool: String,
    k3s_version: String,
    control_plane_cpu: u32,
    control_plane_memory_gib: u32,
    control_plane_disk_gib: u32,
    worker_count: u32,
    worker_cpu: u32,
    worker_memory_gib: u32,
    worker_disk_gib: u32,
    pod_cidr: String,
    service_cidr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    lb_pool_cidr: Option<String>,
    monitoring: bool,
}

#[derive(Deserialize)]
struct K8sClusterRow {
    id: String,
    name: String,
    resource_type: String,
    phase: String,
    created_at: String,
}

#[derive(Deserialize)]
struct KubeconfigResponse {
    kubeconfig: String,
}

pub async fn run(cmd: K8sCmd) -> Result<()> {
    let project = require_project(cmd.project)?;
    let client = CrowClient::from_config(None)?;
    let base = format!("/api/v1/projects/{project}/resources");

    match cmd.command {
        K8sSubcommand::List => {
            let clusters: Vec<K8sClusterRow> = client.get(&base).await?;
            let clusters: Vec<&K8sClusterRow> = clusters
                .iter()
                .filter(|r| r.resource_type == "k8s_cluster")
                .collect();
            if clusters.is_empty() {
                println!("No K8s clusters in {project}.");
            } else {
                println!("{:<36}  {:<24}  {:<14}  CREATED", "ID", "NAME", "PHASE");
                for c in clusters {
                    println!(
                        "{:<36}  {:<24}  {:<14}  {}",
                        c.id, c.name, c.phase, c.created_at
                    );
                }
            }
        }
        K8sSubcommand::Get { name } => {
            let c: K8sClusterRow = client.get(&format!("{base}/{name}")).await?;
            println!("id:      {}", c.id);
            println!("name:    {}", c.name);
            println!("phase:   {}", c.phase);
            println!("created: {}", c.created_at);
        }
        K8sSubcommand::Create(args) => {
            let args = *args;
            let provider_id = client.resolve_provider_id(&args.provider).await?;
            let body = CreateK8sClusterBody {
                resource_type: "k8s_cluster",
                name: args.name.clone(),
                provider_id,
                node: args.node,
                image: args.image,
                ip_pool: args.ip_pool,
                k3s_version: args.k3s_version,
                control_plane_cpu: args.control_plane_cpu,
                control_plane_memory_gib: args.control_plane_memory_gib,
                control_plane_disk_gib: args.control_plane_disk_gib,
                worker_count: args.worker_count,
                worker_cpu: args.worker_cpu,
                worker_memory_gib: args.worker_memory_gib,
                worker_disk_gib: args.worker_disk_gib,
                pod_cidr: args.pod_cidr,
                service_cidr: args.service_cidr,
                lb_pool_cidr: args.lb_pool_cidr,
                monitoring: args.monitoring,
            };
            let c: K8sClusterRow = client.post(&base, &body).await?;
            println!(
                "Created K8s cluster '{}' ({}) — phase: {}",
                c.name, c.id, c.phase
            );
        }
        K8sSubcommand::Delete { name } => {
            client.delete(&format!("{base}/{name}")).await?;
            println!("Deleted K8s cluster '{name}'");
        }
        K8sSubcommand::Kubeconfig { name } => {
            let resp: KubeconfigResponse = client.get(&format!("{base}/{name}/kubeconfig")).await?;
            print!("{}", resp.kubeconfig);
        }
    }
    Ok(())
}
