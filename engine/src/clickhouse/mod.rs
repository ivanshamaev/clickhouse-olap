//! ClickHouse HTTP client.
//!
//! Uses the ClickHouse HTTP interface with JSONEachRow format.
//! Query parameters use ClickHouse's native parameter syntax: {pN:Type} in SQL,
//! param_pN=value in URL query string (FR-QUERY-03).

use std::time::{Duration, Instant};

use reqwest::Client;
use serde_json::Value as JsonValue;

use crate::config::ClickHouseConfig;
use crate::error::EngineError;

#[derive(Debug, Clone)]
pub struct ClickHouseClient {
    client: Client,
    base_url: String,
    database: String,
    username: Option<String>,
    password: Option<String>,
    _timeout: Duration,
    pub max_preaggregate_rows: u64,
}

impl ClickHouseClient {
    pub fn new(cfg: &ClickHouseConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(cfg.query_timeout_secs))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            base_url: cfg.url.trim_end_matches('/').to_string(),
            database: cfg.database.clone(),
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            _timeout: Duration::from_secs(cfg.query_timeout_secs),
            max_preaggregate_rows: cfg.max_preaggregate_rows,
        }
    }

    /// Execute a SQL query and return rows as a list of JSON objects.
    /// `url_params` are ClickHouse query parameters (already prefixed with `param_`).
    pub async fn query(
        &self,
        sql: &str,
        url_params: &[(String, String)],
    ) -> Result<QueryResult, EngineError> {
        let start = Instant::now();

        let mut request = self
            .client
            .post(&self.base_url)
            .query(&[("database", &self.database)])
            .query(url_params)
            .body(sql.to_string())
            .header("Content-Type", "text/plain; charset=utf-8");

        if let Some(user) = &self.username {
            request = request.query(&[("user", user)]);
        }
        if let Some(pass) = &self.password {
            request = request.query(&[("password", pass)]);
        }

        let response = request.send().await.map_err(|e| {
            tracing::error!(error = %e, "ClickHouse request failed");
            EngineError::ClickHouseUnavailable(e.to_string())
        })?;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| EngineError::ClickHouseUnavailable(format!("reading response: {e}")))?;

        if !status.is_success() {
            tracing::error!(
                http_status = %status,
                body = %body,
                sql = %sql,
                "ClickHouse returned error"
            );
            // ClickHouse error codes appear in the body
            if body.contains("TIMEOUT") || body.contains("TIMED_OUT") {
                return Err(EngineError::QueryTimeout);
            }
            return Err(EngineError::ClickHouseUnavailable(format!(
                "HTTP {status}: {body}"
            )));
        }

        // Parse JSONEachRow: each non-empty line is a JSON object
        let rows: Vec<JsonValue> = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap_or(JsonValue::Null))
            .filter(|v| v != &JsonValue::Null)
            .collect();

        let row_count = rows.len() as u64;

        // Safety limit: reject oversized pre-aggregates (FR-ENGINE-07)
        if row_count > self.max_preaggregate_rows {
            return Err(EngineError::PreaggregateToLarge(format!(
                "ClickHouse returned {row_count} rows, limit is {}",
                self.max_preaggregate_rows
            )));
        }

        tracing::debug!(
            rows = row_count,
            elapsed_ms = elapsed_ms,
            "ClickHouse query complete"
        );

        Ok(QueryResult { rows, elapsed_ms })
    }

    /// Health check: runs `SELECT 1`.
    pub async fn health(&self) -> Result<(), EngineError> {
        self.query("SELECT 1 FORMAT JSONEachRow", &[]).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct QueryResult {
    pub rows: Vec<JsonValue>,
    pub elapsed_ms: u64,
}
