mod cloud_init;

use async_trait::async_trait;
use chrono::Utc;
use crow_core::{
    traits::{ProvisionCtx, ResourceDriver},
    types::{CloudInitConfig, Endpoint, K8sClusterHandle, ResourceHandle, ResourcePhase, VmSpec},
    DriverError,
};
use serde::Deserialize;
use serde_json::Value;
use std::net::IpAddr;

pub use cloud_init::generate_token;

pub struct K8sClusterDriver;

/// A resolved static address for one node — mirrors `crow-resource-vm`'s
/// own network fields, but K8sCluster needs one per node (control plane
/// *and* every worker; see `K8sClusterProvisionConfig`), not once per
/// resource. All nodes are static, not just the control plane — DHCP on
/// the target network proved unreliable in practice (new MAC addresses
/// observed never getting a lease at all), and crowCloud already owns the
/// pool capacity to just not depend on it.
#[derive(Debug, Deserialize)]
struct NodeAddress {
    ip: IpAddr,
    prefix_len: u8,
    gateway: IpAddr,
    #[serde(default)]
    dns: Vec<String>,
    bridge: String,
}

/// Shape `ProvisionCtx.config` is expected to hold — built by the
/// operator's `k8s_cluster` controller after it resolves an `IpClaim` for
/// the control plane and one more per worker (all from the same pool).
/// Worker count is implicit in `workers.len()`.
#[derive(Debug, Deserialize)]
struct K8sClusterProvisionConfig {
    image: String,
    #[serde(default)]
    k3s_version: String,
    control_plane: NodeAddress,
    control_plane_cpu: u32,
    control_plane_memory_gib: u32,
    control_plane_disk_gib: u32,
    #[serde(default)]
    workers: Vec<NodeAddress>,
    worker_cpu: u32,
    worker_memory_gib: u32,
    worker_disk_gib: u32,
    pod_cidr: String,
    service_cidr: String,
    lb_pool_cidr: Option<String>,
    #[serde(default)]
    monitoring: bool,
    callback_url: String,
    cluster_token: String,
    bootstrap_secret: String,
    /// Injected into every node's `authorized_keys` before anything else in
    /// the cloud-init script runs, so a bootstrap failure is still
    /// debuggable afterward. Comes from the host's `ssh_public_key` config;
    /// absent for most hosts.
    #[serde(default)]
    debug_ssh_public_key: Option<String>,
}

/// How long to wait for the bootstrap callback before giving up and
/// reporting `Failed` — `set -euo pipefail` in the cloud-init script means
/// any failing step aborts before ever reaching the callback, so without
/// this a broken bootstrap would sit at "Bootstrapping" forever with no
/// visible error. Needs enough headroom for both Helm installs' own
/// `--timeout 15m` (see `cloud_init::render_control_plane_script`) to each
/// genuinely run their course under slow/no-KVM environments, not just
/// for the happy path.
const BOOTSTRAP_TIMEOUT_MINUTES: i64 = 45;

fn deserialize_handle(handle: &ResourceHandle) -> Result<K8sClusterHandle, DriverError> {
    serde_json::from_value(handle.data.clone())
        .map_err(|e| DriverError::Other(format!("corrupt K8sCluster resource handle: {e}")))
}

