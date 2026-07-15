use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::Config;

#[derive(Args)]
pub struct ContextCmd {
    #[command(subcommand)]
    pub command: ContextSubcommand,
}

#[derive(Subcommand)]
pub enum ContextSubcommand {
    /// Set current project
    Set(SetArgs),
    /// Show current context
    Show,
}

#[derive(Args)]
pub struct SetArgs {
    #[arg(long)]
    pub project: Option<String>,
}

pub async fn run(cmd: ContextCmd) -> Result<()> {
    match cmd.command {
        ContextSubcommand::Set(args) => {
            let mut cfg = Config::load()?;
            if let Some(p) = args.project {
                cfg.current_project = Some(p);
            }
            cfg.save()?;
            println!(
                "Context updated — project: {}",
                cfg.current_project.as_deref().unwrap_or("(none)"),
            );
        }
        ContextSubcommand::Show => {
            let cfg = Config::load()?;
            println!("server:  {}", cfg.server.as_deref().unwrap_or("(none)"));
            println!(
                "project: {}",
                cfg.current_project.as_deref().unwrap_or("(none)")
            );
        }
    }
    Ok(())
}
