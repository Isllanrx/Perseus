use std::sync::Arc;

use perseus_core::SoundCloudClient;
use perseus_core::shared::soundcloud::urls::is_valid_client_id;
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;
use vercel_runtime::axum::VercelLayer;

#[tokio::main]
async fn main() -> Result<(), vercel_runtime::Error> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_env("PERSEUS_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let client_id = std::env::var("SOUNDCLOUD_CLIENT_ID")
        .ok()
        .filter(|value| is_valid_client_id(value));
    let client = Arc::new(SoundCloudClient::new(client_id)?);
    let service = ServiceBuilder::new()
        .layer(VercelLayer::new())
        .service(perseus_web::app(client));
    vercel_runtime::run(service).await
}
