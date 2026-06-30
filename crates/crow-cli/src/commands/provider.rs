use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::client::CrowClient;

#[derive(Args)]
pub struct ProviderCmd {
    #[command(subcommand)]
    pub command: ProviderSubcommand,
}

#[derive(Subcommand)]
pub enum ProviderSubcommand {
    /// List registered providers
    List,
    /// Add a Proxmox VE provider
    AddProxmox(AddProxmoxArgs),
    /// Delete a provider by name or ID
    Delete { name: String },
}

#[derive(Args)]
pub struct AddProxmoxArgs {
    /// Name for this provider (must be unique)
    pub name: String,
    /// Proxmox API URL (e.g. https://pve.lan:8006)
    #[arg(long)]
    pub url: String,
    /// API token ID (e.g. root@pam!crow)
    #[arg(long)]
    pub token_id: String,
    /// API token secret UUID
    #[arg(long)]
    pub token_secret: String,
    /// Proxmox node name (e.g. pve)
    #[arg(long)]
    pub node: String,
    /// Default storage pool (e.g. local-lvm)
    #[arg(long, default_value = "local-lvm")]
    pub storage: String,
    /// Default bridge (e.g. vmbr0)
    #[arg(long, default_value = "vmbr0")]
    pub bridge: String,
    /// Skip TLS certificate verification
    #[arg(long)]
    pub tls_insecure: bool,
}

#[derive(Deserialize)]
struct ProviderRow {
    id: String,
    name: String,
    provider_type: String,
    created_at: String,
}

#[derive(Serialize)]
struct CreateProviderBody {
    name: String,
    provider_type: String,
    config: serde_json::Value,
}

pub async fn run(cmd: ProviderCmd) -> Result<()> {
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        ProviderSubcommand::List => {
            let providers: Vec<ProviderRow> = client.get("/api/v1/providers").await?;
            if providers.is_empty() {
                println!("No providers registered.");
            } else {
                println!("{:<36}  {:<24}  {:<10}  CREATED", "ID", "NAME", "TYPE");
                for p in &providers {
                    println!(
                        "{:<36}  {:<24}  {:<10}  {}",
                        p.id, p.name, p.provider_type, p.created_at
                    );
                }
            }
        }
        ProviderSubcommand::AddProxmox(args) => {
            let body = CreateProviderBody {
                name: args.name.clone(),
                provider_type: "proxmox".into(),
                config: json!({
                    "url": args.url,
                    "token_id": args.token_id,
                    "token_secret": args.token_secret,
                    "node": args.node,
                    "default_storage": args.storage,
                    "default_bridge": args.bridge,
                    "tls_insecure": args.tls_insecure,
                }),
            };
            let p: ProviderRow = client.post("/api/v1/providers", &body).await?;
            println!("Added provider '{}' ({})", p.name, p.id);
        }
        ProviderSubcommand::Delete { name } => {
            let id = client.resolve_provider_id(&name).await?;
            client.delete(&format!("/api/v1/providers/{id}")).await?;
            println!("Deleted provider '{name}'");
        }
    }
    Ok(())
}
