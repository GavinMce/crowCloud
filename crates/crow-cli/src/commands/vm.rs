use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::{require_project, CrowClient};

#[derive(Args)]
pub struct VmCmd {
    /// Project (defaults to context)
    #[arg(long, global = true)]
    pub project: Option<String>,
    #[command(subcommand)]
    pub command: VmSubcommand,
}

#[derive(Subcommand)]
pub enum VmSubcommand {
    /// Create a new VM
    Create(CreateArgs),
    /// List VMs in the current resource group
    List,
    /// Show VM details
    Get { name: String },
    /// Delete a VM
    Delete { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// Provider name or UUID
    #[arg(long)]
    pub provider: String,
    #[arg(long, default_value = "2")]
    pub cpu: u32,
    /// Memory in MiB
    #[arg(long, default_value = "2048")]
    pub memory_mib: u64,
    /// Disk in GiB
    #[arg(long, default_value = "20")]
    pub disk_gib: u64,
    #[arg(long)]
    pub image: String,
    #[arg(long)]
    pub hostname: Option<String>,
    /// Name of an IpPool to request a static address from (omit for DHCP)
    #[arg(long = "ip-pool")]
    pub ip_pool: Option<String>,
}

#[derive(Serialize)]
struct CreateVmBody {
    resource_type: &'static str,
    name: String,
    provider_id: String,
    cpu: u32,
    memory_mib: u64,
    disk_gib: u64,
    image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ip_pool: Option<String>,
}

#[derive(Deserialize)]
struct VmRow {
    id: String,
    name: String,
    resource_type: String,
    phase: String,
    created_at: String,
}

pub async fn run(cmd: VmCmd) -> Result<()> {
    let project = require_project(cmd.project)?;
    let client = CrowClient::from_config(None)?;
    let base = format!("/api/v1/projects/{project}/resources");

    match cmd.command {
        VmSubcommand::List => {
            let vms: Vec<VmRow> = client.get(&base).await?;
            let vms: Vec<&VmRow> = vms.iter().filter(|r| r.resource_type == "vm").collect();
            if vms.is_empty() {
                println!("No VMs in {project}.");
            } else {
                println!("{:<36}  {:<24}  {:<14}  CREATED", "ID", "NAME", "PHASE");
                for v in vms {
                    println!(
                        "{:<36}  {:<24}  {:<14}  {}",
                        v.id, v.name, v.phase, v.created_at
                    );
                }
            }
        }
        VmSubcommand::Get { name } => {
            let v: VmRow = client.get(&format!("{base}/{name}")).await?;
            println!("id:      {}", v.id);
            println!("name:    {}", v.name);
            println!("phase:   {}", v.phase);
            println!("created: {}", v.created_at);
        }
        VmSubcommand::Create(args) => {
            let provider_id = client.resolve_provider_id(&args.provider).await?;
            let body = CreateVmBody {
                resource_type: "vm",
                name: args.name.clone(),
                provider_id,
                cpu: args.cpu,
                memory_mib: args.memory_mib,
                disk_gib: args.disk_gib,
                image: args.image,
                hostname: args.hostname,
                ip_pool: args.ip_pool,
            };
            let v: VmRow = client.post(&base, &body).await?;
            println!("Created VM '{}' ({}) — phase: {}", v.name, v.id, v.phase);
        }
        VmSubcommand::Delete { name } => {
            client.delete(&format!("{base}/{name}")).await?;
            println!("Deleted VM '{name}'");
        }
    }
    Ok(())
}
