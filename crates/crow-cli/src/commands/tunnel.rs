use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct TunnelCmd {
    #[command(subcommand)]
    pub command: TunnelSubcommand,
}

#[derive(Subcommand)]
pub enum TunnelSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: TunnelCmd) -> Result<()> {
    todo!("tunnel commands")
}
