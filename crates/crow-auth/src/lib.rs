pub mod jwt;
pub mod password;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub is_admin: bool,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectRole {
    Owner,
    Editor,
    Viewer,
}
