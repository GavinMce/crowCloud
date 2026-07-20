use crow_core::ProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxmoxError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("task failed: {0}")]
    TaskFailed(String),
    #[error("task timed out after {0}s")]
    TaskTimeout(u64),
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("SSH error: {0}")]
    Ssh(String),
}

impl From<ProxmoxError> for ProviderError {
    fn from(e: ProxmoxError) -> Self {
        match e {
            ProxmoxError::Http(e) => ProviderError::Network(e.to_string()),
            ProxmoxError::Api {
                status: 401 | 403,
                message,
            } => ProviderError::Auth(message),
            ProxmoxError::Api {
                status: 404,
                message,
            } => ProviderError::NotFound(message),
            ProxmoxError::Api { status: _, message } => ProviderError::Api(message),
            e => ProviderError::Other(e.to_string()),
        }
    }
}
