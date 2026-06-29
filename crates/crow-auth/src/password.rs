use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("hash error: {0}")]
    Hash(String),
    #[error("invalid password")]
    Invalid,
}

pub fn hash(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Hash(e.to_string()))
}

pub fn verify(password: &str, hash: &str) -> Result<(), PasswordError> {
    let parsed = PasswordHash::new(hash).map_err(|e| PasswordError::Hash(e.to_string()))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| PasswordError::Invalid)
}
