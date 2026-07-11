mod client;
mod error;

use std::net::{IpAddr, Ipv4Addr};

use async_trait::async_trait;
use crow_core::{
    traits::IpamProvider,
    types::{IpAllocHandle, IpAllocSpec},
    ProviderError,
};
use serde::Deserialize;
use serde_json::{json, Value};

use client::OPNsenseClient;
use error::OPNsenseError;

pub struct OPNsenseProvider {
    client: OPNsenseClient,
    /// CIDR of the Kea DHCPv4 subnet to attach reservations to, e.g.
    /// "192.168.100.0/24". Must already exist (Services > Kea DHCPv4 in the
    /// OPNsense UI) — this provider does not create subnets.
    subnet_cidr: String,
    range_start: Ipv4Addr,
    range_end: Ipv4Addr,
    base_url: String,
}

impl OPNsenseProvider {
    pub fn new(
        url: &str,
        api_key: &str,
        api_secret: &str,
        subnet_cidr: &str,
        range_start: Ipv4Addr,
        range_end: Ipv4Addr,
        tls_insecure: bool,
    ) -> Result<Self, ProviderError> {
        let client = OPNsenseClient::new(url, api_key, api_secret, tls_insecure)
            .map_err(ProviderError::from)?;
        Ok(Self {
            client,
            subnet_cidr: subnet_cidr.to_string(),
            range_start,
            range_end,
            base_url: url.trim_end_matches('/').to_string(),
        })
    }
}

// Verified live against a real OPNsense 25.x instance (Kea DHCPv4 backend,
// `/api/kea/dhcpv4/*` — NOT the legacy ISC `/api/dhcpv4/*` module some
// OPNsense installs still use). Confirmed via `getReservation`/`getSubnet`
// empty-form introspection: reservation field is `hw_address` (not `mac`),
// and reservations attach to a subnet by UUID, not by interface name.

