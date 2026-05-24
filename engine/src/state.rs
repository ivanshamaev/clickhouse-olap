//! Shared application state passed to axum handlers.

use std::sync::Arc;

use crate::cache::ResultCache;
use crate::clickhouse::ClickHouseClient;
use crate::config::Config;
use crate::semantic::ModelStore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub models: Arc<ModelStore>,
    pub clickhouse: Arc<ClickHouseClient>,
    pub cache: Arc<ResultCache>,
}

impl AppState {
    pub fn new(config: Config, models: ModelStore, clickhouse: ClickHouseClient) -> Self {
        let cache = ResultCache::new(&config.cache);
        Self {
            config: Arc::new(config),
            models: Arc::new(models),
            clickhouse: Arc::new(clickhouse),
            cache: Arc::new(cache),
        }
    }
}
