use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use chrono::Utc;
use crow_auth::{
    jwt::{self, expiry_secs},
    password, Claims,
};
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    password_hash: String,
    is_admin: bool,
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, password_hash, is_admin FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::Unauthorized)?;

    password::verify(&req.password, &row.password_hash).map_err(|_| ApiError::Unauthorized)?;

    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: row.id.to_string(),
        username: req.username,
        is_admin: row.is_admin,
        exp: expiry_secs(24),
        iat: now,
    };
    let token = jwt::sign(&claims, &state.jwt_secret)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e.to_string())))?;

    Ok(Json(LoginResponse { token }))
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct RegisterResponse {
    id: Uuid,
    username: String,
    email: String,
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<RegisterResponse>)> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if count > 0 {
        return Err(ApiError::Forbidden);
    }

    let hash = password::hash(&req.password).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let row = sqlx::query_as::<_, RegisterResponse>(
        "INSERT INTO users (username, email, password_hash, is_admin)
         VALUES ($1, $2, $3, true)
         RETURNING id, username, email",
    )
    .bind(&req.username)
    .bind(&req.email)
    .bind(&hash)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db)
            if db.constraint() == Some("users_username_key")
                || db.constraint() == Some("users_email_key") =>
        {
            ApiError::Conflict("username or email already exists".into())
        }
        _ => ApiError::Internal(e.into()),
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}
