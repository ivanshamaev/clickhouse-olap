//! QueryPlan: describes which SQL sub-queries to run.

use super::ResolvedMeasure;
use crate::api::types::{FieldRef, PivotRequest};
use crate::semantic::SemanticModel;

/// A single SQL sub-query to send to ClickHouse.
#[derive(Debug, Clone)]
pub struct SubQuery {
    pub kind: SubQueryKind,
    /// SELECT column aliases in the order they appear.
    pub select_aliases: Vec<String>,
    /// Measure aliases (subset of select_aliases).
    pub measure_aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubQueryKind {
    /// Full leaf-level data (GROUP BY all row + col dims).
    Main,
    /// Row subtotal at a given level (GROUP BY first `level` row dims + all col dims).
    /// level=1 means GROUP BY rows[0..1] + cols
    RowSubtotal { level: usize },
    /// Grand total: GROUP BY only col dims (or no GROUP BY if no cols).
    GrandTotal,
}

/// Full plan for a pivot request: which sub-queries to run.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub table: String,
    pub row_fields: Vec<ResolvedField>,
    pub col_fields: Vec<ResolvedField>,
    pub measures: Vec<ResolvedMeasure>,
    pub filter_params: FilterParams,
    pub sub_queries: Vec<SubQuery>,
    pub max_groups: u64,
}

/// A dimension field resolved to a SQL expression and alias.
#[derive(Debug, Clone)]
pub struct ResolvedField {
    /// Original field name in the model.
    pub model_field: String,
    /// SQL expression (may include date truncation function).
    pub sql_expr: String,
    /// Column alias used in SELECT and GROUP BY.
    pub alias: String,
}

impl ResolvedField {
    pub fn new(f: &FieldRef) -> Self {
        let alias = match &f.date_granularity {
            Some(gran) => format!("{}@{}", f.field, gran.suffix()),
            None => f.field.clone(),
        };
        let sql_expr = match &f.date_granularity {
            Some(gran) => gran.to_sql_expr(&f.field),
            None => f.field.clone(),
        };
        Self {
            model_field: f.field.clone(),
            sql_expr,
            alias,
        }
    }
}

/// Collected filter parameters for parameterized queries.
#[derive(Debug, Clone, Default)]
pub struct FilterParams {
    /// List of (param_name, value_string) pairs for URL encoding.
    pub params: Vec<(String, String)>,
    /// SQL WHERE clause fragments (using {pN:Type} placeholders).
    pub clauses: Vec<String>,
}

pub fn build_plan(
    req: &PivotRequest,
    model: &SemanticModel,
    measures: &[ResolvedMeasure],
    max_groups: u64,
) -> QueryPlan {
    let row_fields: Vec<ResolvedField> = req.rows.iter().map(ResolvedField::new).collect();
    let col_fields: Vec<ResolvedField> = req.columns.iter().map(ResolvedField::new).collect();

    // Build filter params
    let mut param_counter = 0usize;
    let mut filter_params = FilterParams::default();
    for filter in &req.filters {
        let dim = model.dimension(&filter.field);
        let ch_type = dim.map(|d| field_ch_type(&d.dim_type)).unwrap_or("String");
        build_filter_clause(filter, ch_type, &mut param_counter, &mut filter_params);
    }

    // Build sub-queries list
    let mut sub_queries = Vec::new();

    // Main query (always)
    let main_aliases = row_col_aliases(&row_fields, &col_fields, measures);
    sub_queries.push(SubQuery {
        kind: SubQueryKind::Main,
        select_aliases: main_aliases.clone(),
        measure_aliases: measures.iter().map(|m| m.id.clone()).collect(),
    });

    // Row subtotals (one per level, from level=1 up to n_rows-1)
    if req.totals.row_subtotals && row_fields.len() > 1 {
        for level in 1..row_fields.len() {
            let aliases = row_col_aliases(&row_fields[..level], &col_fields, measures);
            sub_queries.push(SubQuery {
                kind: SubQueryKind::RowSubtotal { level },
                select_aliases: aliases,
                measure_aliases: measures.iter().map(|m| m.id.clone()).collect(),
            });
        }
    }

    // Grand total
    if req.totals.grand_total {
        let aliases = row_col_aliases(&[], &col_fields, measures);
        sub_queries.push(SubQuery {
            kind: SubQueryKind::GrandTotal,
            select_aliases: aliases,
            measure_aliases: measures.iter().map(|m| m.id.clone()).collect(),
        });
    }

    QueryPlan {
        table: model.table().to_string(),
        row_fields,
        col_fields,
        measures: measures.to_vec(),
        filter_params,
        sub_queries,
        max_groups,
    }
}

