use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ContextCmd {
    #[command(subcommand)]
    pub command: ContextSubcommand,
}

#[derive(Subcommand)]
pub enum ContextSubcommand {
    Set(SetArgs),
    Show,
}

#[derive(Args)]
pub struct SetArgs {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub rg: Option<String>,
}

pub async fn run(_cmd: ContextCmd) -> Result<()> {
    todo!("context")
}
