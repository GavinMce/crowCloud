use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::{require_project, CrowClient};

#[derive(Args)]
pub struct RgCmd {
    /// Project (defaults to context)
    #[arg(long, global = true)]
    pub project: Option<String>,
    #[command(subcommand)]
    pub command: RgSubcommand,
}

#[derive(Subcommand)]
pub enum RgSubcommand {
    /// List resource groups
    List,
    /// Create a resource group
    Create { name: String },
    /// Delete a resource group
    Delete { name: String },
}

#[derive(Deserialize)]
struct RgRow {
    id: String,
    name: String,
    created_at: String,
}

#[derive(Serialize)]
struct CreateRgBody {
    name: String,
}

pub async fn run(cmd: RgCmd) -> Result<()> {
    let project = require_project(cmd.project)?;
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        RgSubcommand::List => {
            let rgs: Vec<RgRow> = client
                .get(&format!("/api/v1/projects/{project}/resource-groups"))
                .await?;
            if rgs.is_empty() {
                println!("No resource groups in project '{project}'.");
            } else {
                println!("{:<36}  {:<32}  CREATED", "ID", "NAME");
                for r in &rgs {
                    println!("{:<36}  {:<32}  {}", r.id, r.name, r.created_at);
                }
            }
        }
        RgSubcommand::Create { name } => {
            let r: RgRow = client
                .post(
                    &format!("/api/v1/projects/{project}/resource-groups"),
                    &CreateRgBody { name },
                )
                .await?;
            println!("Created resource group '{}' ({})", r.name, r.id);
        }
        RgSubcommand::Delete { name } => {
            client
                .delete(&format!(
                    "/api/v1/projects/{project}/resource-groups/{name}"
                ))
                .await?;
            println!("Deleted resource group '{name}'");
        }
    }
    Ok(())
}
