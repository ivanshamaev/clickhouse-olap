import {
  EngineError,
  ModelMetadata,
  ModelSummary,
  PivotRequest,
  PivotResponse,
} from "./types";

export class EngineClientError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly retriable: boolean = false,
  ) {
    super(message);
    this.name = "EngineClientError";
  }
}

export interface IEngineClient {
  health(): Promise<void>;
  listModels(): Promise<ModelSummary[]>;
  getModel(id: string): Promise<ModelMetadata>;
  query(request: PivotRequest): Promise<PivotResponse>;
  cancel(requestId: string): Promise<void>;
}

// ── Real HTTP client ──────────────────────────────────────────────────────────

export class EngineClient implements IEngineClient {
  constructor(private readonly baseUrl: string) {}

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });

    if (!response.ok) {
      let err: EngineError | null = null;
      try {
        err = (await response.json()) as EngineError;
      } catch {
        // not JSON
      }
      if (err?.status === "error") {
        throw new EngineClientError(err.error.code, err.error.message_user, err.error.retriable);
      }
      throw new EngineClientError("INTERNAL", `HTTP ${response.status}: ${response.statusText}`);
    }

    return response.json() as Promise<T>;
  }

  async health(): Promise<void> {
    await this.request<unknown>("GET", "/v1/health");
  }

  async listModels(): Promise<ModelSummary[]> {
    return this.request<ModelSummary[]>("GET", "/v1/models");
  }

  async getModel(id: string): Promise<ModelMetadata> {
    return this.request<ModelMetadata>("GET", `/v1/models/${encodeURIComponent(id)}`);
  }

  async query(req: PivotRequest): Promise<PivotResponse> {
    return this.request<PivotResponse>("POST", "/v1/query", req);
  }

  async cancel(requestId: string): Promise<void> {
    await this.request<unknown>("POST", `/v1/query/${encodeURIComponent(requestId)}/cancel`);
  }
}

// ── Mock client (used when ?mock=1 or in tests) ───────────────────────────────

const MOCK_MODEL: ModelMetadata = {
  model_id: "sales_model",
  label: "Sales (demo)",
  dimensions: [
    { field: "region", type: "categorical", label: "Region" },
    {
      field: "order_date",
      type: "date",
      label: "Order Date",
      hierarchy: ["year", "quarter", "month", "day"],
    },
    { field: "product_category", type: "categorical", label: "Product Category" },
  ],
  measures: [
    { id: "revenue", field: "amount", label: "Revenue", allowed_agg: ["sum", "avg", "min", "max"], additive: true },
    { id: "orders", field: "order_id", label: "Orders", allowed_agg: ["count", "count_distinct"], additive: false },
  ],
  filterable_fields: ["region", "order_date", "product_category"],
  guards: { max_groups: 200000, high_cardinality_fields: ["order_id", "customer_id"] },
};

export class MockEngineClient implements IEngineClient {
  async health(): Promise<void> {
    await delay(80);
  }

  async listModels(): Promise<ModelSummary[]> {
    await delay(100);
    return [{ model_id: "sales_model", label: "Sales (demo)" }];
  }

  async getModel(_id: string): Promise<ModelMetadata> {
    await delay(120);
    return MOCK_MODEL;
  }

  async query(req: PivotRequest): Promise<PivotResponse> {
    await delay(400);

    const rowFields = req.rows.map((r) => r.field);
    const colField = req.columns[0]?.field ?? null;
    const measureId = req.measures[0]?.id ?? "revenue";

    const rowMembers = [
      { key: ["LV", "Electronics"] },
      { key: ["LV", "Clothing"] },
      ...(req.totals.row_subtotals ? [{ key: ["LV", "__TOTAL__"], is_subtotal: true }] : []),
      { key: ["EE", "Electronics"] },
      { key: ["EE", "Clothing"] },
      ...(req.totals.row_subtotals ? [{ key: ["EE", "__TOTAL__"], is_subtotal: true }] : []),
      ...(req.totals.grand_total ? [{ key: ["__GRAND_TOTAL__"], is_grand_total: true }] : []),
    ];

    const colMembers = colField
      ? [{ key: ["2025-01"] }, { key: ["2025-02"] }, { key: ["2025-03"] }]
      : [];

    const cells = buildMockCells(rowMembers, colMembers, measureId);

    return {
      api_version: "1",
      request_id: req.request_id,
      status: "ok",
      from_cache: false,
      data_version: new Date().toISOString(),
      row_axis: { fields: rowFields, members: rowMembers },
      column_axis: { fields: colField ? [colField] : [], members: colMembers },
      measures: req.measures.map((m) => ({ id: m.id, format: m.id === "revenue" ? "currency" : "integer" })),
      cells,
      warnings: [],
      stats: { subqueries: 3, engine_ms: 38, clickhouse_ms: 362, preaggregate_rows: cells.length },
    };
  }

  async cancel(_requestId: string): Promise<void> {
    await delay(50);
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function buildMockCells(
  rows: { key: (string | null)[] }[],
  cols: { key: (string | null)[] }[],
  measureId: string,
): import("./types").Cell[] {
  const cells: import("./types").Cell[] = [];
  const base = measureId === "orders" ? 50 : 5000;
  const effectiveCols = cols.length > 0 ? cols : [null];

  rows.forEach((row, r) => {
    effectiveCols.forEach((col, c) => {
      const seed = (r + 1) * (c + 1) * 1234;
      const v = row.key.includes("__GRAND_TOTAL__")
        ? base * 4 * effectiveCols.length
        : row.key.some((k) => k === "__TOTAL__")
        ? base * 2 * (col ? 1 : effectiveCols.length)
        : (seed % 1000) + base;
      cells.push({ r, c, m: measureId, v });
    });
  });

  return cells;
}
