use async_trait::async_trait;
use crow_core::{
    traits::{ProvisionCtx, ResourceDriver},
    types::{Endpoint, ResourceHandle, ResourcePhase},
    DriverError,
};
use serde_json::Value;

pub struct DatabaseDriver;

#[async_trait]
impl ResourceDriver for DatabaseDriver {
    fn resource_type(&self) -> &'static str {
        "Database"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({}) // TODO: JSON Schema for UI form generation
    }

    async fn provision(&self, _ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError> {
        todo!("Database provision")
    }
    async fn deprovision(
        &self,
        _ctx: &ProvisionCtx,
        _handle: &ResourceHandle,
    ) -> Result<(), DriverError> {
        todo!("Database deprovision")
    }
    async fn reconcile(
        &self,
        _ctx: &ProvisionCtx,
        _handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError> {
        todo!("Database reconcile")
    }
    async fn endpoints(&self, _handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError> {
        todo!("Database endpoints")
    }
    async fn credentials(&self, _handle: &ResourceHandle) -> Result<Value, DriverError> {
        todo!("Database credentials")
    }
}