fn row_col_aliases(
    row_fields: &[ResolvedField],
    col_fields: &[ResolvedField],
    measures: &[ResolvedMeasure],
) -> Vec<String> {
    row_fields
        .iter()
        .map(|f| f.alias.clone())
        .chain(col_fields.iter().map(|f| f.alias.clone()))
        .chain(measures.iter().map(|m| m.id.clone()))
        .collect()
}

fn field_ch_type(dim_type: &str) -> &'static str {
    match dim_type {
        "date" => "Date",
        "categorical" => "String",
        "numeric_bucket" => "Float64",
        _ => "String",
    }
}

fn build_filter_clause(
    filter: &crate::api::types::Filter,
    ch_type: &'static str,
    counter: &mut usize,
    out: &mut FilterParams,
) {
    let field = &filter.field;

    let clause = match &filter.op {
        crate::api::types::FilterOp::Eq => {
            let p = next_param(counter, &filter.value.to_string_lossy(), ch_type, out);
            format!("{field} = {{{p}:{ch_type}}}")
        }
        crate::api::types::FilterOp::Neq => {
            let p = next_param(counter, &filter.value.to_string_lossy(), ch_type, out);
            format!("{field} != {{{p}:{ch_type}}}")
        }
        crate::api::types::FilterOp::In => {
            let vals = filter.value.as_array().cloned().unwrap_or_default();
            let placeholders: Vec<String> = vals
                .iter()
                .map(|v| {
                    let p = next_param(counter, &v.to_string_lossy(), ch_type, out);
                    format!("{{{p}:{ch_type}}}")
                })
                .collect();
            format!("{field} IN ({})", placeholders.join(", "))
        }
        crate::api::types::FilterOp::NotIn => {
            let vals = filter.value.as_array().cloned().unwrap_or_default();
            let placeholders: Vec<String> = vals
                .iter()
                .map(|v| {
                    let p = next_param(counter, &v.to_string_lossy(), ch_type, out);
                    format!("{{{p}:{ch_type}}}")
                })
                .collect();
            format!("{field} NOT IN ({})", placeholders.join(", "))
        }
        crate::api::types::FilterOp::Between => {
            let arr = filter.value.as_array().cloned().unwrap_or_default();
            let (v0, v1) = (
                arr.first().map(|v| v.to_string_lossy()).unwrap_or_default(),
                arr.get(1).map(|v| v.to_string_lossy()).unwrap_or_default(),
            );
            let p0 = next_param(counter, &v0, ch_type, out);
            let p1 = next_param(counter, &v1, ch_type, out);
            format!("{field} BETWEEN {{{p0}:{ch_type}}} AND {{{p1}:{ch_type}}}")
        }
        crate::api::types::FilterOp::Gt => {
            let p = next_param(counter, &filter.value.to_string_lossy(), ch_type, out);
            format!("{field} > {{{p}:{ch_type}}}")
        }
        crate::api::types::FilterOp::Gte => {
            let p = next_param(counter, &filter.value.to_string_lossy(), ch_type, out);
            format!("{field} >= {{{p}:{ch_type}}}")
        }
        crate::api::types::FilterOp::Lt => {
            let p = next_param(counter, &filter.value.to_string_lossy(), ch_type, out);
            format!("{field} < {{{p}:{ch_type}}}")
        }
        crate::api::types::FilterOp::Lte => {
            let p = next_param(counter, &filter.value.to_string_lossy(), ch_type, out);
            format!("{field} <= {{{p}:{ch_type}}}")
        }
        crate::api::types::FilterOp::IsNull => format!("{field} IS NULL"),
        crate::api::types::FilterOp::IsNotNull => format!("{field} IS NOT NULL"),
    };

    out.clauses.push(clause);
}

fn next_param(counter: &mut usize, value: &str, _ch_type: &str, out: &mut FilterParams) -> String {
    let name = format!("p{}", *counter);
    *counter += 1;
    // Strip surrounding quotes that serde_json adds for strings
    let clean = value.trim_matches('"').to_string();
    out.params.push((name.clone(), clean));
    name
}

// Helper trait to convert JsonValue to string for filter params
trait ToStringLossy {
    fn to_string_lossy(&self) -> String;
}

impl ToStringLossy for serde_json::Value {
    fn to_string_lossy(&self) -> String {
        match self {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "".to_string(),
            other => other.to_string(),
        }
    }
}
