use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ExposeCmd {
    #[command(subcommand)]
    pub command: ExposeSubcommand,
}

#[derive(Subcommand)]
pub enum ExposeSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: ExposeCmd) -> Result<()> {
    todo!("expose commands")
}
