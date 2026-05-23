// API contract types — mirrors docs/contract/ (doc 03).
// api_version "1" is the current version. Breaking changes require "/v2".

export type SortDir = "asc" | "desc";
export type AggType = "sum" | "count" | "count_distinct" | "min" | "max" | "avg";
export type DateGranularity = "year" | "quarter" | "month" | "week" | "day";
export type FilterOp = "in" | "not_in" | "between" | "eq" | "neq" | "gt" | "gte" | "lt" | "lte";
export type DimensionType = "categorical" | "date" | "numeric_bucket";

// ── Request ─────────────────────────────────────────────────────────────────

export interface RowDimension {
  field: string;
  sort?: { by: "value" | "measure"; measure?: string; dir: SortDir };
  top_n?: number;
}

export interface ColumnDimension {
  field: string;
  date_granularity?: DateGranularity;
}

export interface Measure {
  id: string;
  field?: string;
  agg?: AggType;
  /** Calculated measure — evaluated in Rust after aggregates (FR-PIVOT-06). */
  type?: "calculated";
  expr?: string;
}

export interface Filter {
  field: string;
  op: FilterOp;
  value: string | number | boolean | (string | number)[];
}

export interface TotalsConfig {
  row_subtotals: boolean;
  column_subtotals: boolean;
  grand_total: boolean;
}

export interface LimitsConfig {
  max_cells?: number;
  max_groups?: number;
  bypass_cache?: boolean;
}

export interface PivotRequest {
  api_version: "1";
  request_id: string;
  model_id: string;
  rows: RowDimension[];
  columns: ColumnDimension[];
  measures: Measure[];
  filters: Filter[];
  totals: TotalsConfig;
  limits?: LimitsConfig;
}

// ── Response ─────────────────────────────────────────────────────────────────

export interface AxisMember {
  key: (string | number | null)[];
  is_subtotal?: boolean;
  is_grand_total?: boolean;
}

export interface Axis {
  fields: string[];
  members: AxisMember[];
}

export interface MeasureMetadata {
  id: string;
  format?: "integer" | "decimal" | "currency" | "percent";
  type?: "number" | "string";
}

export interface Cell {
  r: number;
  c: number;
  m: string;
  v: number | null;
}

export interface PivotStats {
  subqueries: number;
  engine_ms: number;
  clickhouse_ms: number;
  preaggregate_rows: number;
}

export interface PivotResponse {
  api_version: "1";
  request_id: string;
  status: "ok";
  from_cache: boolean;
  data_version: string;
  row_axis: Axis;
  column_axis: Axis;
  measures: MeasureMetadata[];
  cells: Cell[];
  warnings: string[];
  stats: PivotStats;
}

// ── Error response ────────────────────────────────────────────────────────────

export type ErrorCode =
  | "VALIDATION_FAILED"
  | "FIELD_NOT_ALLOWED"
  | "RESULT_TOO_LARGE"
  | "PREAGGREGATE_TOO_LARGE"
  | "QUERY_TIMEOUT"
  | "CLICKHOUSE_UNAVAILABLE"
  | "CANCELLED"
  | "INTERNAL";

export interface EngineError {
  status: "error";
  request_id: string;
  error: {
    code: ErrorCode;
    message_user: string;
    retriable: boolean;
  };
}

// ── Model metadata ────────────────────────────────────────────────────────────

export interface DimensionMeta {
  field: string;
  type: DimensionType;
  label?: string;
  hierarchy?: DateGranularity[];
  bucket_size?: number;
}

export interface MeasureMeta {
  id: string;
  field: string;
  label?: string;
  allowed_agg: AggType[];
  additive: boolean;
}

export interface ModelGuards {
  max_groups: number;
  high_cardinality_fields: string[];
}

export interface ModelMetadata {
  model_id: string;
  label?: string;
  dimensions: DimensionMeta[];
  measures: MeasureMeta[];
  filterable_fields: string[];
  guards: ModelGuards;
}

export interface ModelSummary {
  model_id: string;
  label?: string;
}
