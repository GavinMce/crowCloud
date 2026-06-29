use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct RgCmd {
    #[command(subcommand)]
    pub command: RgSubcommand,
}

#[derive(Subcommand)]
pub enum RgSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: RgCmd) -> Result<()> {
    todo!("rg commands")
}
