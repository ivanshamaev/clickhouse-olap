//! axum HTTP handlers for all API endpoints.

use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value as JsonValue};

use crate::api::types::PivotRequest;
use crate::error::EngineError;
use crate::pivot::{shape, SubQueryRows};
use crate::query;
use crate::query::sql::build_sql;
use crate::state::AppState;

// ── GET /v1/health ────────────────────────────────────────────────────────────

pub async fn health(State(state): State<AppState>) -> Result<impl IntoResponse, EngineError> {
    state.clickhouse.health().await?;
    Ok(Json(json!({ "status": "ok" })))
}

// ── GET /v1/models ────────────────────────────────────────────────────────────

pub async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    let summaries = state.models.list();
    Json(summaries)
}

// ── GET /v1/models/{id} ───────────────────────────────────────────────────────

pub async fn get_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<impl IntoResponse, EngineError> {
    let model = state
        .models
        .get(&model_id)
        .ok_or_else(|| EngineError::ModelNotFound(model_id.clone()))?;
    Ok(Json(model.to_metadata()))
}

// ── POST /v1/query ────────────────────────────────────────────────────────────

pub async fn query(
    State(state): State<AppState>,
    Json(req): Json<PivotRequest>,
) -> Result<impl IntoResponse, EngineError> {
    let engine_start = Instant::now();
    let request_id = req.request_id.clone();

    tracing::info!(
        request_id = %request_id,
        model_id = %req.model_id,
        rows = req.rows.len(),
        cols = req.columns.len(),
        measures = req.measures.len(),
        "pivot query received"
    );

    // ── Check cache ───────────────────────────────────────────────────────────
    let cache_key = crate::cache::ResultCache::cache_key(&req);
    if let Some(ref key) = cache_key {
        if let Some(cached) = state.cache.get(key).await {
            tracing::debug!(request_id = %request_id, "cache hit");
            let mut resp: JsonValue = (*cached).clone();
            // Patch request_id and from_cache in the cached response
            if let Some(obj) = resp.as_object_mut() {
                obj.insert("request_id".to_string(), json!(request_id));
                obj.insert("from_cache".to_string(), json!(true));
            }
            return Ok(Json(resp));
        }
    }

    // ── Get model ─────────────────────────────────────────────────────────────
    let model = state
        .models
        .get(&req.model_id)
        .ok_or_else(|| EngineError::ModelNotFound(req.model_id.clone()))?;

    // ── Plan ──────────────────────────────────────────────────────────────────
    let max_groups = req
        .limits
        .max_groups
        .unwrap_or(state.config.limits.max_groups);
    let plan = query::plan(&req, model, max_groups)?;

    tracing::debug!(
        request_id = %request_id,
        sub_queries = plan.sub_queries.len(),
        "query plan built"
    );

    // ── Execute sub-queries ───────────────────────────────────────────────────
    let mut sub_results: Vec<SubQueryRows> = Vec::with_capacity(plan.sub_queries.len());

    for sub in &plan.sub_queries {
        let (sql, url_params) = build_sql(&plan, &sub.kind);

        tracing::debug!(
            request_id = %request_id,
            kind = ?sub.kind,
            sql = %sql,
            "executing sub-query"
        );

        let result = state.clickhouse.query(&sql, &url_params).await?;

        // Check overflow (we asked for max_groups+1 rows; if we got that many, reject)
        if result.rows.len() as u64 > max_groups {
            return Err(EngineError::PreaggregateToLarge(format!(
                "sub-query returned >{max_groups} groups"
            )));
        }

        sub_results.push(SubQueryRows {
            kind: sub.kind.clone(),
            rows: result.rows,
            elapsed_ms: result.elapsed_ms,
        });
    }

    // ── Shape pivot ───────────────────────────────────────────────────────────
    let engine_ms = engine_start.elapsed().as_millis() as u64;
    let response = shape(&request_id, &plan, sub_results, engine_ms);

    // ── Check cell limit ──────────────────────────────────────────────────────
    let max_cells = req
        .limits
        .max_cells
        .unwrap_or(state.config.limits.max_cells);
    let cell_count = response.cells.len() as u64;
    if cell_count > max_cells {
        return Err(EngineError::ResultTooLarge(format!(
            "{cell_count} cells exceed limit of {max_cells}"
        )));
    }

    // ── Store in cache ────────────────────────────────────────────────────────
    let response_json = serde_json::to_value(&response)
        .map_err(|e| EngineError::Internal(format!("serializing response: {e}")))?;

    if let Some(key) = cache_key {
        state
            .cache
            .insert(key, std::sync::Arc::new(response_json.clone()))
            .await;
    }

    tracing::info!(
        request_id = %request_id,
        rows = response.row_axis.members.len(),
        cols = response.column_axis.members.len(),
        cells = response.cells.len(),
        engine_ms = response.stats.engine_ms,
        clickhouse_ms = response.stats.clickhouse_ms,
        "pivot query complete"
    );

    Ok(Json(response_json))
}

// ── POST /v1/query/{id}/cancel ────────────────────────────────────────────────

pub async fn cancel_query(
    State(_state): State<AppState>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    // Best-effort cancellation. In-flight HTTP requests to ClickHouse are dropped
    // when the tokio task is aborted. Full cancellation token support is a TODO.
    tracing::info!(request_id = %request_id, "cancel requested (best-effort)");
    StatusCode::OK
}
