use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ProjectCmd {
    #[command(subcommand)]
    pub command: ProjectSubcommand,
}

#[derive(Subcommand)]
pub enum ProjectSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: ProjectCmd) -> Result<()> {
    todo!("project commands")
}
