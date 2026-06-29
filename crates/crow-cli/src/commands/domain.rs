use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct DomainCmd {
    #[command(subcommand)]
    pub command: DomainSubcommand,
}

#[derive(Subcommand)]
pub enum DomainSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: DomainCmd) -> Result<()> {
    todo!("domain commands")
}
