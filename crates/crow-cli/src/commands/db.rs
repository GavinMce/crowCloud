use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct DbCmd {
    #[command(subcommand)]
    pub command: DbSubcommand,
}

#[derive(Subcommand)]
pub enum DbSubcommand {
    Create { name: String },
    List,
    Get { name: String },
    Delete { name: String },
}

pub async fn run(_cmd: DbCmd) -> Result<()> {
    todo!("db commands")
}
