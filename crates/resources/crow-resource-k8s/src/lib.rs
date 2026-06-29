use async_trait::async_trait;
use crow_core::{
    traits::{ProvisionCtx, ResourceDriver},
    types::{Endpoint, ResourceHandle, ResourcePhase},
    DriverError,
};
use serde_json::Value;

pub struct K8sClusterDriver;

#[async_trait]
impl ResourceDriver for K8sClusterDriver {
    fn resource_type(&self) -> &'static str {
        "K8sCluster"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({}) // TODO: JSON Schema for UI form generation
    }

    async fn provision(&self, _ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError> {
        todo!("K8sCluster provision")
    }
    async fn deprovision(
        &self,
        _ctx: &ProvisionCtx,
        _handle: &ResourceHandle,
    ) -> Result<(), DriverError> {
        todo!("K8sCluster deprovision")
    }
    async fn reconcile(
        &self,
        _ctx: &ProvisionCtx,
        _handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError> {
        todo!("K8sCluster reconcile")
    }
    async fn endpoints(&self, _handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError> {
        todo!("K8sCluster endpoints")
    }
    async fn credentials(&self, _handle: &ResourceHandle) -> Result<Value, DriverError> {
        todo!("K8sCluster credentials")
    }
}
