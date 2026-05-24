//! Query planning: validate request → build SQL plan → emit parameterized SQL.
//!
//! Invariants enforced here:
//!   • Only model fields may appear in GROUP BY / filters (FR-QUERY-04).
//!   • All filter values are ClickHouse HTTP parameters (FR-QUERY-03).
//!   • Cardinality guard fires before any SQL is executed (FR-QUERY-05).

use crate::api::types::{AggType, FilterOp, PivotRequest};
use crate::error::EngineError;
use crate::semantic::SemanticModel;

pub use plan::{QueryPlan, SubQuery, SubQueryKind};
pub use sql::build_sql;

pub mod plan;
pub mod sql;

/// Validate the request against the semantic model and produce a `QueryPlan`.
pub fn plan(
    req: &PivotRequest,
    model: &SemanticModel,
    max_groups: u64,
) -> Result<QueryPlan, EngineError> {
    // ── 1. Validate dimension fields ──────────────────────────────────────────
    for f in req.rows.iter().chain(req.columns.iter()) {
        if !model.validate_dimension_field(&f.field) {
            return Err(EngineError::FieldNotAllowed(f.field.clone()));
        }
        // Validate date granularity is only on date dimensions
        if f.date_granularity.is_some() {
            let dim = model.dimension(&f.field).unwrap();
            if dim.dim_type != "date" {
                return Err(EngineError::ValidationFailed(format!(
                    "date_granularity is only allowed on date dimensions, but '{}' has type '{}'",
                    f.field, dim.dim_type
                )));
            }
        }
    }

    // ── 2. Validate measures ──────────────────────────────────────────────────
    for m in &req.measures {
        // Skip calculated measures for now (validated separately)
        if m.measure_type.as_deref() == Some("calculated") {
            continue;
        }
        if model.measure(&m.id).is_none() {
            return Err(EngineError::FieldNotAllowed(format!("measure '{}'", m.id)));
        }
        if let Some(agg) = &m.agg {
            if !model.validate_measure_agg(&m.id, agg) {
                let meta = model.measure(&m.id).unwrap();
                return Err(EngineError::ValidationFailed(format!(
                    "aggregation '{:?}' is not allowed for measure '{}' (allowed: {})",
                    agg,
                    m.id,
                    meta.allowed_agg.join(", ")
                )));
            }
        }
    }

    // ── 3. Validate filter fields ─────────────────────────────────────────────
    for f in &req.filters {
        if !model.validate_filter_field(&f.field) {
            return Err(EngineError::FieldNotAllowed(format!(
                "filter field '{}'",
                f.field
            )));
        }
    }

    // ── 4. Cardinality guard (FR-QUERY-05, R2) ────────────────────────────────
    for f in req.rows.iter().chain(req.columns.iter()) {
        if model.guards().high_cardinality_fields.contains(&f.field) {
            // High-cardinality field in GROUP BY — check if bounded by a filter or top_n
            let has_top_n = f.top_n.is_some();
            let has_eq_filter = req
                .filters
                .iter()
                .any(|fl| fl.field == f.field && matches!(fl.op, FilterOp::Eq | FilterOp::In));
            if !has_top_n && !has_eq_filter {
                return Err(EngineError::PreaggregateToLarge(format!(
                    "field '{}' is high-cardinality; add a filter or top_n limit",
                    f.field
                )));
            }
        }
    }

    // Rough group count estimation: if explicit max_groups requested, check it
    // (real cardinality check happens at ClickHouse execution time via LIMIT)
    let effective_max = req.limits.max_groups.unwrap_or(max_groups);

    // ── 5. Build resolved measures ────────────────────────────────────────────
    let resolved_measures: Vec<ResolvedMeasure> = req
        .measures
        .iter()
        .filter(|m| m.measure_type.as_deref() != Some("calculated"))
        .map(|m| {
            let meta = model.measure(&m.id).unwrap();
            let agg = m.agg.unwrap_or_else(|| {
                // Default to first allowed_agg
                parse_agg(
                    meta.allowed_agg
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("sum"),
                )
            });
            ResolvedMeasure {
                id: m.id.clone(),
                field: meta.field.clone(),
                agg,
                additive: meta.additive,
                format: meta.format.clone(),
            }
        })
        .collect();

    // ── 6. Build QueryPlan ────────────────────────────────────────────────────
    let plan = plan::build_plan(req, model, &resolved_measures, effective_max);

    Ok(plan)
}

#[derive(Debug, Clone)]
pub struct ResolvedMeasure {
    pub id: String,
    pub field: String,
    pub agg: AggType,
    pub additive: bool,
    pub format: Option<String>,
}

fn parse_agg(s: &str) -> AggType {
    match s {
        "sum" => AggType::Sum,
        "count" => AggType::Count,
        "count_distinct" => AggType::CountDistinct,
        "min" => AggType::Min,
        "max" => AggType::Max,
        "avg" => AggType::Avg,
        _ => AggType::Sum,
    }
}
