use kube::Client;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub kube: Option<Client>,
    pub db: PgPool,
    pub jwt_secret: Vec<u8>,
}

impl AppState {
    pub async fn init() -> anyhow::Result<Self> {
        let kube = match Client::try_default().await {
            Ok(c) => {
                tracing::info!("connected to Kubernetes cluster");
                Some(c)
            }
            Err(e) => {
                tracing::warn!("no Kubernetes cluster available, operator features disabled: {e}");
                None
            }
        };

        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let db = crow_db::connect(&database_url).await?;
        crow_db::run_migrations(&db).await?;

        let jwt_secret = std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set")
            .into_bytes();

        Ok(Self {
            kube,
            db,
            jwt_secret,
        })
    }
}
