import { EngineClient, MockEngineClient } from "../../src/api/engineClient";
import { PivotRequest } from "../../src/api/types";

// ── MockEngineClient ──────────────────────────────────────────────────────────

describe("MockEngineClient", () => {
  const client = new MockEngineClient();

  it("health() resolves without error", async () => {
    await expect(client.health()).resolves.toBeUndefined();
  });

  it("listModels() returns at least one model", async () => {
    const models = await client.listModels();
    expect(models.length).toBeGreaterThan(0);
    expect(models[0]).toHaveProperty("model_id");
  });

  it("getModel() returns model metadata with dimensions and measures", async () => {
    const model = await client.getModel("sales_model");
    expect(model.dimensions.length).toBeGreaterThan(0);
    expect(model.measures.length).toBeGreaterThan(0);
    expect(model.guards).toBeDefined();
  });

  it("query() returns a valid PivotResponse with ok status", async () => {
    const req = makeRequest();
    const resp = await client.query(req);
    expect(resp.status).toBe("ok");
    expect(resp.request_id).toBe(req.request_id);
    expect(resp.row_axis.members.length).toBeGreaterThan(0);
  });

  it("query() includes grand total row when requested", async () => {
    const req = makeRequest({ grand_total: true });
    const resp = await client.query(req);
    const grandTotal = resp.row_axis.members.find((m) => m.is_grand_total);
    expect(grandTotal).toBeDefined();
  });

  it("query() omits grand total row when not requested", async () => {
    const req = makeRequest({ grand_total: false });
    const resp = await client.query(req);
    const grandTotal = resp.row_axis.members.find((m) => m.is_grand_total);
    expect(grandTotal).toBeUndefined();
  });

  it("query() includes subtotal rows when requested", async () => {
    const req = makeRequest({ row_subtotals: true });
    const resp = await client.query(req);
    const subtotal = resp.row_axis.members.find((m) => m.is_subtotal);
    expect(subtotal).toBeDefined();
  });

  it("cancel() resolves without error", async () => {
    await expect(client.cancel("some-id")).resolves.toBeUndefined();
  });
});

// ── EngineClient (HTTP) ───────────────────────────────────────────────────────

describe("EngineClient", () => {
  const mockFetch = jest.fn();
  beforeEach(() => {
    global.fetch = mockFetch;
    mockFetch.mockReset();
  });

  function jsonOk(data: unknown): Response {
    return {
      ok: true,
      json: () => Promise.resolve(data),
    } as unknown as Response;
  }

  function jsonError(status: number, body: unknown): Response {
    return {
      ok: false,
      status,
      statusText: "Error",
      json: () => Promise.resolve(body),
    } as unknown as Response;
  }

  it("health() calls GET /v1/health", async () => {
    mockFetch.mockResolvedValue(jsonOk({}));
    const c = new EngineClient("http://localhost:3000");
    await c.health();
    expect(mockFetch).toHaveBeenCalledWith("http://localhost:3000/v1/health", expect.objectContaining({ method: "GET" }));
  });

  it("listModels() calls GET /v1/models and returns data", async () => {
    const models = [{ model_id: "m1", label: "M1" }];
    mockFetch.mockResolvedValue(jsonOk(models));
    const c = new EngineClient("http://localhost:3000");
    const result = await c.listModels();
    expect(result).toEqual(models);
  });

  it("getModel() encodes model id in URL", async () => {
    mockFetch.mockResolvedValue(jsonOk({ model_id: "my model", dimensions: [], measures: [], filterable_fields: [], guards: { max_groups: 1000, high_cardinality_fields: [] } }));
    const c = new EngineClient("http://localhost:3000");
    await c.getModel("my model");
    expect(mockFetch).toHaveBeenCalledWith("http://localhost:3000/v1/models/my%20model", expect.anything());
  });

  it("query() sends POST with request body", async () => {
    const response = makePivotResponse();
    mockFetch.mockResolvedValue(jsonOk(response));
    const c = new EngineClient("http://localhost:3000");
    const req = makeRequest();
    const result = await c.query(req);
    expect(result.status).toBe("ok");
    const [url, init] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("http://localhost:3000/v1/query");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toMatchObject({ request_id: req.request_id });
  });

  it("throws EngineClientError with code from error response", async () => {
    mockFetch.mockResolvedValue(
      jsonError(400, { status: "error", request_id: "r1", error: { code: "VALIDATION_FAILED", message_user: "Bad request", retriable: false } }),
    );
    const c = new EngineClient("http://localhost:3000");
    await expect(c.health()).rejects.toMatchObject({ code: "VALIDATION_FAILED", retriable: false });
  });

  it("throws EngineClientError with INTERNAL for non-JSON error", async () => {
    mockFetch.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: () => Promise.reject(new SyntaxError("not json")),
    } as unknown as Response);
    const c = new EngineClient("http://localhost:3000");
    await expect(c.health()).rejects.toMatchObject({ code: "INTERNAL" });
  });

  it("cancel() calls POST /v1/query/:id/cancel", async () => {
    mockFetch.mockResolvedValue(jsonOk({}));
    const c = new EngineClient("http://localhost:3000");
    await c.cancel("req-abc");
    expect(mockFetch).toHaveBeenCalledWith("http://localhost:3000/v1/query/req-abc/cancel", expect.objectContaining({ method: "POST" }));
  });
});

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeRequest(totalsOverride: Partial<{ grand_total: boolean; row_subtotals: boolean }> = {}): PivotRequest {
  return {
    api_version: "1",
    request_id: "test-req-1",
    model_id: "sales_model",
    rows: [{ field: "region" }],
    columns: [{ field: "order_date", date_granularity: "month" }],
    measures: [{ id: "revenue", field: "amount", agg: "sum" }],
    filters: [],
    totals: { row_subtotals: true, column_subtotals: false, grand_total: true, ...totalsOverride },
    limits: { max_cells: 500000, max_groups: 200000 },
  };
}

function makePivotResponse() {
  return {
    api_version: "1",
    request_id: "test-req-1",
    status: "ok",
    from_cache: false,
    data_version: "2026-05-23T00:00:00Z",
    row_axis: { fields: ["region"], members: [{ key: ["LV"] }] },
    column_axis: { fields: ["order_date@month"], members: [] },
    measures: [{ id: "revenue", format: "currency" }],
    cells: [{ r: 0, c: 0, m: "revenue", v: 12500 }],
    warnings: [],
    stats: { subqueries: 1, engine_ms: 30, clickhouse_ms: 200, preaggregate_rows: 1 },
  };
}
