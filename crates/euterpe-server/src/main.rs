use euterpe_server::{serve, AppConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "euterpe_server=info,tower_http=info".into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    serve(config).await
}
