use axum::{
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
};
use crow_auth::{jwt, Claims};

use crate::{error::ApiError, AppState};

pub struct AuthUser(pub Claims);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(&parts.headers).ok_or(ApiError::Unauthorized)?;
        let claims = jwt::verify(token, &state.jwt_secret).map_err(|_| ApiError::Unauthorized)?;
        Ok(AuthUser(claims))
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let val = headers.get("Authorization")?.to_str().ok()?;
    val.strip_prefix("Bearer ")
}
