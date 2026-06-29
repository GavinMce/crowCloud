use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct StoreCmd {
    #[command(subcommand)]
    pub command: StoreSubcommand,
}

#[derive(Subcommand)]
pub enum StoreSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: StoreCmd) -> Result<()> {
    todo!("store commands")
}
