//! BillForge API Server Entry Point

use billforge_api::{routes, swagger_ui, AppState, Config};
use axum::http::{HeaderName, HeaderValue, Method};
use std::net::SocketAddr;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tower_http::compression::CompressionLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "billforge_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!(
        environment = ?config.environment,
        "Starting BillForge API server..."
    );

    // Initialize application state
    let state = AppState::new(&config).await?;
    tracing::info!("Application state initialized");

    // Build CORS layer with explicit allowed origins
    let cors = build_cors_layer(&config);

    // Build router with security middleware
    // Note: Timeout layer has compatibility issues with other layers
    // For request timeout, use tokio::time::timeout in handlers if needed
    let app = routes::create_router(state)
        // Add Swagger UI for API documentation
        .merge(swagger_ui())
        .layer(
            ServiceBuilder::new()
                // Limit request body size to 50MB (for file uploads)
                .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024))
                // Compress responses
                .layer(CompressionLayer::new())
                // Request tracing
                .layer(TraceLayer::new_for_http())
                // CORS
                .layer(cors),
        );

    tracing::info!("Swagger UI available at http://{}:{}/swagger-ui", config.host, config.port);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Build CORS layer with appropriate configuration based on environment
fn build_cors_layer(config: &Config) -> CorsLayer {
    let allowed_origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    tracing::info!(
        origins = ?config.allowed_origins,
        "CORS configured with allowed origins"
    );

    CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("x-request-id"),
            HeaderName::from_static("x-tenant-id"),
        ])
        .allow_credentials(true)
        .max_age(Duration::from_secs(3600))
}
