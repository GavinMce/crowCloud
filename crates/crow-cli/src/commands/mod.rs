use anyhow::Result;
use crate::Cli;

pub mod auth;
pub mod context;
pub mod db;
pub mod domain;
pub mod expose;
pub mod k8s;
pub mod project;
pub mod provider;
pub mod rg;
pub mod store;
pub mod tunnel;
pub mod vm;

pub async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        crate::Commands::Login(args) => auth::login(args).await,
        crate::Commands::Context(cmd) => context::run(cmd).await,
        crate::Commands::Project(cmd) => project::run(cmd).await,
        crate::Commands::Rg(cmd) => rg::run(cmd).await,
        crate::Commands::Vm(cmd) => vm::run(cmd).await,
        crate::Commands::K8s(cmd) => k8s::run(cmd).await,
        crate::Commands::Db(cmd) => db::run(cmd).await,
        crate::Commands::Store(cmd) => store::run(cmd).await,
        crate::Commands::Expose(cmd) => expose::run(cmd).await,
        crate::Commands::Domain(cmd) => domain::run(cmd).await,
        crate::Commands::Provider(cmd) => provider::run(cmd).await,
        crate::Commands::Tunnel(cmd) => tunnel::run(cmd).await,
    }
}
