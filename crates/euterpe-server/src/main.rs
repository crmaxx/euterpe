use euterpe_server::{AppConfig, serve};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    euterpe_server::config::load_dotenv();
    let config = AppConfig::from_env()?;

    let (hawk, _hawk_guard) = euterpe_hawk::Hawk::install_from_env(Some(env!("CARGO_PKG_VERSION")));

    let default_filter = if config.debug {
        "euterpe_server=debug,euterpe_qobuz=debug,tower_http=debug"
    } else {
        "euterpe_server=info,tower_http=info"
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());
    let fmt_layer = tracing_subscriber::fmt::layer();
    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    if let Some(ref h) = hawk {
        registry
            .with(euterpe_hawk::HawkLayer::new(std::sync::Arc::clone(h)))
            .init();
    } else {
        registry.init();
    }

    serve(config, hawk).await
}
