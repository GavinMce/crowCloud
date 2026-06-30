use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::CrowClient;

#[derive(Args)]
pub struct ProjectCmd {
    #[command(subcommand)]
    pub command: ProjectSubcommand,
}

#[derive(Subcommand)]
pub enum ProjectSubcommand {
    /// List projects
    List,
    /// Create a project
    Create { name: String },
    /// Delete a project
    Delete { name: String },
}

#[derive(Deserialize)]
struct ProjectRow {
    id: String,
    name: String,
    created_at: String,
}

#[derive(Serialize)]
struct CreateProjectBody {
    name: String,
}

pub async fn run(cmd: ProjectCmd) -> Result<()> {
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        ProjectSubcommand::List => {
            let projects: Vec<ProjectRow> = client.get("/api/v1/projects").await?;
            if projects.is_empty() {
                println!("No projects.");
            } else {
                println!("{:<36}  {:<32}  CREATED", "ID", "NAME");
                for p in &projects {
                    println!("{:<36}  {:<32}  {}", p.id, p.name, p.created_at);
                }
            }
        }
        ProjectSubcommand::Create { name } => {
            let p: ProjectRow = client
                .post("/api/v1/projects", &CreateProjectBody { name })
                .await?;
            println!("Created project '{}' ({})", p.name, p.id);
        }
        ProjectSubcommand::Delete { name } => {
            client.delete(&format!("/api/v1/projects/{name}")).await?;
            println!("Deleted project '{name}'");
        }
    }
    Ok(())
}
