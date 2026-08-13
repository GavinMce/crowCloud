use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::client::CrowClient;

#[derive(Args)]
pub struct ExposeCmd {
    #[command(subcommand)]
    pub command: ExposeSubcommand,
}

#[derive(Subcommand)]
pub enum ExposeSubcommand {
    /// Expose a resource to the public internet (HTTP via Caddy, or a raw
    /// TCP/UDP port forward, both via VyOS)
    Create(CreateArgs),
    /// List exposed endpoints
    List,
    /// Show exposed endpoint details
    Get { name: String },
    /// Delete an exposed endpoint
    Delete { name: String },
}

#[derive(Args)]
pub struct CreateArgs {
    pub name: String,
    /// What kind of resource is being exposed. Only "vm" actually
    /// resolves a target IP today -- the others are accepted by the CRD
    /// schema for forward-compatibility but rejected by the API for now.
    #[arg(long, default_value = "vm")]
    pub target_kind: String,
    /// CR name of the target (e.g. the VM's resource id)
    #[arg(long)]
    pub target_name: String,
    /// http, tcp, or udp
    #[arg(long = "type")]
    pub expose_type: String,
    /// Required for --type http
    #[arg(long)]
    pub domain: Option<String>,
    #[arg(long)]
    pub port: u16,
    /// Defaults to --port for tcp/udp if omitted
    #[arg(long)]
    pub public_port: Option<u16>,
    /// tcp, udp, or tcp-udp -- overrides what --type would otherwise imply
    #[arg(long)]
    pub protocol: Option<String>,
    #[arg(long)]
    pub tls: bool,
}

#[derive(Serialize)]
struct CreateExposeBody {
    name: String,
    target_kind: String,
    target_name: String,
    expose_type: String,
    domain: Option<String>,
    port: u16,
    public_port: Option<u16>,
    protocol: Option<String>,
    tls: bool,
}

#[derive(Deserialize)]
struct ExposeRow {
    name: String,
    target_kind: String,
    target_name: String,
    expose_type: String,
    port: u16,
    phase: Option<String>,
    public_url: Option<String>,
}

#[derive(Deserialize)]
struct ExposeDetail {
    name: String,
    target_kind: String,
    target_name: String,
    expose_type: String,
    domain: Option<String>,
    port: u16,
    public_port: Option<u16>,
    protocol: Option<String>,
    tls: bool,
    phase: Option<String>,
    public_url: Option<String>,
    cert_expiry: Option<String>,
}

fn fmt_opt(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("—")
}

/// The API deserializes these straight into `crow-core`'s CRD enums, which
/// use their exact Rust variant name on the wire (e.g. `"VirtualMachine"`,
/// `"Http"`) -- this maps the natural lowercase CLI spelling to that, so a
/// user typing `--type http` doesn't get a confusing 400 for a casing
/// mismatch they'd have no way to guess. `pub(crate)`: `commands::public_ip`
/// reuses this directly rather than re-deriving the same mapping.
pub(crate) fn canonical_target_kind(s: &str) -> Result<&'static str> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "vm" | "virtualmachine" => "VirtualMachine",
        "k8s" | "k8scluster" => "K8sCluster",
        "objectstore" | "store" => "ObjectStore",
        "database" | "db" => "Database",
        other => bail!("unknown target-kind '{other}' (expected vm, k8s, store, or db)"),
    })
}

fn canonical_expose_type(s: &str) -> Result<&'static str> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "http" => "Http",
        "tcp" => "Tcp",
        "udp" => "Udp",
        other => bail!("unknown --type '{other}' (expected http, tcp, or udp)"),
    })
}

fn canonical_protocol(s: &str) -> Result<&'static str> {
    Ok(match s.to_ascii_lowercase().replace('-', "").as_str() {
        "tcp" => "Tcp",
        "udp" => "Udp",
        "tcpudp" => "TcpUdp",
        other => bail!("unknown --protocol '{other}' (expected tcp, udp, or tcp-udp)"),
    })
}

