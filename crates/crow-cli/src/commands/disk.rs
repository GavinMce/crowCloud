use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::{require_project, CrowClient};

#[derive(Args)]
pub struct DiskCmd {
    /// Project (defaults to context)
    #[arg(long, global = true)]
    pub project: Option<String>,
    #[command(subcommand)]
    pub command: DiskSubcommand,
}

#[derive(Subcommand)]
pub enum DiskSubcommand {
    /// Create a new disk
    Create(CreateArgs),
    /// List disks in the current project
    List,
    /// Show disk details
    Get { name: String },
    /// Attach an unattached disk to a VM
    Attach {
        name: String,
        /// VM to attach to (must be on the same host+node the disk was created on)
        #[arg(long)]
        vm: String,
    },
    /// Detach a disk, leaving it for reuse (does not delete data)
    Detach { name: String },
    /// Grow a disk (shrinking is not supported)
    Resize {
        name: String,
        #[arg(long)]
        size_gib: u32,
    },
    /// Delete a disk (must be detached first)
    Delete { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// Provider name or UUID
    #[arg(long)]
    pub provider: String,
    /// Which of the host's adopted nodes the disk's storage lives on
    #[arg(long)]
    pub node: String,
    #[arg(long)]
    pub size_gib: u32,
    /// Attach immediately to this VM (omit to create unattached, for later assignment)
    #[arg(long)]
    pub vm: Option<String>,
}

#[derive(Serialize)]
struct CreateDiskBody {
    resource_type: &'static str,
    name: String,
    provider_id: String,
    node: String,
    size_gib: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    vm_name: Option<String>,
}

#[derive(Serialize)]
struct UpdateDiskBody {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    detach: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    vm_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_gib: Option<u32>,
}

#[derive(Deserialize)]
struct DiskRow {
    id: String,
    name: String,
    resource_type: String,
    phase: String,
    created_at: String,
}

pub async fn run(cmd: DiskCmd) -> Result<()> {
    let project = require_project(cmd.project)?;
    let client = CrowClient::from_config(None)?;
    let base = format!("/api/v1/projects/{project}/resources");

    match cmd.command {
        DiskSubcommand::List => {
            let disks: Vec<DiskRow> = client.get(&base).await?;
            let disks: Vec<&DiskRow> = disks.iter().filter(|r| r.resource_type == "disk").collect();
            if disks.is_empty() {
                println!("No disks in {project}.");
            } else {
                println!("{:<36}  {:<24}  {:<14}  CREATED", "ID", "NAME", "PHASE");
                for d in disks {
                    println!(
                        "{:<36}  {:<24}  {:<14}  {}",
                        d.id, d.name, d.phase, d.created_at
                    );
                }
            }
        }
        DiskSubcommand::Get { name } => {
            let d: DiskRow = client.get(&format!("{base}/{name}")).await?;
            println!("id:      {}", d.id);
            println!("name:    {}", d.name);
            println!("phase:   {}", d.phase);
            println!("created: {}", d.created_at);
        }
        DiskSubcommand::Create(args) => {
            let provider_id = client.resolve_provider_id(&args.provider).await?;
            let body = CreateDiskBody {
                resource_type: "disk",
                name: args.name.clone(),
                provider_id,
                node: args.node,
                size_gib: args.size_gib,
                vm_name: args.vm,
            };
            let d: DiskRow = client.post(&base, &body).await?;
            println!("Created disk '{}' ({}) — phase: {}", d.name, d.id, d.phase);
        }
        DiskSubcommand::Attach { name, vm } => {
            let body = UpdateDiskBody {
                detach: false,
                vm_name: Some(vm.clone()),
                size_gib: None,
            };
            let d: DiskRow = client.patch(&format!("{base}/{name}"), &body).await?;
            println!("Attached disk '{}' to '{vm}' — phase: {}", d.name, d.phase);
        }
        DiskSubcommand::Detach { name } => {
            let body = UpdateDiskBody {
                detach: true,
                vm_name: None,
                size_gib: None,
            };
            let d: DiskRow = client.patch(&format!("{base}/{name}"), &body).await?;
            println!("Detached disk '{}' — phase: {}", d.name, d.phase);
        }
        DiskSubcommand::Resize { name, size_gib } => {
            let body = UpdateDiskBody {
                detach: false,
                vm_name: None,
                size_gib: Some(size_gib),
            };
            let d: DiskRow = client.patch(&format!("{base}/{name}"), &body).await?;
            println!(
                "Resizing disk '{}' to {size_gib}GiB — phase: {}",
                d.name, d.phase
            );
        }
        DiskSubcommand::Delete { name } => {
            client.delete(&format!("{base}/{name}")).await?;
            println!("Deleted disk '{name}'");
        }
    }
    Ok(())
}
