//! Contract types — must stay in sync with docs/contract/ and the TypeScript add-in.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── Request ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct PivotRequest {
    pub api_version: String,
    pub request_id: String,
    pub model_id: String,
    pub rows: Vec<FieldRef>,
    pub columns: Vec<FieldRef>,
    pub measures: Vec<MeasureRef>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    pub totals: TotalsConfig,
    #[serde(default)]
    pub limits: RequestLimits,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FieldRef {
    pub field: String,
    pub date_granularity: Option<DateGranularity>,
    pub sort: Option<SortSpec>,
    pub top_n: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DateGranularity {
    Year,
    Quarter,
    Month,
    Day,
}

impl DateGranularity {
    /// ClickHouse date truncation expression.
    pub fn to_sql_expr(&self, field: &str) -> String {
        match self {
            Self::Year => format!("toStartOfYear({field})"),
            Self::Quarter => format!("toStartOfQuarter({field})"),
            Self::Month => format!("toStartOfMonth({field})"),
            Self::Day => format!("toDate({field})"),
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Quarter => "quarter",
            Self::Month => "month",
            Self::Day => "day",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SortSpec {
    pub by: String,
    pub measure: Option<String>,
    #[serde(default = "default_asc")]
    pub dir: String,
}

fn default_asc() -> String {
    "asc".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct MeasureRef {
    pub id: String,
    pub field: Option<String>,
    pub agg: Option<AggType>,
    #[serde(rename = "type")]
    pub measure_type: Option<String>,
    pub expr: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AggType {
    Sum,
    Count,
    CountDistinct,
    Min,
    Max,
    Avg,
}

impl AggType {
    /// SQL aggregation expression for a given field.
    pub fn to_sql_expr(&self, field: &str) -> String {
        match self {
            Self::Sum => format!("sum({field})"),
            Self::Count => format!("count({field})"),
            Self::CountDistinct => format!("uniq({field})"),
            Self::Min => format!("min({field})"),
            Self::Max => format!("max({field})"),
            Self::Avg => format!("avg({field})"),
        }
    }

    /// Whether this aggregation is additive (subtotals can be summed from parts).
    pub fn is_additive(&self) -> bool {
        matches!(self, Self::Sum | Self::Count)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Filter {
    pub field: String,
    pub op: FilterOp,
    pub value: JsonValue,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Neq,
    In,
    NotIn,
    Between,
    Gt,
    Gte,
    Lt,
    Lte,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TotalsConfig {
    #[serde(default)]
    pub row_subtotals: bool,
    #[serde(default)]
    pub column_subtotals: bool,
    #[serde(default)]
    pub grand_total: bool,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct RequestLimits {
    #[serde(default)]
    pub max_cells: Option<u64>,
    #[serde(default)]
    pub max_groups: Option<u64>,
    #[serde(default)]
    pub bypass_cache: Option<bool>,
}

// ── Response ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PivotResponse {
    pub api_version: String,
    pub request_id: String,
    pub status: &'static str,
    pub from_cache: bool,
    pub data_version: String,
    pub row_axis: Axis,
    pub column_axis: Axis,
    pub measures: Vec<MeasureMeta>,
    pub cells: Vec<Cell>,
    pub warnings: Vec<String>,
    pub stats: Stats,
}

#[derive(Debug, Serialize, Clone)]
pub struct Axis {
    pub fields: Vec<String>,
    pub members: Vec<AxisMember>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AxisMember {
    pub key: Vec<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subtotal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_grand_total: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MeasureMeta {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub value_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Cell {
    pub r: usize,
    pub c: usize,
    pub m: String,
    pub v: JsonValue,
}

#[derive(Debug, Serialize, Default)]
pub struct Stats {
    pub subqueries: u32,
    pub engine_ms: u64,
    pub clickhouse_ms: u64,
    pub preaggregate_rows: u64,
}

// ── Model metadata ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct ModelMetadata {
    pub model_id: String,
    pub label: Option<String>,
    pub dimensions: Vec<DimensionMeta>,
    pub measures: Vec<MeasureMetaModel>,
    pub filterable_fields: Vec<String>,
    pub guards: GuardsMeta,
}

#[derive(Debug, Serialize, Clone)]
pub struct ModelSummary {
    pub model_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DimensionMeta {
    pub field: String,
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub dim_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MeasureMetaModel {
    pub id: String,
    pub field: String,
    pub label: Option<String>,
    pub allowed_agg: Vec<String>,
    pub additive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct GuardsMeta {
    pub max_groups: u64,
    pub high_cardinality_fields: Vec<String>,
}

// ── Cancel request ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CancelRequest {
    // body is empty or has nothing; the id comes from the URL path
}
