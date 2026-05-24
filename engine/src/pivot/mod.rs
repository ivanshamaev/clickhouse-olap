//! Pivot shaper: shapes pre-aggregated ClickHouse rows into a PivotResponse.
//!
//! Invariants:
//!   • Non-additive totals (count distinct, avg) come from ClickHouse, not summed here (FR-PIVOT-04b).
//!   • Shaping is deterministic: identical input → identical output (FR-PIVOT-08).

use serde_json::Value as JsonValue;

use crate::api::types::{Axis, AxisMember, Cell, MeasureMeta, PivotResponse, Stats};
use crate::query::plan::{QueryPlan, SubQueryKind};

/// One batch of rows from a single sub-query execution.
pub struct SubQueryRows {
    pub kind: SubQueryKind,
    pub rows: Vec<JsonValue>,
    pub elapsed_ms: u64,
}

/// Shape all sub-query results into a final PivotResponse.
pub fn shape(
    request_id: &str,
    plan: &QueryPlan,
    sub_results: Vec<SubQueryRows>,
    total_engine_ms: u64,
) -> PivotResponse {
    let n_rows = plan.row_fields.len();

    let row_aliases: Vec<&str> = plan.row_fields.iter().map(|f| f.alias.as_str()).collect();
    let col_aliases: Vec<&str> = plan.col_fields.iter().map(|f| f.alias.as_str()).collect();
    let measure_ids: Vec<&str> = plan.measures.iter().map(|m| m.id.as_str()).collect();

    // Use ordered maps so output is deterministic.
    // Row key (Vec<JsonValue>) → insertion index in row_members
    let mut row_key_to_idx: Vec<(Vec<OrdValue>, usize)> = Vec::new();
    let mut row_members: Vec<AxisMember> = Vec::new();

    // Col key → insertion index; collected before sorting
    let mut col_key_to_idx: Vec<(Vec<OrdValue>, usize)> = Vec::new();
    let mut col_members_unsorted: Vec<AxisMember> = Vec::new();

    // Raw cells (may reference unsorted col indices — will be remapped later)
    struct RawCell {
        r: usize,
        c_unsorted: usize,
        m: String,
        v: JsonValue,
    }
    let mut raw_cells: Vec<RawCell> = Vec::new();

    let mut total_clickhouse_ms = 0u64;
    let mut total_preaggregate_rows = 0u64;
    let mut subquery_count = 0u32;

    for sub in sub_results {
        total_clickhouse_ms += sub.elapsed_ms;
        total_preaggregate_rows += sub.rows.len() as u64;
        subquery_count += 1;

        for row in &sub.rows {
            let obj = match row.as_object() {
                Some(o) => o,
                None => continue,
            };

            // ── Column key ────────────────────────────────────────────────────
            let col_key_json: Vec<JsonValue> = col_aliases
                .iter()
                .map(|a| obj.get(*a).cloned().unwrap_or(JsonValue::Null))
                .collect();
            let col_ord: Vec<OrdValue> = col_key_json.iter().cloned().map(OrdValue).collect();

            let col_idx = find_or_insert(&mut col_key_to_idx, col_ord, || {
                col_members_unsorted.push(AxisMember {
                    key: col_key_json.clone(),
                    is_subtotal: None,
                    is_grand_total: None,
                });
                col_members_unsorted.len() - 1
            });

            // ── Row key ───────────────────────────────────────────────────────
            let (row_key_json, is_subtotal, is_grand_total) = match &sub.kind {
                SubQueryKind::Main => {
                    let key: Vec<JsonValue> = row_aliases
                        .iter()
                        .map(|a| obj.get(*a).cloned().unwrap_or(JsonValue::Null))
                        .collect();
                    (key, false, false)
                }
                SubQueryKind::RowSubtotal { level } => {
                    let mut key: Vec<JsonValue> = row_aliases[..*level]
                        .iter()
                        .map(|a| obj.get(*a).cloned().unwrap_or(JsonValue::Null))
                        .collect();
                    for _ in *level..n_rows {
                        key.push(JsonValue::String("__TOTAL__".to_string()));
                    }
                    (key, true, false)
                }
                SubQueryKind::GrandTotal => {
                    let key = vec![JsonValue::String("__GRAND_TOTAL__".to_string())];
                    (key, false, true)
                }
            };

            let row_ord: Vec<OrdValue> = row_key_json.iter().cloned().map(OrdValue).collect();
            let row_idx = find_or_insert(&mut row_key_to_idx, row_ord, || {
                row_members.push(AxisMember {
                    key: row_key_json.clone(),
                    is_subtotal: if is_subtotal { Some(true) } else { None },
                    is_grand_total: if is_grand_total { Some(true) } else { None },
                });
                row_members.len() - 1
            });

            // ── Cells ─────────────────────────────────────────────────────────
            for m_id in &measure_ids {
                if let Some(v) = obj.get(*m_id) {
                    raw_cells.push(RawCell {
                        r: row_idx,
                        c_unsorted: col_idx,
                        m: m_id.to_string(),
                        v: v.clone(),
                    });
                }
            }
        }
    }

    // ── Sort column axis ──────────────────────────────────────────────────────
    // col_key_to_idx contains (ord_key, original_insertion_idx) pairs.
    // Sort by ord_key, then build old_idx → new_idx mapping.
    let mut col_sort_order: Vec<(Vec<OrdValue>, usize)> = col_key_to_idx.clone();
    col_sort_order.sort_by(|a, b| a.0.cmp(&b.0));

    let mut old_col_to_new: Vec<usize> = vec![0; col_members_unsorted.len()];
    let mut sorted_col_members: Vec<AxisMember> = Vec::with_capacity(col_members_unsorted.len());
    for (new_idx, (_, old_idx)) in col_sort_order.iter().enumerate() {
        old_col_to_new[*old_idx] = new_idx;
        sorted_col_members.push(col_members_unsorted[*old_idx].clone());
    }

    // Remap cell column indices
    let cells: Vec<Cell> = raw_cells
        .into_iter()
        .map(|rc| Cell {
            r: rc.r,
            c: old_col_to_new[rc.c_unsorted],
            m: rc.m,
            v: rc.v,
        })
        .collect();

    let measures: Vec<MeasureMeta> = plan
        .measures
        .iter()
        .map(|m| MeasureMeta {
            id: m.id.clone(),
            format: m.format.clone(),
            value_type: Some("number".to_string()),
        })
        .collect();

    PivotResponse {
        api_version: "1".to_string(),
        request_id: request_id.to_string(),
        status: "ok",
        from_cache: false,
        data_version: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        row_axis: Axis {
            fields: plan.row_fields.iter().map(|f| f.alias.clone()).collect(),
            members: row_members,
        },
        column_axis: Axis {
            fields: plan.col_fields.iter().map(|f| f.alias.clone()).collect(),
            members: sorted_col_members,
        },
        measures,
        cells,
        warnings: vec![],
        stats: Stats {
            subqueries: subquery_count,
            engine_ms: total_engine_ms,
            clickhouse_ms: total_clickhouse_ms,
            preaggregate_rows: total_preaggregate_rows,
        },
    }
}

