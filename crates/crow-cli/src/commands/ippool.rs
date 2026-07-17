use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::CrowClient;

#[derive(Args)]
pub struct IpPoolCmd {
    #[command(subcommand)]
    pub command: IpPoolSubcommand,
}

#[derive(Subcommand)]
pub enum IpPoolSubcommand {
    /// Create an IP pool
    Create(CreateArgs),
    /// List IP pools
    List,
    /// Show IP pool details
    Get { name: String },
    /// Delete an IP pool (fails if addresses are still allocated)
    Delete { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// CIDR the pool's addresses fall within (e.g. 10.20.0.0/24)
    #[arg(long)]
    pub cidr: String,
    /// First allocatable address in the range
    #[arg(long)]
    pub range_start: String,
    /// Last allocatable address in the range
    #[arg(long)]
    pub range_end: String,
    #[arg(long)]
    pub gateway: String,
    /// DNS servers, comma-separated
    #[arg(long, value_delimiter = ',')]
    pub dns: Vec<String>,
    /// Bridge to attach allocated addresses to (e.g. vmbr0)
    #[arg(long)]
    pub bridge: String,
}

#[derive(Serialize)]
struct CreateIpPoolBody {
    name: String,
    cidr: String,
    range_start: String,
    range_end: String,
    gateway: String,
    dns: Vec<String>,
    bridge: String,
}

#[derive(Deserialize)]
struct IpPoolRow {
    name: String,
    cidr: String,
    range_start: String,
    range_end: String,
    allocated: Option<u32>,
    available: Option<u32>,
}

#[derive(Deserialize)]
struct IpPoolDetail {
    name: String,
    cidr: String,
    range_start: String,
    range_end: String,
    gateway: String,
    dns: Vec<String>,
    bridge: String,
    allocated: Option<u32>,
    available: Option<u32>,
}

fn fmt_counter(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "—".to_string())
}

pub async fn run(cmd: IpPoolCmd) -> Result<()> {
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        IpPoolSubcommand::List => {
            let pools: Vec<IpPoolRow> = client.get("/api/v1/ip-pools").await?;
            if pools.is_empty() {
                println!("No IP pools registered.");
            } else {
                println!(
                    "{:<24}  {:<18}  {:<15}  {:<15}  ALLOC  AVAIL",
                    "NAME", "CIDR", "RANGE START", "RANGE END"
                );
                for p in &pools {
                    println!(
                        "{:<24}  {:<18}  {:<15}  {:<15}  {:<5}  {}",
                        p.name,
                        p.cidr,
                        p.range_start,
                        p.range_end,
                        fmt_counter(p.allocated),
                        fmt_counter(p.available)
                    );
                }
            }
        }
        IpPoolSubcommand::Get { name } => {
            let p: IpPoolDetail = client.get(&format!("/api/v1/ip-pools/{name}")).await?;
            println!("name:        {}", p.name);
            println!("cidr:        {}", p.cidr);
            println!("range:       {} - {}", p.range_start, p.range_end);
            println!("gateway:     {}", p.gateway);
            println!("dns:         {}", p.dns.join(", "));
            println!("bridge:      {}", p.bridge);
            println!("allocated:   {}", fmt_counter(p.allocated));
            println!("available:   {}", fmt_counter(p.available));
        }
        IpPoolSubcommand::Create(args) => {
            let body = CreateIpPoolBody {
                name: args.name.clone(),
                cidr: args.cidr,
                range_start: args.range_start,
                range_end: args.range_end,
                gateway: args.gateway,
                dns: args.dns,
                bridge: args.bridge,
            };
            let p: IpPoolDetail = client.post("/api/v1/ip-pools", &body).await?;
            println!("Created IP pool '{}'", p.name);
        }
        IpPoolSubcommand::Delete { name } => {
            client.delete(&format!("/api/v1/ip-pools/{name}")).await?;
            println!("Deleted IP pool '{name}'");
        }
    }
    Ok(())
}
