use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::CrowClient;

#[derive(Args)]
pub struct SubnetCmd {
    #[command(subcommand)]
    pub command: SubnetSubcommand,
}

#[derive(Subcommand)]
pub enum SubnetSubcommand {
    /// Create a private subnet (VXLAN/EVPN dataplane on the provider)
    Create(CreateArgs),
    /// List private subnets
    List,
    /// Show private subnet details
    Get { name: String },
    /// Delete a private subnet (fails while any IP claims are still bound)
    Delete { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// Name of the infra provider to create the subnet on
    #[arg(long)]
    pub provider: String,
    /// Which of the provider's adopted nodes to create it on
    #[arg(long)]
    pub node: String,
    /// CIDR the subnet's addresses fall within (e.g. 10.30.0.0/24)
    #[arg(long)]
    pub cidr: String,
    /// VXLAN VNI -- must be unique across every private subnet, no
    /// central allocator exists yet
    #[arg(long)]
    pub vni: u32,
    #[arg(long)]
    pub gateway: String,
    /// DNS servers, comma-separated
    #[arg(long, value_delimiter = ',')]
    pub dns: Vec<String>,
}

#[derive(Serialize)]
struct CreateSubnetBody {
    name: String,
    infra_provider_ref: String,
    node: String,
    cidr: String,
    vni: u32,
    gateway: String,
    dns: Vec<String>,
}

#[derive(Deserialize)]
struct SubnetRow {
    name: String,
    cidr: String,
    vni: u32,
    node: String,
    phase: Option<String>,
}

#[derive(Deserialize)]
struct SubnetDetail {
    name: String,
    infra_provider_ref: String,
    node: String,
    cidr: String,
    vni: u32,
    gateway: String,
    dns: Vec<String>,
    bridge: Option<String>,
    ip_pool_ref: Option<String>,
    phase: Option<String>,
    message: Option<String>,
}

fn fmt_opt(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("—")
}

pub async fn run(cmd: SubnetCmd) -> Result<()> {
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        SubnetSubcommand::List => {
            let subnets: Vec<SubnetRow> = client.get("/api/v1/private-subnets").await?;
            if subnets.is_empty() {
                println!("No private subnets registered.");
            } else {
                println!(
                    "{:<24}  {:<18}  {:<10}  {:<12}  PHASE",
                    "NAME", "CIDR", "VNI", "NODE"
                );
                for s in &subnets {
                    println!(
                        "{:<24}  {:<18}  {:<10}  {:<12}  {}",
                        s.name,
                        s.cidr,
                        s.vni,
                        s.node,
                        fmt_opt(&s.phase)
                    );
                }
            }
        }
        SubnetSubcommand::Get { name } => {
            let s: SubnetDetail = client
                .get(&format!("/api/v1/private-subnets/{name}"))
                .await?;
            println!("name:              {}", s.name);
            println!("infra_provider:    {}", s.infra_provider_ref);
            println!("node:              {}", s.node);
            println!("cidr:              {}", s.cidr);
            println!("vni:               {}", s.vni);
            println!("gateway:           {}", s.gateway);
            println!("dns:               {}", s.dns.join(", "));
            println!("bridge:            {}", fmt_opt(&s.bridge));
            println!("ip_pool:           {}", fmt_opt(&s.ip_pool_ref));
            println!("phase:             {}", fmt_opt(&s.phase));
            println!("message:           {}", fmt_opt(&s.message));
        }
        SubnetSubcommand::Create(args) => {
            let body = CreateSubnetBody {
                name: args.name.clone(),
                infra_provider_ref: args.provider,
                node: args.node,
                cidr: args.cidr,
                vni: args.vni,
                gateway: args.gateway,
                dns: args.dns,
            };
            let s: SubnetDetail = client.post("/api/v1/private-subnets", &body).await?;
            println!("Created private subnet '{}'", s.name);
        }
        SubnetSubcommand::Delete { name } => {
            client
                .delete(&format!("/api/v1/private-subnets/{name}"))
                .await?;
            println!("Deleted private subnet '{name}'");
        }
    }
    Ok(())
}