#[derive(Debug, Deserialize)]
struct SearchResponse<T> {
    rows: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct SubnetRow {
    uuid: String,
    subnet: String,
}

#[derive(Debug, Deserialize)]
struct ReservationRow {
    ip_address: String,
}

#[derive(Debug, Deserialize)]
struct SavedResponse {
    result: String,
    uuid: Option<String>,
}

async fn find_subnet_uuid(
    client: &OPNsenseClient,
    subnet_cidr: &str,
) -> Result<String, OPNsenseError> {
    let resp: SearchResponse<SubnetRow> = client.get("/api/kea/dhcpv4/searchSubnet").await?;
    resp.rows
        .into_iter()
        .find(|r| r.subnet == subnet_cidr)
        .map(|r| r.uuid)
        .ok_or_else(|| OPNsenseError::Api {
            status: 404,
            message: format!(
                "no Kea DHCPv4 subnet '{subnet_cidr}' found — configure it in \
                     Services > Kea DHCPv4 first"
            ),
        })
}

async fn used_ips(
    client: &OPNsenseClient,
) -> Result<std::collections::HashSet<Ipv4Addr>, OPNsenseError> {
    let resp: SearchResponse<ReservationRow> =
        client.get("/api/kea/dhcpv4/searchReservation").await?;
    Ok(resp
        .rows
        .into_iter()
        .filter_map(|r| r.ip_address.parse::<Ipv4Addr>().ok())
        .collect())
}

fn next_free_ip(
    range_start: Ipv4Addr,
    range_end: Ipv4Addr,
    used: &std::collections::HashSet<Ipv4Addr>,
) -> Result<Ipv4Addr, OPNsenseError> {
    let start: u32 = range_start.into();
    let end: u32 = range_end.into();
    for raw in start..=end {
        let candidate = Ipv4Addr::from(raw);
        if !used.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(OPNsenseError::NoFreeIp)
}

async fn apply(client: &OPNsenseClient) -> Result<(), OPNsenseError> {
    let _: Value = client.post_empty("/api/kea/service/reconfigure").await?;
    Ok(())
}

#[async_trait]
impl IpamProvider for OPNsenseProvider {
    fn provider_type(&self) -> &'static str {
        "opnsense"
    }

    fn name(&self) -> &str {
        &self.base_url
    }

    async fn allocate_ip(&self, spec: IpAllocSpec) -> Result<IpAllocHandle, ProviderError> {
        let subnet_uuid = find_subnet_uuid(&self.client, &self.subnet_cidr)
            .await
            .map_err(ProviderError::from)?;
        let used = used_ips(&self.client).await.map_err(ProviderError::from)?;
        let ip =
            next_free_ip(self.range_start, self.range_end, &used).map_err(ProviderError::from)?;

        let body = json!({
            "reservation": {
                "subnet": subnet_uuid,
                "ip_address": ip.to_string(),
                "hw_address": spec.mac,
                "hostname": spec.hostname,
                "description": "crowCloud",
            }
        });
        let add: SavedResponse = self
            .client
            .post("/api/kea/dhcpv4/addReservation", &body)
            .await
            .map_err(ProviderError::from)?;
        if add.result != "saved" {
            return Err(ProviderError::Api(format!(
                "OPNsense did not confirm the reservation (result: {})",
                add.result
            )));
        }
        let uuid = add.uuid.ok_or_else(|| {
            ProviderError::Other("OPNsense did not return a reservation uuid".into())
        })?;

        apply(&self.client).await.map_err(ProviderError::from)?;

        Ok(IpAllocHandle {
            ip: IpAddr::V4(ip),
            mac: spec.mac,
            provider_id: uuid,
        })
    }

    async fn release_ip(&self, handle: &IpAllocHandle) -> Result<(), ProviderError> {
        let _: Value = self
            .client
            .post_empty(&format!(
                "/api/kea/dhcpv4/delReservation/{}",
                handle.provider_id
            ))
            .await
            .map_err(ProviderError::from)?;

        apply(&self.client).await.map_err(ProviderError::from)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_first_free_ip_in_range() {
        let used = std::collections::HashSet::from(["10.0.0.10".parse().unwrap()]);
        let ip = next_free_ip(
            "10.0.0.10".parse().unwrap(),
            "10.0.0.12".parse().unwrap(),
            &used,
        )
        .unwrap();
        assert_eq!(ip, "10.0.0.11".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn errors_when_range_exhausted() {
        let used = std::collections::HashSet::from(["10.0.0.10".parse().unwrap()]);
        let err = next_free_ip(
            "10.0.0.10".parse().unwrap(),
            "10.0.0.10".parse().unwrap(),
            &used,
        )
        .unwrap_err();
        assert!(matches!(err, OPNsenseError::NoFreeIp));
    }

    /// Live round-trip against a real OPNsense instance. Ignored by default
    /// (not run in CI — no OPNsense instance there); run manually with:
    ///   OPNSENSE_URL=... OPNSENSE_KEY=... OPNSENSE_SECRET=... OPNSENSE_SUBNET_CIDR=... \
    ///     cargo test -p crow-provider-opnsense -- --ignored
    #[tokio::test]
    #[ignore]
    async fn allocate_and_release_ip_live() {
        let url = std::env::var("OPNSENSE_URL").expect("OPNSENSE_URL");
        let key = std::env::var("OPNSENSE_KEY").expect("OPNSENSE_KEY");
        let secret = std::env::var("OPNSENSE_SECRET").expect("OPNSENSE_SECRET");
        let subnet_cidr = std::env::var("OPNSENSE_SUBNET_CIDR").expect("OPNSENSE_SUBNET_CIDR");

        let provider = OPNsenseProvider::new(
            &url,
            &key,
            &secret,
            &subnet_cidr,
            "192.168.100.150".parse().unwrap(),
            "192.168.100.199".parse().unwrap(),
            true,
        )
        .expect("build provider");

        let handle = provider
            .allocate_ip(IpAllocSpec {
                hostname: "crowcloud-live-test".to_string(),
                mac: "52:54:00:aa:bb:cd".to_string(),
            })
            .await
            .expect("allocate_ip");

        assert!(handle.ip >= IpAddr::V4("192.168.100.150".parse().unwrap()));
        assert!(handle.ip <= IpAddr::V4("192.168.100.199".parse().unwrap()));

        provider.release_ip(&handle).await.expect("release_ip");
    }
}