#[async_trait]
impl ResourceDriver for K8sClusterDriver {
    fn resource_type(&self) -> &'static str {
        "K8sCluster"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": [
                "image", "control_plane", "control_plane_cpu", "control_plane_memory_gib",
                "control_plane_disk_gib", "worker_cpu", "worker_memory_gib",
                "worker_disk_gib", "pod_cidr", "service_cidr",
                "callback_url", "cluster_token", "bootstrap_secret"
            ],
            "properties": {
                "image": { "type": "string" },
                "k3s_version": { "type": "string" },
                "control_plane_cpu": { "type": "integer", "minimum": 1 },
                "control_plane_memory_gib": { "type": "integer", "minimum": 1 },
                "control_plane_disk_gib": { "type": "integer", "minimum": 1 },
                "workers": { "type": "array" },
                "worker_cpu": { "type": "integer", "minimum": 1 },
                "worker_memory_gib": { "type": "integer", "minimum": 1 },
                "worker_disk_gib": { "type": "integer", "minimum": 1 },
                "pod_cidr": { "type": "string" },
                "service_cidr": { "type": "string" },
                "lb_pool_cidr": { "type": "string" },
                "monitoring": { "type": "boolean" }
            }
        })
    }

    async fn provision(&self, ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError> {
        let cfg: K8sClusterProvisionConfig = serde_json::from_value(ctx.config.clone())
            .map_err(|e| DriverError::InvalidConfig(format!("invalid K8sCluster config: {e}")))?;

        let cp_name = format!("{}-cp", ctx.resource_name);
        let cp_network_config = cloud_init::render_network_config(
            cfg.control_plane.ip,
            cfg.control_plane.prefix_len,
            cfg.control_plane.gateway,
            &cfg.control_plane.dns,
        );
        let cp_script =
            cloud_init::render_control_plane_script(&cloud_init::ControlPlaneScriptInput {
                node_name: &cp_name,
                k3s_version: &cfg.k3s_version,
                cluster_token: &cfg.cluster_token,
                node_ip: &cfg.control_plane.ip.to_string(),
                pod_cidr: &cfg.pod_cidr,
                service_cidr: &cfg.service_cidr,
                lb_pool_cidr: cfg.lb_pool_cidr.as_deref(),
                monitoring: cfg.monitoring,
                callback_url: &cfg.callback_url,
                bootstrap_secret: &cfg.bootstrap_secret,
                debug_ssh_public_key: cfg.debug_ssh_public_key.as_deref(),
            });

        let control_plane = ctx
            .infra
            .create_vm(VmSpec {
                name: cp_name.clone(),
                cpu: cfg.control_plane_cpu,
                memory_mib: (cfg.control_plane_memory_gib as u64) * 1024,
                disk_gib: cfg.control_plane_disk_gib as u64,
                image: cfg.image.clone(),
                ip: Some(cfg.control_plane.ip),
                cloud_init: Some(CloudInitConfig {
                    hostname: cp_name,
                    user_data: Some(cp_script),
                    network_config: Some(cp_network_config),
                }),
                network_ref: Some(cfg.control_plane.bridge.clone()),
            })
            .await?;

        // The join script only needs the control plane's IP, which we
        // already know (it's the address we just requested for it) — no
        // need to read it back from the handle Proxmox returned.
        let control_plane_ip = cfg.control_plane.ip.to_string();

        // From here on, the control plane VM already exists — a worker
        // failing partway through must not leak it (or any workers already
        // created this attempt), or a retrying caller (the K8sCluster
        // operator always calls provision() fresh when it has no handle
        // yet) orphans one more control-plane-plus-N-workers batch per
        // failed attempt instead of retrying against nothing.
        let mut workers = Vec::with_capacity(cfg.workers.len());
        for (i, addr) in cfg.workers.iter().enumerate() {
            let worker_name = format!("{}-w{i}", ctx.resource_name);
            let worker_network_config = cloud_init::render_network_config(
                addr.ip,
                addr.prefix_len,
                addr.gateway,
                &addr.dns,
            );
            let worker_script = cloud_init::render_worker_script(&cloud_init::WorkerScriptInput {
                node_name: &worker_name,
                k3s_version: &cfg.k3s_version,
                cluster_token: &cfg.cluster_token,
                control_plane_ip: &control_plane_ip,
                debug_ssh_public_key: cfg.debug_ssh_public_key.as_deref(),
            });
            let worker_result = ctx
                .infra
                .create_vm(VmSpec {
                    name: worker_name.clone(),
                    cpu: cfg.worker_cpu,
                    memory_mib: (cfg.worker_memory_gib as u64) * 1024,
                    disk_gib: cfg.worker_disk_gib as u64,
                    image: cfg.image.clone(),
                    ip: Some(addr.ip),
                    cloud_init: Some(CloudInitConfig {
                        hostname: worker_name,
                        user_data: Some(worker_script),
                        network_config: Some(worker_network_config),
                    }),
                    network_ref: Some(addr.bridge.clone()),
                })
                .await;
            match worker_result {
                Ok(worker) => workers.push(worker),
                Err(e) => {
                    let _ = ctx.infra.delete_vm(&control_plane).await;
                    for w in &workers {
                        let _ = ctx.infra.delete_vm(w).await;
                    }
                    return Err(e.into());
                }
            }
        }

        let handle = K8sClusterHandle {
            control_plane,
            workers,
            cluster_token: cfg.cluster_token,
            bootstrap_secret: cfg.bootstrap_secret,
            kubeconfig: None,
            provisioned_at: Utc::now(),
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
        let h = deserialize_handle(handle)?;
        for worker in &h.workers {
            ctx.infra.delete_vm(worker).await?;
        }
        ctx.infra.delete_vm(&h.control_plane).await?;
        Ok(())
    }

    async fn reconcile(
        &self,
        _ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError> {
        let h = deserialize_handle(handle)?;
        if h.kubeconfig.is_some() {
            return Ok(ResourcePhase::Ready);
        }
        let elapsed = Utc::now().signed_duration_since(h.provisioned_at);
        if elapsed.num_minutes() > BOOTSTRAP_TIMEOUT_MINUTES {
            return Ok(ResourcePhase::Failed(format!(
                "bootstrap callback never arrived within {BOOTSTRAP_TIMEOUT_MINUTES} minutes"
            )));
        }
        Ok(ResourcePhase::Bootstrapping)
    }

    async fn endpoints(&self, handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError> {
        let h = deserialize_handle(handle)?;
        Ok(match h.control_plane.ip {
            Some(ip) => vec![Endpoint {
                name: "kube-apiserver".to_string(),
                url: format!("https://{ip}:6443"),
                description: Some("Kubernetes API server".to_string()),
            }],
            None => vec![],
        })
    }

    async fn credentials(&self, handle: &ResourceHandle) -> Result<Value, DriverError> {
        let h = deserialize_handle(handle)?;
        Ok(match h.kubeconfig {
            Some(kubeconfig) => serde_json::json!({ "kubeconfig": kubeconfig }),
            None => serde_json::json!({}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crow_core::{
        traits::InfraProvider,
        types::{NetworkHandle, NetworkSpec, VmHandle, VmStatus, VolumeHandle, VolumeSpec},
        ProviderError,
    };
    use std::sync::{atomic::AtomicU32, atomic::Ordering, Arc, Mutex};

    struct MockInfraProvider {
        created: Mutex<Vec<VmSpec>>,
        next_id: AtomicU32,
    }

    impl MockInfraProvider {
        fn new() -> Self {
            Self {
                created: Mutex::new(Vec::new()),
                next_id: AtomicU32::new(1),
            }
        }
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
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            let handle = VmHandle {
                provider_type: "mock".into(),
                provider_id: id.to_string(),
                ip: spec.ip,
                name: spec.name.clone(),
            };
            self.created.lock().unwrap().push(spec);
            Ok(handle)
        }
        async fn delete_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn vm_status(&self, _handle: &VmHandle) -> Result<VmStatus, ProviderError> {
            unimplemented!()
        }
        async fn start_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
            unimplemented!()
        }
        async fn stop_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
            unimplemented!()
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

    fn ctx_with(infra: Arc<dyn InfraProvider>, config: Value) -> ProvisionCtx {
        ProvisionCtx {
            infra,
            network: None,
            dns: None,
            config,
            project: "proj".to_string(),
            resource_name: "my-cluster".to_string(),
        }
    }

    fn valid_config() -> Value {
        serde_json::json!({
            "image": "9000",
            "k3s_version": "v1.29.4+k3s1",
            "control_plane": {
                "ip": "10.20.0.10",
                "prefix_len": 24,
                "gateway": "10.20.0.1",
                "dns": ["1.1.1.1"],
                "bridge": "vmbr0"
            },
            "control_plane_cpu": 2,
            "control_plane_memory_gib": 4,
            "control_plane_disk_gib": 40,
            "workers": [
                {
                    "ip": "10.20.0.11",
                    "prefix_len": 24,
                    "gateway": "10.20.0.1",
                    "dns": ["1.1.1.1"],
                    "bridge": "vmbr0"
                },
                {
                    "ip": "10.20.0.12",
                    "prefix_len": 24,
                    "gateway": "10.20.0.1",
                    "dns": ["1.1.1.1"],
                    "bridge": "vmbr0"
                }
            ],
            "worker_cpu": 2,
            "worker_memory_gib": 4,
            "worker_disk_gib": 40,
            "pod_cidr": "10.42.0.0/16",
            "service_cidr": "10.43.0.0/16",
            "lb_pool_cidr": "10.20.0.200/29",
            "monitoring": false,
            "callback_url": "https://crow-api.local/api/v1/internal/k8s-clusters/abc/report",
            "cluster_token": "tok-123",
            "bootstrap_secret": "secret-xyz"
        })
    }

    #[tokio::test]
    async fn provision_creates_one_control_plane_and_n_workers() {
        let infra = Arc::new(MockInfraProvider::new());
        let ctx = ctx_with(infra.clone(), valid_config());

        K8sClusterDriver.provision(&ctx).await.unwrap();

        let created = infra.created.lock().unwrap();
        assert_eq!(created.len(), 3); // 1 control plane + 2 workers
        assert_eq!(created[0].name, "my-cluster-cp");
        assert_eq!(created[0].ip, Some("10.20.0.10".parse().unwrap()));
        assert!(created[0]
            .cloud_init
            .as_ref()
            .unwrap()
            .network_config
            .is_some());
        assert_eq!(created[1].name, "my-cluster-w0");
        assert_eq!(created[1].ip, Some("10.20.0.11".parse().unwrap()));
        assert!(created[1]
            .cloud_init
            .as_ref()
            .unwrap()
            .network_config
            .is_some());
        assert_eq!(created[2].name, "my-cluster-w1");
        assert_eq!(created[2].ip, Some("10.20.0.12".parse().unwrap()));
    }

    #[tokio::test]
    async fn provision_embeds_the_control_plane_ip_in_worker_join_scripts() {
        let infra = Arc::new(MockInfraProvider::new());
        let ctx = ctx_with(infra.clone(), valid_config());

        K8sClusterDriver.provision(&ctx).await.unwrap();

        let created = infra.created.lock().unwrap();
        let worker_script = created[1]
            .cloud_init
            .as_ref()
            .unwrap()
            .user_data
            .as_ref()
            .unwrap();
        assert!(worker_script.contains("K3S_URL='https://10.20.0.10:6443'"));
    }

    #[tokio::test]
    async fn reconcile_is_bootstrapping_without_a_kubeconfig() {
        let infra = Arc::new(MockInfraProvider::new());
        let ctx = ctx_with(infra, Value::Null);

        let handle = K8sClusterHandle {
            control_plane: VmHandle {
                provider_type: "mock".into(),
                provider_id: "1".into(),
                ip: Some("10.20.0.10".parse().unwrap()),
                name: "my-cluster-cp".into(),
            },
            workers: vec![],
            cluster_token: "tok".into(),
            bootstrap_secret: "secret".into(),
            kubeconfig: None,
            provisioned_at: Utc::now(),
        };
        let resource_handle = ResourceHandle {
            resource_type: "K8sCluster".into(),
            data: serde_json::to_value(&handle).unwrap(),
        };

        let phase = K8sClusterDriver
            .reconcile(&ctx, &resource_handle)
            .await
            .unwrap();
        assert_eq!(phase, ResourcePhase::Bootstrapping);
    }

    #[tokio::test]
    async fn reconcile_is_ready_once_kubeconfig_is_set() {
        let infra = Arc::new(MockInfraProvider::new());
        let ctx = ctx_with(infra, Value::Null);

        let handle = K8sClusterHandle {
            control_plane: VmHandle {
                provider_type: "mock".into(),
                provider_id: "1".into(),
                ip: Some("10.20.0.10".parse().unwrap()),
                name: "my-cluster-cp".into(),
            },
            workers: vec![],
            cluster_token: "tok".into(),
            bootstrap_secret: "secret".into(),
            kubeconfig: Some("apiVersion: v1\n...".into()),
            provisioned_at: Utc::now(),
        };
        let resource_handle = ResourceHandle {
            resource_type: "K8sCluster".into(),
            data: serde_json::to_value(&handle).unwrap(),
        };

        let phase = K8sClusterDriver
            .reconcile(&ctx, &resource_handle)
            .await
            .unwrap();
        assert_eq!(phase, ResourcePhase::Ready);
    }

    #[tokio::test]
    async fn reconcile_fails_after_the_bootstrap_timeout() {
        let infra = Arc::new(MockInfraProvider::new());
        let ctx = ctx_with(infra, Value::Null);

        let handle = K8sClusterHandle {
            control_plane: VmHandle {
                provider_type: "mock".into(),
                provider_id: "1".into(),
                ip: Some("10.20.0.10".parse().unwrap()),
                name: "my-cluster-cp".into(),
            },
            workers: vec![],
            cluster_token: "tok".into(),
            bootstrap_secret: "secret".into(),
            kubeconfig: None,
            provisioned_at: Utc::now() - chrono::Duration::minutes(BOOTSTRAP_TIMEOUT_MINUTES + 1),
        };
        let resource_handle = ResourceHandle {
            resource_type: "K8sCluster".into(),
            data: serde_json::to_value(&handle).unwrap(),
        };

        let phase = K8sClusterDriver
            .reconcile(&ctx, &resource_handle)
            .await
            .unwrap();
        assert!(matches!(phase, ResourcePhase::Failed(_)));
    }

    #[tokio::test]
    async fn endpoints_reports_the_kube_apiserver_url() {
        let handle = ResourceHandle {
            resource_type: "K8sCluster".into(),
            data: serde_json::to_value(&K8sClusterHandle {
                control_plane: VmHandle {
                    provider_type: "mock".into(),
                    provider_id: "1".into(),
                    ip: Some("10.20.0.10".parse().unwrap()),
                    name: "my-cluster-cp".into(),
                },
                workers: vec![],
                cluster_token: "tok".into(),
                bootstrap_secret: "secret".into(),
                kubeconfig: None,
                provisioned_at: Utc::now(),
            })
            .unwrap(),
        };

        let endpoints = K8sClusterDriver.endpoints(&handle).await.unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].url, "https://10.20.0.10:6443");
    }
}