pub async fn run(cmd: ExposeCmd) -> Result<()> {
    let client = CrowClient::from_config(None)?;

    match cmd.command {
        ExposeSubcommand::List => {
            let endpoints: Vec<ExposeRow> = client.get("/api/v1/expose").await?;
            if endpoints.is_empty() {
                println!("No exposed endpoints.");
            } else {
                println!(
                    "{:<20}  {:<8}  {:<20}  {:<6}  {:<6}  {:<8}  PUBLIC_URL",
                    "NAME", "KIND", "TARGET", "TYPE", "PORT", "PHASE"
                );
                for e in &endpoints {
                    println!(
                        "{:<20}  {:<8}  {:<20}  {:<6}  {:<6}  {:<8}  {}",
                        e.name,
                        e.target_kind,
                        e.target_name,
                        e.expose_type,
                        e.port,
                        fmt_opt(&e.phase),
                        fmt_opt(&e.public_url)
                    );
                }
            }
        }
        ExposeSubcommand::Get { name } => {
            let e: ExposeDetail = client.get(&format!("/api/v1/expose/{name}")).await?;
            println!("name:          {}", e.name);
            println!("target_kind:   {}", e.target_kind);
            println!("target_name:   {}", e.target_name);
            println!("expose_type:   {}", e.expose_type);
            println!("domain:        {}", fmt_opt(&e.domain));
            println!("port:          {}", e.port);
            println!(
                "public_port:   {}",
                e.public_port.map(|p| p.to_string()).unwrap_or_default()
            );
            println!("protocol:      {}", fmt_opt(&e.protocol));
            println!("tls:           {}", e.tls);
            println!("phase:         {}", fmt_opt(&e.phase));
            println!("public_url:    {}", fmt_opt(&e.public_url));
            println!("cert_expiry:   {}", fmt_opt(&e.cert_expiry));
        }
        ExposeSubcommand::Create(args) => {
            let body = CreateExposeBody {
                name: args.name.clone(),
                target_kind: canonical_target_kind(&args.target_kind)?.to_string(),
                target_name: args.target_name,
                expose_type: canonical_expose_type(&args.expose_type)?.to_string(),
                domain: args.domain,
                port: args.port,
                public_port: args.public_port,
                protocol: args
                    .protocol
                    .as_deref()
                    .map(canonical_protocol)
                    .transpose()?
                    .map(String::from),
                tls: args.tls,
            };
            let e: ExposeDetail = client.post("/api/v1/expose", &body).await?;
            println!("Created exposed endpoint '{}'", e.name);
        }
        ExposeSubcommand::Delete { name } => {
            client.delete(&format!("/api/v1/expose/{name}")).await?;
            println!("Deleted exposed endpoint '{name}'");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_lowercase_target_kinds() {
        assert_eq!(canonical_target_kind("vm").unwrap(), "VirtualMachine");
        assert_eq!(
            canonical_target_kind("VirtualMachine").unwrap(),
            "VirtualMachine"
        );
        assert_eq!(canonical_target_kind("k8s").unwrap(), "K8sCluster");
        assert_eq!(canonical_target_kind("store").unwrap(), "ObjectStore");
        assert_eq!(canonical_target_kind("db").unwrap(), "Database");
    }

    #[test]
    fn rejects_an_unknown_target_kind() {
        assert!(canonical_target_kind("bogus").is_err());
    }

    #[test]
    fn maps_lowercase_expose_types() {
        assert_eq!(canonical_expose_type("http").unwrap(), "Http");
        assert_eq!(canonical_expose_type("tcp").unwrap(), "Tcp");
        assert_eq!(canonical_expose_type("udp").unwrap(), "Udp");
    }

    #[test]
    fn maps_lowercase_protocols_including_tcp_udp() {
        assert_eq!(canonical_protocol("tcp").unwrap(), "Tcp");
        assert_eq!(canonical_protocol("tcp-udp").unwrap(), "TcpUdp");
        assert_eq!(canonical_protocol("tcpudp").unwrap(), "TcpUdp");
    }
}
