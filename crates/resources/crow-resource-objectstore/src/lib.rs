use async_trait::async_trait;
use crow_core::{
    traits::{ProvisionCtx, ResourceDriver},
    types::{Endpoint, ResourceHandle, ResourcePhase},
    DriverError,
};
use serde_json::Value;

pub struct ObjectStoreDriver;

#[async_trait]
impl ResourceDriver for ObjectStoreDriver {
    fn resource_type(&self) -> &'static str {
        "ObjectStore"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({}) // TODO: JSON Schema for UI form generation
    }

    async fn provision(&self, _ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError> {
        todo!("ObjectStore provision")
    }
    async fn deprovision(
        &self,
        _ctx: &ProvisionCtx,
        _handle: &ResourceHandle,
    ) -> Result<(), DriverError> {
        todo!("ObjectStore deprovision")
    }
    async fn reconcile(
        &self,
        _ctx: &ProvisionCtx,
        _handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError> {
        todo!("ObjectStore reconcile")
    }
    async fn endpoints(&self, _handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError> {
        todo!("ObjectStore endpoints")
    }
    async fn credentials(&self, _handle: &ResourceHandle) -> Result<Value, DriverError> {
        todo!("ObjectStore credentials")
    }
}
