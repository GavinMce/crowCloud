use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("API error: {0}")]
    Api(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("provision failed: {0}")]
    ProvisionFailed(String),
    #[error("deprovision failed: {0}")]
    DeprovisionFailed(String),
    #[error("reconcile error: {0}")]
    Reconcile(String),
    #[error("{0}")]
    Other(String),
}
