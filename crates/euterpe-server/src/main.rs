use euterpe_server::{serve, AppConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    euterpe_server::config::load_dotenv();
    let config = AppConfig::from_env()?;

    let default_filter = if config.dev_verbose {
        "euterpe_server=debug,euterpe_qobuz=debug,tower_http=debug"
    } else {
        "euterpe_server=info,tower_http=info"
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .init();
    serve(config).await
}