/// Find existing entry by `key` in the vec of (key, idx) pairs, or call `insert_fn`
/// to create a new entry and return its index.
fn find_or_insert<F>(
    vec: &mut Vec<(Vec<OrdValue>, usize)>,
    key: Vec<OrdValue>,
    insert_fn: F,
) -> usize
where
    F: FnOnce() -> usize,
{
    if let Some(&(_, idx)) = vec.iter().find(|(k, _)| k == &key) {
        return idx;
    }
    let idx = insert_fn();
    vec.push((key, idx));
    idx
}

/// Wrapper for JsonValue that implements Ord for deterministic axis sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OrdValue(JsonValue);

impl PartialOrd for OrdValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrdValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        json_sort_key(&self.0).cmp(&json_sort_key(&other.0))
    }
}

fn json_sort_key(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => format!("s:{s}"),
        JsonValue::Number(n) => {
            if let Some(f) = n.as_f64() {
                format!("n:{:025.6}", f)
            } else {
                format!("n:{n}")
            }
        }
        JsonValue::Null => "z:null".to_string(),
        other => format!("o:{other}"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::AggType;
    use crate::query::plan::{FilterParams, QueryPlan, ResolvedField};
    use crate::query::ResolvedMeasure;
    use serde_json::json;

    fn make_plan() -> QueryPlan {
        QueryPlan {
            table: "sales".to_string(),
            row_fields: vec![ResolvedField {
                model_field: "region".to_string(),
                sql_expr: "region".to_string(),
                alias: "region".to_string(),
            }],
            col_fields: vec![ResolvedField {
                model_field: "order_date".to_string(),
                sql_expr: "toStartOfMonth(order_date)".to_string(),
                alias: "order_date@month".to_string(),
            }],
            measures: vec![
                ResolvedMeasure {
                    id: "revenue".to_string(),
                    field: "amount".to_string(),
                    agg: AggType::Sum,
                    additive: true,
                    format: Some("currency".to_string()),
                },
                ResolvedMeasure {
                    id: "orders".to_string(),
                    field: "order_id".to_string(),
                    agg: AggType::CountDistinct,
                    additive: false,
                    format: Some("integer".to_string()),
                },
            ],
            filter_params: FilterParams::default(),
            sub_queries: vec![],
            max_groups: 200_000,
        }
    }

    #[test]
    fn shape_leaf_rows_produces_correct_axes() {
        let plan = make_plan();
        let sub = SubQueryRows {
            kind: SubQueryKind::Main,
            rows: vec![
                json!({"region": "EE", "order_date@month": "2025-01-01", "revenue": 5000, "orders": 10}),
                json!({"region": "LV", "order_date@month": "2025-01-01", "revenue": 12500, "orders": 87}),
            ],
            elapsed_ms: 100,
        };

        let resp = shape("req-1", &plan, vec![sub], 10);

        assert_eq!(resp.status, "ok");
        assert_eq!(resp.row_axis.members.len(), 2);
        assert_eq!(resp.column_axis.members.len(), 1);
        assert_eq!(resp.cells.len(), 4); // 2 rows × 2 measures
    }

    #[test]
    fn shape_grand_total_marked_correctly() {
        let plan = make_plan();
        let main_sub = SubQueryRows {
            kind: SubQueryKind::Main,
            rows: vec![
                json!({"region": "LV", "order_date@month": "2025-01-01", "revenue": 12500, "orders": 87}),
            ],
            elapsed_ms: 50,
        };
        let total_sub = SubQueryRows {
            kind: SubQueryKind::GrandTotal,
            // Non-additive total from ClickHouse (FR-PIVOT-04b):
            // orders=87 is not sum of subtotals but computed by ClickHouse uniq()
            rows: vec![json!({"order_date@month": "2025-01-01", "revenue": 12500, "orders": 87})],
            elapsed_ms: 30,
        };

        let resp = shape("req-2", &plan, vec![main_sub, total_sub], 20);

        let grand_total = resp
            .row_axis
            .members
            .iter()
            .find(|m| m.is_grand_total == Some(true));
        assert!(grand_total.is_some(), "grand total row must be present");
        assert_eq!(grand_total.unwrap().key[0], json!("__GRAND_TOTAL__"));
    }

    #[test]
    fn shape_subtotal_sentinel_value() {
        let mut plan = make_plan();
        plan.row_fields.push(ResolvedField {
            model_field: "product_category".to_string(),
            sql_expr: "product_category".to_string(),
            alias: "product_category".to_string(),
        });

        let main_sub = SubQueryRows {
            kind: SubQueryKind::Main,
            rows: vec![
                json!({"region": "LV", "product_category": "Electronics", "order_date@month": "2025-01-01", "revenue": 5000, "orders": 30}),
            ],
            elapsed_ms: 50,
        };
        let subtotal_sub = SubQueryRows {
            kind: SubQueryKind::RowSubtotal { level: 1 },
            rows: vec![
                // Non-additive orders computed by ClickHouse at region level (FR-PIVOT-04b)
                json!({"region": "LV", "order_date@month": "2025-01-01", "revenue": 5000, "orders": 30}),
            ],
            elapsed_ms: 30,
        };

        let resp = shape("req-3", &plan, vec![main_sub, subtotal_sub], 20);

        let subtotal = resp
            .row_axis
            .members
            .iter()
            .find(|m| m.is_subtotal == Some(true));
        assert!(subtotal.is_some(), "subtotal row must exist");
        assert_eq!(subtotal.unwrap().key[1], json!("__TOTAL__"));
    }

    #[test]
    fn column_axis_is_sorted() {
        let plan = make_plan();
        let sub = SubQueryRows {
            kind: SubQueryKind::Main,
            rows: vec![
                // ClickHouse returns these in random order
                json!({"region": "LV", "order_date@month": "2025-03-01", "revenue": 3000, "orders": 20}),
                json!({"region": "LV", "order_date@month": "2025-01-01", "revenue": 1000, "orders": 10}),
                json!({"region": "LV", "order_date@month": "2025-02-01", "revenue": 2000, "orders": 15}),
            ],
            elapsed_ms: 50,
        };

        let resp = shape("req-4", &plan, vec![sub], 10);

        let keys: Vec<&str> = resp
            .column_axis
            .members
            .iter()
            .map(|m| m.key[0].as_str().unwrap())
            .collect();
        assert_eq!(keys, ["2025-01-01", "2025-02-01", "2025-03-01"]);
    }
}
