use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal server error")]
    Internal(#[from] anyhow::Error),
}

impl From<crow_provider_registry::RegistryError> for ApiError {
    fn from(e: crow_provider_registry::RegistryError) -> Self {
        match e {
            crow_provider_registry::RegistryError::NotFound => ApiError::NotFound,
            other => ApiError::BadRequest(other.to_string()),
        }
    }
}

impl From<crow_core::ProviderError> for ApiError {
    fn from(e: crow_core::ProviderError) -> Self {
        match e {
            crow_core::ProviderError::NotFound(_) => ApiError::NotFound,
            crow_core::ProviderError::Auth(_) => ApiError::Forbidden,
            other => ApiError::Internal(anyhow::anyhow!(other.to_string())),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            ApiError::Internal(e) => {
                tracing::error!("internal error: {e:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".into(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[allow(dead_code)]
pub type ApiResult<T> = Result<T, ApiError>;
