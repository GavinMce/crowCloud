use async_trait::async_trait;
use crow_core::{
    traits::{ProvisionCtx, ResourceDriver},
    types::{CloudInitConfig, Endpoint, ResourceHandle, ResourcePhase, VmHandle, VmSpec, VmStatus},
    DriverError,
};
use serde::Deserialize;
use serde_json::Value;

pub struct VirtualMachineDriver;

/// Shape `ProvisionCtx.config` is expected to hold for a VirtualMachine resource.
#[derive(Debug, Deserialize)]
struct VmProvisionConfig {
    cpu: u32,
    memory_mib: u64,
    disk_gib: u64,
    image: String,
    hostname: Option<String>,
    user_data: Option<String>,
}

fn deserialize_vm_handle(handle: &ResourceHandle) -> Result<VmHandle, DriverError> {
    serde_json::from_value(handle.data.clone())
        .map_err(|e| DriverError::Other(format!("corrupt VM resource handle: {e}")))
}

#[async_trait]
impl ResourceDriver for VirtualMachineDriver {
    fn resource_type(&self) -> &'static str {
        "VirtualMachine"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["cpu", "memory_mib", "disk_gib", "image"],
            "properties": {
                "cpu": { "type": "integer", "minimum": 1 },
                "memory_mib": { "type": "integer", "minimum": 1 },
                "disk_gib": { "type": "integer", "minimum": 1 },
                "image": { "type": "string" },
                "hostname": { "type": "string" },
                "user_data": { "type": "string" }
            }
        })
    }

    async fn provision(&self, ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError> {
        let cfg: VmProvisionConfig = serde_json::from_value(ctx.config.clone())
            .map_err(|e| DriverError::InvalidConfig(format!("invalid VM config: {e}")))?;

        let cloud_init = cfg.hostname.as_ref().map(|hostname| CloudInitConfig {
            hostname: hostname.clone(),
            user_data: cfg.user_data.clone(),
            network_config: None,
        });

        let spec = VmSpec {
            name: ctx.resource_name.clone(),
            cpu: cfg.cpu,
            memory_mib: cfg.memory_mib,
            disk_gib: cfg.disk_gib,
            image: cfg.image,
            ip: None,
            cloud_init,
            network_ref: None,
        };

        let vm_handle = ctx.infra.create_vm(spec).await?;

        Ok(ResourceHandle {
            resource_type: self.resource_type().to_string(),
            data: serde_json::to_value(&vm_handle)
                .map_err(|e| DriverError::Other(format!("failed to encode VM handle: {e}")))?,
        })
    }

    async fn deprovision(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<(), DriverError> {
        let vm_handle = deserialize_vm_handle(handle)?;
        ctx.infra.delete_vm(&vm_handle).await?;
        Ok(())
    }

    async fn reconcile(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError> {
        let vm_handle = deserialize_vm_handle(handle)?;
        let status = ctx.infra.vm_status(&vm_handle).await?;

        Ok(match status {
            VmStatus::Running => ResourcePhase::Ready,
            VmStatus::Starting | VmStatus::Stopping => ResourcePhase::ProvisioningInfra,
            VmStatus::Stopped => ResourcePhase::Degraded("vm is stopped".to_string()),
            VmStatus::Error(msg) => ResourcePhase::Failed(msg),
            VmStatus::Unknown => ResourcePhase::Degraded("vm status unknown".to_string()),
        })
    }

    async fn endpoints(&self, handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError> {
        let vm_handle = deserialize_vm_handle(handle)?;

        Ok(match vm_handle.ip {
            Some(ip) => vec![Endpoint {
                name: "ssh".to_string(),
                url: format!("ssh://{ip}"),
                description: Some("SSH access".to_string()),
            }],
            None => vec![],
        })
    }

    async fn credentials(&self, _handle: &ResourceHandle) -> Result<Value, DriverError> {
        // v1: bare VMs have no generated credential material (no cloud-init secret
        // management yet) — "no credentials" is a valid state, not an error.
        Ok(serde_json::json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crow_core::{
        traits::InfraProvider,
        types::{NetworkHandle, NetworkSpec, VolumeHandle, VolumeSpec},
        ProviderError,
    };
    use std::{net::IpAddr, sync::Arc};

    struct MockInfraProvider {
        vm_handle: VmHandle,
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
        async fn create_vm(&self, _spec: VmSpec) -> Result<VmHandle, ProviderError> {
            Ok(self.vm_handle.clone())
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
            project: "proj".into(),
            resource_group: "rg".into(),
            resource_name: "my-vm".into(),
        }
    }

    #[tokio::test]
    async fn provision_maps_config_and_wraps_handle() {
        let vm_handle = VmHandle {
            provider_type: "mock".into(),
            provider_id: "123".into(),
            ip: Some("10.0.0.5".parse::<IpAddr>().unwrap()),
            name: "my-vm".into(),
        };
        let infra = Arc::new(MockInfraProvider {
            vm_handle: vm_handle.clone(),
            vm_status: VmStatus::Running,
        });
        let ctx = ctx_with(
            infra,
            serde_json::json!({ "cpu": 2, "memory_mib": 2048, "disk_gib": 20, "image": "9000" }),
        );

        let handle = VirtualMachineDriver.provision(&ctx).await.unwrap();
        assert_eq!(handle.resource_type, "VirtualMachine");
        let decoded: VmHandle = serde_json::from_value(handle.data).unwrap();
        assert_eq!(decoded.provider_id, "123");
        assert_eq!(decoded.ip, vm_handle.ip);
    }

    #[tokio::test]
    async fn reconcile_maps_vm_status_to_resource_phase() {
        let vm_handle = VmHandle {
            provider_type: "mock".into(),
            provider_id: "123".into(),
            ip: None,
            name: "my-vm".into(),
        };
        let resource_handle = ResourceHandle {
            resource_type: "VirtualMachine".into(),
            data: serde_json::to_value(&vm_handle).unwrap(),
        };

        for (status, expected) in [
            (VmStatus::Running, ResourcePhase::Ready),
            (
                VmStatus::Stopped,
                ResourcePhase::Degraded("vm is stopped".into()),
            ),
            (
                VmStatus::Error("boom".into()),
                ResourcePhase::Failed("boom".into()),
            ),
        ] {
            let infra = Arc::new(MockInfraProvider {
                vm_handle: vm_handle.clone(),
                vm_status: status,
            });
            let ctx = ctx_with(infra, Value::Null);
            let phase = VirtualMachineDriver
                .reconcile(&ctx, &resource_handle)
                .await
                .unwrap();
            assert_eq!(phase, expected);
        }
    }

    #[tokio::test]
    async fn endpoints_empty_when_no_ip_known() {
        let vm_handle = VmHandle {
            provider_type: "mock".into(),
            provider_id: "123".into(),
            ip: None,
            name: "my-vm".into(),
        };
        let resource_handle = ResourceHandle {
            resource_type: "VirtualMachine".into(),
            data: serde_json::to_value(&vm_handle).unwrap(),
        };
        let infra = Arc::new(MockInfraProvider {
            vm_handle: vm_handle.clone(),
            vm_status: VmStatus::Running,
        });
        let ctx = ctx_with(infra, Value::Null);

        let endpoints = VirtualMachineDriver
            .endpoints(&resource_handle)
            .await
            .unwrap();
        assert!(endpoints.is_empty());
        let _ = ctx; // endpoints() only takes the handle, ctx unused here
    }

    #[tokio::test]
    async fn endpoints_has_ssh_entry_when_ip_known() {
        let vm_handle = VmHandle {
            provider_type: "mock".into(),
            provider_id: "123".into(),
            ip: Some("10.0.0.5".parse::<IpAddr>().unwrap()),
            name: "my-vm".into(),
        };
        let resource_handle = ResourceHandle {
            resource_type: "VirtualMachine".into(),
            data: serde_json::to_value(&vm_handle).unwrap(),
        };

        let endpoints = VirtualMachineDriver
            .endpoints(&resource_handle)
            .await
            .unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].url, "ssh://10.0.0.5");
    }
}
