use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ProviderCmd {
    #[command(subcommand)]
    pub command: ProviderSubcommand,
}

#[derive(Subcommand)]
pub enum ProviderSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: ProviderCmd) -> Result<()> {
    todo!("provider commands")
}
