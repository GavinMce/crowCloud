use kube::Client;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub kube: Client,
    pub db: PgPool,
}

impl AppState {
    pub async fn init() -> anyhow::Result<Self> {
        let kube = Client::try_default().await?;

        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");
        let db = crow_db::connect(&database_url).await?;
        crow_db::run_migrations(&db).await?;

        Ok(Self { kube, db })
    }
}
