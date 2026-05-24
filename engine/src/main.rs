use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use engine_lib::api::handlers;
use engine_lib::clickhouse::ClickHouseClient;
use engine_lib::config::Config;
use engine_lib::semantic::ModelStore;
use engine_lib::state::AppState;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "engine", about = "ClickHouse OLAP pivot engine")]
struct Cli {
    #[arg(long, short, default_value = "engine/config/config.local.toml")]
    config: PathBuf,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Config
    let config = Config::load(&cli.config)
        .with_context(|| format!("loading config from {}", cli.config.display()))?;
    tracing::info!(bind = %config.server.bind, "starting engine");

    // Semantic models
    let models_dir = PathBuf::from(&config.models.path);
    let models = ModelStore::load_from_dir(&models_dir)
        .with_context(|| format!("loading models from {}", models_dir.display()))?;
    tracing::info!(count = models.list().len(), "semantic models loaded");

    // ClickHouse client
    let clickhouse = ClickHouseClient::new(&config.clickhouse);

    // App state
    let state = AppState::new(config.clone(), models, clickhouse);

    // Router
    let app = build_router(state);

    // Bind
    let addr: SocketAddr = config
        .server
        .bind
        .parse()
        .with_context(|| format!("parsing bind address: {}", config.server.bind))?;

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "listening");
    axum::serve(listener, app).await?;

    Ok(())
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(handlers::health))
        .route("/v1/models", get(handlers::list_models))
        .route("/v1/models/{id}", get(handlers::get_model))
        .route("/v1/query", post(handlers::query))
        .route("/v1/query/{id}/cancel", post(handlers::cancel_query))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
