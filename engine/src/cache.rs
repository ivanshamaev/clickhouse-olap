//! In-memory result cache using moka.
//! Key = hash of (model_id, row fields, col fields, measures, filters, totals config).

use std::sync::Arc;
use std::time::Duration;

use crate::api::types::PivotRequest;
use crate::config::CacheConfig;
use moka::future::Cache;

pub type CachedResponse = Arc<serde_json::Value>;

#[derive(Clone)]
pub struct ResultCache {
    inner: Cache<String, CachedResponse>,
}

impl ResultCache {
    pub fn new(cfg: &CacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(cfg.capacity)
            .time_to_live(Duration::from_secs(cfg.ttl_secs))
            .build();
        Self { inner: cache }
    }

    pub async fn get(&self, key: &str) -> Option<CachedResponse> {
        self.inner.get(key).await
    }

    pub async fn insert(&self, key: String, value: CachedResponse) {
        self.inner.insert(key, value).await;
    }

    /// Compute a stable cache key from the request (excluding request_id and bypass_cache).
    pub fn cache_key(req: &PivotRequest) -> Option<String> {
        if req.limits.bypass_cache == Some(true) {
            return None;
        }

        // Build a canonical representation for hashing.
        // We use serde_json to serialize the stable parts of the request.
        let stable = serde_json::json!({
            "model_id": req.model_id,
            "rows": req.rows.iter().map(|f| serde_json::json!({
                "field": f.field,
                "date_granularity": f.date_granularity,
            })).collect::<Vec<_>>(),
            "columns": req.columns.iter().map(|f| serde_json::json!({
                "field": f.field,
                "date_granularity": f.date_granularity,
            })).collect::<Vec<_>>(),
            "measures": req.measures.iter().map(|m| serde_json::json!({
                "id": m.id,
                "agg": m.agg,
            })).collect::<Vec<_>>(),
            "filters": req.filters.iter().map(|f| serde_json::json!({
                "field": f.field,
                "op": format!("{:?}", f.op),
                "value": f.value,
            })).collect::<Vec<_>>(),
            "totals": {
                "row_subtotals": req.totals.row_subtotals,
                "column_subtotals": req.totals.column_subtotals,
                "grand_total": req.totals.grand_total,
            },
        });

        Some(format!("{:x}", md5_of(&stable.to_string())))
    }
}

fn md5_of(s: &str) -> u128 {
    // Simple non-cryptographic hash for cache keys.
    // Using FNV-like 128-bit hash to avoid adding a dependency.
    let mut h: u128 = 0x6c62272e07bb0142_u128;
    for b in s.bytes() {
        h ^= b as u128;
        h = h.wrapping_mul(0x0000013b_00000000_0000013b_00000001);
    }
    h
}
