use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::{client::CrowClient, config::Config};

#[derive(Args)]
pub struct LoginArgs {
    /// crowCloud server URL
    #[arg(long, env = "CROW_SERVER")]
    pub server: String,
    #[arg(long)]
    pub username: String,
    /// Password (if omitted, prompted securely)
    #[arg(long)]
    pub password: Option<String>,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

pub async fn login(args: LoginArgs) -> Result<()> {
    let password = match args.password {
        Some(p) => p,
        None => rpassword::prompt_password("Password: ")?,
    };

    let server = args.server.trim_end_matches('/').to_string();
    let client = CrowClient::new(server.clone(), None);

    let resp: LoginResponse = client
        .post(
            "/api/v1/auth/login",
            &LoginRequest {
                username: &args.username,
                password: &password,
            },
        )
        .await?;

    let mut cfg = Config::load()?;
    cfg.server = Some(server);
    cfg.token = Some(resp.token);
    cfg.save()?;

    println!("Logged in as {}. Token saved.", args.username);
    Ok(())
}
