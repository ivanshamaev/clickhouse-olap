use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub clickhouse: ClickHouseConfig,
    pub models: ModelsConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Bind address. Use "127.0.0.1:3000" for local mode, "0.0.0.0:3000" for server mode.
    #[serde(default = "default_bind")]
    pub bind: String,
}

fn default_bind() -> String {
    "127.0.0.1:3000".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickHouseConfig {
    pub url: String,
    #[serde(default = "default_database")]
    pub database: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Per-query timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub query_timeout_secs: u64,
    /// Max rows to accept from ClickHouse per sub-query (safety limit).
    #[serde(default = "default_max_preaggregate_rows")]
    pub max_preaggregate_rows: u64,
}

fn default_database() -> String {
    "default".to_string()
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_max_preaggregate_rows() -> u64 {
    500_000
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelsConfig {
    /// Glob pattern or directory for model TOML files.
    pub path: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct CacheConfig {
    /// Max cache entries.
    #[serde(default = "default_cache_capacity")]
    pub capacity: u64,
    /// TTL in seconds.
    #[serde(default = "default_cache_ttl")]
    pub ttl_secs: u64,
}

fn default_cache_capacity() -> u64 {
    1000
}

fn default_cache_ttl() -> u64 {
    300 // 5 minutes
}

#[derive(Debug, Deserialize, Clone)]
pub struct LimitsConfig {
    /// Default max cells (overridable per request).
    #[serde(default = "default_max_cells")]
    pub max_cells: u64,
    /// Default max groups (overridable per request).
    #[serde(default = "default_max_groups")]
    pub max_groups: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_cells: default_max_cells(),
            max_groups: default_max_groups(),
        }
    }
}

fn default_max_cells() -> u64 {
    500_000
}

fn default_max_groups() -> u64 {
    200_000
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file: {}", path.display()))?;
        toml::from_str(&text).with_context(|| "parsing config TOML")
    }
}
