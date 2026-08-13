use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::CrowClient;
use crate::commands::expose::canonical_target_kind;

#[derive(Args)]
pub struct PublicIpCmd {
    #[command(subcommand)]
    pub command: PublicIpSubcommand,
}

#[derive(Subcommand)]
pub enum PublicIpSubcommand {
    /// Reserve an address on the upstream LAN and forward all traffic to
    /// it straight through to a private-subnet resource
    Create(CreateArgs),
    /// List reserved public IPs
    List,
    /// Show reserved public IP details
    Get { name: String },
    /// Release a reserved public IP
    Delete { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// Address to reserve -- must fall within the uplink network's own
    /// subnet
    #[arg(long)]
    pub address: String,
    #[arg(long)]
    pub prefix: u8,
    /// What kind of resource all traffic forwards to. Only "vm" actually
    /// resolves a target IP today.
    #[arg(long, default_value = "vm")]
    pub target_kind: String,
    /// CR name of the target (e.g. the VM's resource id)
    #[arg(long)]
    pub target_name: String,
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Serialize)]
struct CreatePublicIpBody {
    name: String,
    address: String,
    prefix: u8,
    target_kind: String,
    target_name: String,
    label: Option<String>,
}

#[derive(Deserialize)]
struct PublicIpRow {
    name: String,
    address: String,
    target_kind: String,
    target_name: String,
    phase: Option<String>,
}

#[derive(Deserialize)]
struct PublicIpDetail {
    name: String,
    address: String,
    prefix: u8,
    target_kind: String,
    target_name: String,
    label: Option<String>,
    phase: Option<String>,
    message: Option<String>,
}

fn fmt_opt(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("—")
}

pub async fn run(cmd: PublicIpCmd) -> Result<()> {
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        PublicIpSubcommand::List => {
            let ips: Vec<PublicIpRow> = client.get("/api/v1/public-ips").await?;
            if ips.is_empty() {
                println!("No public IPs reserved.");
            } else {
                println!(
                    "{:<20}  {:<16}  {:<8}  {:<20}  PHASE",
                    "NAME", "ADDRESS", "KIND", "TARGET"
                );
                for ip in &ips {
                    println!(
                        "{:<20}  {:<16}  {:<8}  {:<20}  {}",
                        ip.name,
                        ip.address,
                        ip.target_kind,
                        ip.target_name,
                        fmt_opt(&ip.phase)
                    );
                }
            }
        }
        PublicIpSubcommand::Get { name } => {
            let ip: PublicIpDetail = client.get(&format!("/api/v1/public-ips/{name}")).await?;
            println!("name:          {}", ip.name);
            println!("address:       {}", ip.address);
            println!("prefix:        {}", ip.prefix);
            println!("target_kind:   {}", ip.target_kind);
            println!("target_name:   {}", ip.target_name);
            println!("label:         {}", fmt_opt(&ip.label));
            println!("phase:         {}", fmt_opt(&ip.phase));
            println!("message:       {}", fmt_opt(&ip.message));
        }
        PublicIpSubcommand::Create(args) => {
            let body = CreatePublicIpBody {
                name: args.name.clone(),
                address: args.address,
                prefix: args.prefix,
                target_kind: canonical_target_kind(&args.target_kind)?.to_string(),
                target_name: args.target_name,
                label: args.label,
            };
            let ip: PublicIpDetail = client.post("/api/v1/public-ips", &body).await?;
            println!("Reserved public IP '{}'", ip.name);
        }
        PublicIpSubcommand::Delete { name } => {
            client.delete(&format!("/api/v1/public-ips/{name}")).await?;
            println!("Released public IP '{name}'");
        }
    }
    Ok(())
}
