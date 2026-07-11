use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use crow_core::{
    traits::{ProvisionCtx, ResourceDriver},
    types::{
        CloudInitConfig, Endpoint, IpAllocHandle, IpAllocSpec, ResourceHandle, ResourcePhase,
        VmHandle, VmSpec, VmStatus,
    },
    DriverError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct K8sClusterDriver;

/// Shape `ProvisionCtx.config` is expected to hold for a K8sCluster resource
/// (built by the operator from `K8sClusterSpec`, GiB fields already
/// converted to MiB for the control-plane/worker groups).
#[derive(Debug, Deserialize)]
struct K8sClusterProvisionConfig {
    distribution: String,
    #[serde(default)]
    version: String,
    image: String,
    vip: Option<String>,
    control_plane: NodeGroupConfig,
    workers: NodeGroupConfig,
}

#[derive(Debug, Deserialize)]
struct NodeGroupConfig {
    count: u32,
    cpu: u32,
    memory_mib: u64,
    disk_gib: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeHandle {
    vm: VmHandle,
    ip_alloc: Option<IpAllocHandle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct K8sClusterHandle {
    control_plane: Vec<NodeHandle>,
    workers: Vec<NodeHandle>,
    vip: Option<String>,
    token: String,
}

fn generate_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// QEMU-prefixed (`52:54:00:...`) locally-administered MAC, matching
/// Proxmox's own auto-generation range so pinned MACs don't collide with
/// ones Proxmox would have picked itself.
fn generate_mac() -> String {
    let id = uuid::Uuid::new_v4();
    let b = id.as_bytes();
    format!("52:54:00:{:02x}:{:02x}:{:02x}", b[0], b[1], b[2])
}

/// A `#!`-prefixed cloud-init user-data script (NoCloud executes these
/// directly rather than treating them as `#cloud-config` YAML).
fn kube_vip_manifest(vip: &str) -> String {
    format!(
        r#"apiVersion: v1
kind: Pod
metadata:
  name: kube-vip
  namespace: kube-system
spec:
  hostNetwork: true
  containers:
    - name: kube-vip
      image: ghcr.io/kube-vip/kube-vip:v0.8.0
      imagePullPolicy: IfNotPresent
      args: ["manager"]
      env:
        - name: vip_arp
          value: "true"
        - name: port
          value: "6443"
        - name: vip_interface
          value: "eth0"
        - name: vip_cidr
          value: "32"
        - name: cp_enable
          value: "true"
        - name: cp_namespace
          value: "kube-system"
        - name: svc_enable
          value: "false"
        - name: vip_leaderelection
          value: "true"
        - name: address
          value: "{vip}"
      securityContext:
        capabilities:
          add: ["NET_ADMIN", "NET_RAW"]
      volumeMounts:
        - mountPath: /etc/kubernetes/admin.conf
          name: kubeconfig
  volumes:
    - name: kubeconfig
      hostPath:
        path: /etc/rancher/k3s/k3s.yaml
"#
    )
}

fn version_env(version: &str) -> String {
    if version.is_empty() {
        String::new()
    } else {
        format!("INSTALL_K3S_VERSION=\"{version}\" ")
    }
}

fn k3s_server_init_script(token: &str, vip: Option<&str>, version: &str) -> String {
    let mut script = String::from("#!/bin/sh\nset -eu\n");
    let mut tls_san = String::new();
    if let Some(vip) = vip {
        tls_san = format!(" --tls-san {vip}");
        script.push_str("mkdir -p /var/lib/rancher/k3s/server/manifests\n");
        script.push_str(&format!(
            "cat > /var/lib/rancher/k3s/server/manifests/kube-vip.yaml <<'EOF'\n{}\nEOF\n",
            kube_vip_manifest(vip)
        ));
    }
    let version_env = version_env(version);
    script.push_str(&format!(
        "curl -sfL https://get.k3s.io | {version_env}INSTALL_K3S_EXEC=\"server --cluster-init{tls_san} --token {token}\" sh -\n"
    ));
    script
}

fn k3s_server_join_script(
    token: &str,
    vip: Option<&str>,
    join_ip: IpAddr,
    version: &str,
) -> String {
    let mut script = String::from("#!/bin/sh\nset -eu\n");
    let mut tls_san = String::new();
    if let Some(vip) = vip {
        tls_san = format!(" --tls-san {vip}");
        script.push_str("mkdir -p /var/lib/rancher/k3s/server/manifests\n");
        script.push_str(&format!(
            "cat > /var/lib/rancher/k3s/server/manifests/kube-vip.yaml <<'EOF'\n{}\nEOF\n",
            kube_vip_manifest(vip)
        ));
    }
    let version_env = version_env(version);
    script.push_str(&format!(
        "curl -sfL https://get.k3s.io | {version_env}INSTALL_K3S_EXEC=\"server --server https://{join_ip}:6443{tls_san} --token {token}\" sh -\n"
    ));
    script
}

fn k3s_agent_join_script(token: &str, join_addr: &str, version: &str) -> String {
    let version_env = version_env(version);
    format!(
        "#!/bin/sh\nset -eu\ncurl -sfL https://get.k3s.io | {version_env}K3S_URL=\"https://{join_addr}:6443\" K3S_TOKEN=\"{token}\" sh -\n"
    )
}

async fn allocate_node_ip(
    ctx: &ProvisionCtx,
    hostname: &str,
) -> Result<(String, IpAllocHandle), DriverError> {
    let ipam = ctx.ipam.as_ref().ok_or_else(|| {
        DriverError::InvalidConfig(
            "K8sCluster requires an IPAM provider (ip_pool_ref must reference one)".to_string(),
        )
    })?;
    let mac = generate_mac();
    let ip_alloc = ipam
        .allocate_ip(IpAllocSpec {
            hostname: hostname.to_string(),
            mac: mac.clone(),
        })
        .await?;
    Ok((mac, ip_alloc))
}

/// Creates the VM with only Proxmox-native cloud-init (static IP via
/// `ipconfig0`, hostname via the VM name — both handled by Proxmox itself,
/// no snippet upload involved) then, once the guest agent answers, runs
/// `script` inside it to actually bootstrap k3s.
///
/// This deliberately avoids `VmSpec.cloud_init.user_data` (delivered via a
/// Proxmox cloud-init *snippet*, uploaded through
/// `/nodes/{node}/storage/{storage}/upload`): on Proxmox VE 9.2 that
/// endpoint's `content` parameter hard-rejects `snippets` (`"value
/// 'snippets' does not have a value in the enumeration 'iso, vztmpl,
/// import'"`), confirmed directly against the real API — no config on the
/// storage side can work around it. Running the script post-boot over the
/// QEMU guest agent (already used for readyz/kubeconfig) sidesteps the
/// broken endpoint entirely.
async fn create_node(
    ctx: &ProvisionCtx,
    cfg: &NodeGroupConfig,
    image: &str,
    hostname: String,
    script: String,
) -> Result<NodeHandle, DriverError> {
    let (mac, ip_alloc) = allocate_node_ip(ctx, &hostname).await?;
    let spec = VmSpec {
        name: hostname.clone(),
        cpu: cfg.cpu,
        memory_mib: cfg.memory_mib,
        disk_gib: cfg.disk_gib,
        image: image.to_string(),
        ip: Some(ip_alloc.ip),
        mac: Some(mac),
        cloud_init: Some(CloudInitConfig {
            hostname,
            user_data: None,
            network_config: None,
        }),
        network_ref: None,
    };
    let vm = match ctx.infra.create_vm(spec).await {
        Ok(vm) => vm,
        Err(e) => {
            release_ip(ctx, &ip_alloc).await;
            return Err(e.into());
        }
    };

    let bootstrap_result = match wait_for_agent(ctx, &vm, 60).await {
        Ok(()) => ctx.infra.exec_in_vm(&vm, &script).await.map_err(Into::into),
        Err(e) => Err(e),
    };
    if let Err(e) = bootstrap_result {
        if let Err(del_err) = ctx.infra.delete_vm(&vm).await {
            tracing::warn!(
                provider_id = %vm.provider_id,
                "failed to clean up node VM after bootstrap failure: {del_err}"
            );
        }
        release_ip(ctx, &ip_alloc).await;
        return Err(e);
    }

    Ok(NodeHandle {
        vm,
        ip_alloc: Some(ip_alloc),
    })
}

async fn release_ip(ctx: &ProvisionCtx, ip_alloc: &IpAllocHandle) {
    if let Some(ipam) = &ctx.ipam {
        if let Err(e) = ipam.release_ip(ip_alloc).await {
            tracing::warn!(ip = %ip_alloc.ip, "failed to release node IP after failure: {e}");
        }
    }
}

/// Polls until the QEMU guest agent answers a trivial command, or gives up
/// after `attempts * 5s`. `start_vm` only confirms the QEMU process is
/// running, not that the guest OS has booted far enough for
/// qemu-guest-agent to be listening.
async fn wait_for_agent(
    ctx: &ProvisionCtx,
    vm: &VmHandle,
    attempts: u32,
) -> Result<(), DriverError> {
    for _ in 0..attempts {
        if ctx.infra.exec_in_vm(vm, "true").await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err(DriverError::ProvisionFailed(format!(
        "guest agent on node {} did not become ready in time",
        vm.provider_id
    )))
}

/// Polls a control-plane node's k3s API server until it reports ready, or
/// gives up after `attempts * 5s`.
async fn wait_for_readyz(
    ctx: &ProvisionCtx,
    vm: &VmHandle,
    attempts: u32,
) -> Result<(), DriverError> {
    for _ in 0..attempts {
        if let Ok(out) = ctx
            .infra
            .exec_in_vm(vm, "k3s kubectl get --raw /readyz 2>/dev/null || true")
            .await
        {
            if out.trim() == "ok" {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err(DriverError::ProvisionFailed(format!(
        "control-plane node {} did not become ready in time",
        vm.provider_id
    )))
}

#[async_trait]
impl ResourceDriver for K8sClusterDriver {
    fn resource_type(&self) -> &'static str {
        "K8sCluster"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["image", "control_plane", "workers"],
            "properties": {
                "image": { "type": "string" },
                "vip": { "type": "string" },
                "control_plane": {
                    "type": "object",
                    "required": ["count", "cpu", "memory_mib", "disk_gib"],
                    "properties": {
                        "count": { "type": "integer", "minimum": 1 },
                        "cpu": { "type": "integer", "minimum": 1 },
                        "memory_mib": { "type": "integer", "minimum": 1 },
                        "disk_gib": { "type": "integer", "minimum": 1 }
                    }
                },
                "workers": {
                    "type": "object",
                    "required": ["count", "cpu", "memory_mib", "disk_gib"],
                    "properties": {
                        "count": { "type": "integer", "minimum": 0 },
                        "cpu": { "type": "integer", "minimum": 1 },
                        "memory_mib": { "type": "integer", "minimum": 1 },
                        "disk_gib": { "type": "integer", "minimum": 1 }
                    }
                }
            }
        })
    }

    async fn provision(&self, ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError> {
        let cfg: K8sClusterProvisionConfig = serde_json::from_value(ctx.config.clone())
            .map_err(|e| DriverError::InvalidConfig(format!("invalid K8sCluster config: {e}")))?;

        if cfg.distribution != "K3s" {
            return Err(DriverError::InvalidConfig(format!(
                "distribution '{}' is not implemented — only K3s is supported",
                cfg.distribution
            )));
        }

        if cfg.control_plane.count > 1 && cfg.vip.is_none() {
            return Err(DriverError::InvalidConfig(
                "control_plane.vip is required when control_plane.count > 1".to_string(),
            ));
        }

        let token = generate_token();
        let mut control_plane: Vec<NodeHandle> =
            Vec::with_capacity(cfg.control_plane.count as usize);
        let mut node0_ip: Option<IpAddr> = None;

        for i in 0..cfg.control_plane.count {
            let hostname = format!("{}-cp-{i}", ctx.resource_name);
            let user_data = match node0_ip {
                None => k3s_server_init_script(&token, cfg.vip.as_deref(), &cfg.version),
                Some(ip) => k3s_server_join_script(&token, cfg.vip.as_deref(), ip, &cfg.version),
            };
            let node =
                create_node(ctx, &cfg.control_plane, &cfg.image, hostname, user_data).await?;

            if node0_ip.is_none() {
                node0_ip = node.ip_alloc.as_ref().map(|a| a.ip);
                wait_for_readyz(ctx, &node.vm, 60).await?;
            }
            control_plane.push(node);
        }

        let join_addr = cfg
            .vip
            .clone()
            .or_else(|| node0_ip.map(|ip| ip.to_string()))
            .ok_or_else(|| {
                DriverError::Other("no control-plane join address available".to_string())
            })?;

        let mut workers: Vec<NodeHandle> = Vec::with_capacity(cfg.workers.count as usize);
        for i in 0..cfg.workers.count {
            let hostname = format!("{}-worker-{i}", ctx.resource_name);
            let user_data = k3s_agent_join_script(&token, &join_addr, &cfg.version);
            let node = create_node(ctx, &cfg.workers, &cfg.image, hostname, user_data).await?;
            workers.push(node);
        }

        let handle = K8sClusterHandle {
            control_plane,
            workers,
            vip: cfg.vip,
            token,
        };
        Ok(ResourceHandle {
            resource_type: self.resource_type().to_string(),
            data: serde_json::to_value(&handle).map_err(|e| {
                DriverError::Other(format!("failed to encode K8sCluster handle: {e}"))
            })?,
        })
    }

    async fn deprovision(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<(), DriverError> {
        let h = decode_handle(handle)?;
        for node in h.control_plane.iter().chain(h.workers.iter()) {
            if let Err(e) = ctx.infra.delete_vm(&node.vm).await {
                tracing::warn!(provider_id = %node.vm.provider_id, "failed to delete K8sCluster node: {e}");
            }
            if let (Some(ipam), Some(alloc)) = (&ctx.ipam, &node.ip_alloc) {
                if let Err(e) = ipam.release_ip(alloc).await {
                    tracing::warn!(ip = %alloc.ip, "failed to release K8sCluster node IP: {e}");
                }
            }
        }
        Ok(())
    }

    async fn reconcile(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError> {
        let h = decode_handle(handle)?;

        for node in h.control_plane.iter().chain(h.workers.iter()) {
            match ctx.infra.vm_status(&node.vm).await? {
                VmStatus::Running => {}
                VmStatus::Error(msg) => return Ok(ResourcePhase::Failed(msg)),
                _ => return Ok(ResourcePhase::ProvisioningInfra),
            }
        }

        let node0 = h.control_plane.first().ok_or_else(|| {
            DriverError::Other("K8sCluster handle has no control-plane nodes".to_string())
        })?;
        match ctx
            .infra
            .exec_in_vm(
                &node0.vm,
                "k3s kubectl get --raw /readyz 2>/dev/null || true",
            )
            .await
        {
            Ok(out) if out.trim() == "ok" => Ok(ResourcePhase::Ready),
            _ => Ok(ResourcePhase::Degraded(
                "k3s API server not responding".to_string(),
            )),
        }
    }

    async fn endpoints(&self, handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError> {
        let h = decode_handle(handle)?;
        let addr = h.vip.clone().or_else(|| {
            h.control_plane
                .first()
                .and_then(|n| n.ip_alloc.as_ref())
                .map(|a| a.ip.to_string())
        });
        Ok(match addr {
            Some(addr) => vec![Endpoint {
                name: "kubernetes-api".to_string(),
                url: format!("https://{addr}:6443"),
                description: Some("k3s API server".to_string()),
            }],
            None => vec![],
        })
    }

    async fn credentials(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<Value, DriverError> {
        let h = decode_handle(handle)?;
        let node0 = h.control_plane.first().ok_or_else(|| {
            DriverError::Other("K8sCluster handle has no control-plane nodes".to_string())
        })?;

        let raw = ctx
            .infra
            .exec_in_vm(&node0.vm, "cat /etc/rancher/k3s/k3s.yaml")
            .await?;

        let server_addr = h
            .vip
            .clone()
            .or_else(|| node0.ip_alloc.as_ref().map(|a| a.ip.to_string()))
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let kubeconfig = raw.replace("127.0.0.1", &server_addr);

        Ok(serde_json::json!({ "kubeconfig": kubeconfig }))
    }
}

fn decode_handle(handle: &ResourceHandle) -> Result<K8sClusterHandle, DriverError> {
    serde_json::from_value(handle.data.clone())
        .map_err(|e| DriverError::Other(format!("corrupt K8sCluster resource handle: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crow_core::{
        traits::{InfraProvider, IpamProvider},
        types::{NetworkHandle, NetworkSpec, VolumeHandle, VolumeSpec},
        ProviderError,
    };
    use std::sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    };

    struct MockInfraProvider {
        vm_status: VmStatus,
    }

    #[async_trait]
    impl InfraProvider for MockInfraProvider {
        fn provider_type(&self) -> &'static str {
            "mock"
        }
        fn name(&self) -> &str {
            "mock"
        }
        async fn create_vm(&self, spec: VmSpec) -> Result<VmHandle, ProviderError> {
            Ok(VmHandle {
                provider_type: "mock".to_string(),
                provider_id: spec.name.clone(),
                ip: spec.ip,
                name: spec.name,
            })
        }
        async fn delete_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn vm_status(&self, _handle: &VmHandle) -> Result<VmStatus, ProviderError> {
            Ok(self.vm_status.clone())
        }
        async fn start_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn stop_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn exec_in_vm(
            &self,
            _handle: &VmHandle,
            command: &str,
        ) -> Result<String, ProviderError> {
            if command.contains("readyz") {
                Ok("ok".to_string())
            } else {
                Ok("server: https://127.0.0.1:6443\n".to_string())
            }
        }
        async fn create_volume(&self, _spec: VolumeSpec) -> Result<VolumeHandle, ProviderError> {
            unimplemented!()
        }
        async fn delete_volume(&self, _handle: &VolumeHandle) -> Result<(), ProviderError> {
            unimplemented!()
        }
        async fn create_network(&self, _spec: NetworkSpec) -> Result<NetworkHandle, ProviderError> {
            unimplemented!()
        }
        async fn delete_network(&self, _handle: &NetworkHandle) -> Result<(), ProviderError> {
            unimplemented!()
        }
    }

    struct MockIpamProvider {
        next_octet: AtomicU8,
    }

    #[async_trait]
    impl IpamProvider for MockIpamProvider {
        fn provider_type(&self) -> &'static str {
            "mock"
        }
        fn name(&self) -> &str {
            "mock"
        }
        async fn allocate_ip(&self, spec: IpAllocSpec) -> Result<IpAllocHandle, ProviderError> {
            let n = self.next_octet.fetch_add(1, Ordering::SeqCst);
            Ok(IpAllocHandle {
                ip: format!("10.0.0.{n}").parse().unwrap(),
                mac: spec.mac,
                provider_id: format!("mapping-{n}"),
            })
        }
        async fn release_ip(&self, _handle: &IpAllocHandle) -> Result<(), ProviderError> {
            Ok(())
        }
    }

    fn ctx_with(
        infra: Arc<dyn InfraProvider>,
        ipam: Arc<dyn IpamProvider>,
        config: Value,
    ) -> ProvisionCtx {
        ProvisionCtx {
            infra,
            network: None,
            dns: None,
            ipam: Some(ipam),
            config,
            project: "proj".to_string(),
            resource_group: "rg".to_string(),
            resource_name: "my-cluster".to_string(),
        }
    }

    #[test]
    fn mac_is_qemu_prefixed() {
        let mac = generate_mac();
        assert!(mac.starts_with("52:54:00:"));
        assert_eq!(mac.split(':').count(), 6);
    }

    #[test]
    fn handle_round_trips_through_json() {
        let handle = K8sClusterHandle {
            control_plane: vec![NodeHandle {
                vm: VmHandle {
                    provider_type: "mock".to_string(),
                    provider_id: "100".to_string(),
                    ip: Some("10.0.0.5".parse().unwrap()),
                    name: "cp-0".to_string(),
                },
                ip_alloc: Some(IpAllocHandle {
                    ip: "10.0.0.5".parse().unwrap(),
                    mac: "52:54:00:00:00:01".to_string(),
                    provider_id: "map-1".to_string(),
                }),
            }],
            workers: vec![],
            vip: Some("10.0.0.100".to_string()),
            token: "tok".to_string(),
        };
        let json = serde_json::to_value(&handle).unwrap();
        let decoded: K8sClusterHandle = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.token, "tok");
        assert_eq!(decoded.control_plane.len(), 1);
    }

    #[tokio::test]
    async fn provision_rejects_ha_without_vip() {
        let infra = Arc::new(MockInfraProvider {
            vm_status: VmStatus::Running,
        });
        let ipam = Arc::new(MockIpamProvider {
            next_octet: AtomicU8::new(10),
        });
        let ctx = ctx_with(
            infra,
            ipam,
            serde_json::json!({
                "distribution": "K3s",
                "image": "9000",
                "control_plane": {"count": 3, "cpu": 2, "memory_mib": 2048, "disk_gib": 20},
                "workers": {"count": 0, "cpu": 1, "memory_mib": 1024, "disk_gib": 10}
            }),
        );
        let err = K8sClusterDriver.provision(&ctx).await.unwrap_err();
        assert!(matches!(err, DriverError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn provision_reconcile_endpoints_credentials_single_node() {
        let infra = Arc::new(MockInfraProvider {
            vm_status: VmStatus::Running,
        });
        let ipam = Arc::new(MockIpamProvider {
            next_octet: AtomicU8::new(10),
        });
        let ctx = ctx_with(
            infra,
            ipam,
            serde_json::json!({
                "distribution": "K3s",
                "image": "9000",
                "control_plane": {"count": 1, "cpu": 2, "memory_mib": 2048, "disk_gib": 20},
                "workers": {"count": 1, "cpu": 1, "memory_mib": 1024, "disk_gib": 10}
            }),
        );

        let handle = K8sClusterDriver.provision(&ctx).await.unwrap();
        let decoded: K8sClusterHandle = serde_json::from_value(handle.data.clone()).unwrap();
        assert_eq!(decoded.control_plane.len(), 1);
        assert_eq!(decoded.workers.len(), 1);
        assert_ne!(
            decoded.control_plane[0].ip_alloc.as_ref().unwrap().ip,
            decoded.workers[0].ip_alloc.as_ref().unwrap().ip
        );

        let phase = K8sClusterDriver.reconcile(&ctx, &handle).await.unwrap();
        assert_eq!(phase, ResourcePhase::Ready);

        let endpoints = K8sClusterDriver.endpoints(&handle).await.unwrap();
        assert_eq!(endpoints.len(), 1);
        assert!(endpoints[0].url.starts_with("https://10.0.0."));

        let creds = K8sClusterDriver.credentials(&ctx, &handle).await.unwrap();
        let kubeconfig = creds["kubeconfig"].as_str().unwrap();
        assert!(!kubeconfig.contains("127.0.0.1"));
        assert!(kubeconfig.contains("10.0.0."));
    }
}
