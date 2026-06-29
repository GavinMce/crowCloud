use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use thiserror::Error;

use crate::Claims;

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("invalid token")]
    Invalid(#[from] jsonwebtoken::errors::Error),
    #[error("token expired")]
    Expired,
}

pub fn sign(claims: &Claims, secret: &[u8]) -> Result<String, JwtError> {
    Ok(encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret),
    )?)
}

pub fn verify(token: &str, secret: &[u8]) -> Result<Claims, JwtError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )?;
    let now = Utc::now().timestamp() as usize;
    if data.claims.exp < now {
        return Err(JwtError::Expired);
    }
    Ok(data.claims)
}

pub fn expiry_secs(hours: i64) -> usize {
    (Utc::now().timestamp() + hours * 3600) as usize
}
