use crow_core::ProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OPNsenseError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("no free IP available in the configured range")]
    NoFreeIp,
    #[error("unexpected response: {0}")]
    Parse(String),
}

impl From<OPNsenseError> for ProviderError {
    fn from(e: OPNsenseError) -> Self {
        match e {
            OPNsenseError::Http(e) => ProviderError::Network(e.to_string()),
            OPNsenseError::Api {
                status: 401 | 403,
                message,
            } => ProviderError::Auth(message),
            OPNsenseError::Api { status: _, message } => ProviderError::Api(message),
            e => ProviderError::Other(e.to_string()),
        }
    }
}
