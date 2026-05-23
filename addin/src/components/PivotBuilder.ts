import { v4 as uuidv4 } from "../utils/uuid";
import {
  AggType,
  ColumnDimension,
  DateGranularity,
  DimensionMeta,
  Filter,
  Measure,
  MeasureMeta,
  ModelMetadata,
  PivotRequest,
  RowDimension,
  TotalsConfig,
} from "../api/types";

export interface PivotBuilderState {
  rows: RowDimension[];
  columns: ColumnDimension[];
  measures: Measure[];
  filters: Filter[];
  totals: TotalsConfig;
}

export class PivotBuilder {
  private state: PivotBuilderState = {
    rows: [],
    columns: [],
    measures: [],
    filters: [],
    totals: { row_subtotals: true, column_subtotals: false, grand_total: true },
  };

  private model: ModelMetadata | null = null;

  // ── State management ─────────────────────────────────────────────────────

  loadModel(model: ModelMetadata): void {
    this.model = model;
    this.state = {
      rows: [],
      columns: [],
      measures: [],
      filters: [],
      totals: { row_subtotals: true, column_subtotals: false, grand_total: true },
    };
  }

  getModel(): ModelMetadata | null {
    return this.model;
  }

  getState(): Readonly<PivotBuilderState> {
    return this.state;
  }

  addRow(field: string): void {
    if (this.state.rows.some((r) => r.field === field)) return;
    this.state.rows.push({ field });
  }

  removeRow(field: string): void {
    this.state.rows = this.state.rows.filter((r) => r.field !== field);
  }

  addColumn(field: string, granularity?: DateGranularity): void {
    if (this.state.columns.some((c) => c.field === field)) return;
    const col: ColumnDimension = { field };
    if (granularity) col.date_granularity = granularity;
    this.state.columns.push(col);
  }

  removeColumn(field: string): void {
    this.state.columns = this.state.columns.filter((c) => c.field !== field);
  }

  addMeasure(measureMeta: MeasureMeta, agg?: AggType): void {
    if (this.state.measures.some((m) => m.id === measureMeta.id)) return;
    const effectiveAgg: AggType = agg ?? measureMeta.allowed_agg[0];
    this.state.measures.push({ id: measureMeta.id, field: measureMeta.field, agg: effectiveAgg });
  }

  removeMeasure(id: string): void {
    this.state.measures = this.state.measures.filter((m) => m.id !== id);
  }

  setMeasureAgg(id: string, agg: AggType): void {
    const m = this.state.measures.find((m) => m.id === id);
    if (m) m.agg = agg;
  }

  setTotals(totals: Partial<TotalsConfig>): void {
    this.state.totals = { ...this.state.totals, ...totals };
  }

  // ── Validation ────────────────────────────────────────────────────────────

  validate(): string[] {
    const errors: string[] = [];
    if (!this.model) errors.push("No model loaded");
    if (this.state.measures.length === 0) errors.push("Add at least one measure");
    if (this.state.rows.length === 0 && this.state.columns.length === 0) {
      errors.push("Add at least one dimension to rows or columns");
    }
    return errors;
  }

  // ── Build request ─────────────────────────────────────────────────────────

  buildRequest(modelId: string): PivotRequest {
    const errors = this.validate();
    if (errors.length > 0) throw new Error(errors.join("; "));

    return {
      api_version: "1",
      request_id: uuidv4(),
      model_id: modelId,
      rows: this.state.rows,
      columns: this.state.columns,
      measures: this.state.measures,
      filters: this.state.filters,
      totals: this.state.totals,
      limits: { max_cells: 500000, max_groups: 200000 },
    };
  }

  // ── Helpers ───────────────────────────────────────────────────────────────

  getAvailableDimensions(): DimensionMeta[] {
    return this.model?.dimensions ?? [];
  }

  getAvailableMeasures(): MeasureMeta[] {
    return this.model?.measures ?? [];
  }

  isDimensionUsed(field: string): boolean {
    return (
      this.state.rows.some((r) => r.field === field) ||
      this.state.columns.some((c) => c.field === field)
    );
  }

  isMeasureUsed(id: string): boolean {
    return this.state.measures.some((m) => m.id === id);
  }
}
