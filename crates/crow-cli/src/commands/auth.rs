use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct LoginArgs {
    #[arg(long, env = "CROW_SERVER")]
    pub server: String,
    #[arg(long)]
    pub username: String,
}

pub async fn login(_args: LoginArgs) -> Result<()> {
    todo!("login")
}
