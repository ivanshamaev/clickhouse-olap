import { PivotBuilder } from "../../src/components/PivotBuilder";
import { ModelMetadata } from "../../src/api/types";

const MODEL: ModelMetadata = {
  model_id: "sales_model",
  label: "Sales",
  dimensions: [
    { field: "region", type: "categorical", label: "Region" },
    { field: "order_date", type: "date", label: "Order Date", hierarchy: ["year", "quarter", "month", "day"] },
    { field: "product_category", type: "categorical", label: "Category" },
  ],
  measures: [
    { id: "revenue", field: "amount", label: "Revenue", allowed_agg: ["sum", "avg", "min", "max"], additive: true },
    { id: "orders", field: "order_id", label: "Orders", allowed_agg: ["count", "count_distinct"], additive: false },
  ],
  filterable_fields: ["region", "order_date", "product_category"],
  guards: { max_groups: 200000, high_cardinality_fields: ["order_id", "customer_id"] },
};

function makeBuilder(): PivotBuilder {
  const b = new PivotBuilder();
  b.loadModel(MODEL);
  return b;
}

// ── loadModel ─────────────────────────────────────────────────────────────────

describe("PivotBuilder.loadModel", () => {
  it("resets state when a new model is loaded", () => {
    const b = makeBuilder();
    b.addRow("region");
    b.loadModel(MODEL);
    expect(b.getState().rows).toHaveLength(0);
  });

  it("exposes model dimensions and measures after loading", () => {
    const b = makeBuilder();
    expect(b.getAvailableDimensions()).toHaveLength(3);
    expect(b.getAvailableMeasures()).toHaveLength(2);
  });
});

// ── Rows ──────────────────────────────────────────────────────────────────────

describe("PivotBuilder rows", () => {
  it("addRow() adds a dimension to rows", () => {
    const b = makeBuilder();
    b.addRow("region");
    expect(b.getState().rows).toEqual([{ field: "region" }]);
  });

  it("addRow() ignores duplicates", () => {
    const b = makeBuilder();
    b.addRow("region");
    b.addRow("region");
    expect(b.getState().rows).toHaveLength(1);
  });

  it("removeRow() removes the dimension", () => {
    const b = makeBuilder();
    b.addRow("region");
    b.addRow("product_category");
    b.removeRow("region");
    expect(b.getState().rows.map((r) => r.field)).toEqual(["product_category"]);
  });

  it("isDimensionUsed() returns true after addRow()", () => {
    const b = makeBuilder();
    b.addRow("region");
    expect(b.isDimensionUsed("region")).toBe(true);
    expect(b.isDimensionUsed("order_date")).toBe(false);
  });
});

// ── Columns ───────────────────────────────────────────────────────────────────

describe("PivotBuilder columns", () => {
  it("addColumn() adds a dimension with optional granularity", () => {
    const b = makeBuilder();
    b.addColumn("order_date", "month");
    expect(b.getState().columns).toEqual([{ field: "order_date", date_granularity: "month" }]);
  });

  it("addColumn() without granularity stores no date_granularity", () => {
    const b = makeBuilder();
    b.addColumn("region");
    expect(b.getState().columns[0].date_granularity).toBeUndefined();
  });

  it("addColumn() ignores duplicates", () => {
    const b = makeBuilder();
    b.addColumn("region");
    b.addColumn("region");
    expect(b.getState().columns).toHaveLength(1);
  });

  it("removeColumn() removes the dimension", () => {
    const b = makeBuilder();
    b.addColumn("region");
    b.addColumn("order_date");
    b.removeColumn("region");
    expect(b.getState().columns.map((c) => c.field)).toEqual(["order_date"]);
  });

  it("isDimensionUsed() returns true after addColumn()", () => {
    const b = makeBuilder();
    b.addColumn("order_date", "month");
    expect(b.isDimensionUsed("order_date")).toBe(true);
  });
});

// ── Measures ──────────────────────────────────────────────────────────────────

describe("PivotBuilder measures", () => {
  it("addMeasure() uses first allowed_agg by default", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0]);
    expect(b.getState().measures[0].agg).toBe("sum");
  });

  it("addMeasure() uses provided agg override", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0], "avg");
    expect(b.getState().measures[0].agg).toBe("avg");
  });

  it("addMeasure() ignores duplicates", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0]);
    b.addMeasure(MODEL.measures[0]);
    expect(b.getState().measures).toHaveLength(1);
  });

  it("removeMeasure() removes by id", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0]);
    b.addMeasure(MODEL.measures[1]);
    b.removeMeasure("revenue");
    expect(b.getState().measures.map((m) => m.id)).toEqual(["orders"]);
  });

  it("setMeasureAgg() changes the aggregation of an existing measure", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0]);
    b.setMeasureAgg("revenue", "max");
    expect(b.getState().measures[0].agg).toBe("max");
  });

  it("isMeasureUsed() returns true after addMeasure()", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0]);
    expect(b.isMeasureUsed("revenue")).toBe(true);
    expect(b.isMeasureUsed("orders")).toBe(false);
  });
});

// ── Totals ────────────────────────────────────────────────────────────────────

describe("PivotBuilder totals", () => {
  it("has row_subtotals and grand_total enabled by default", () => {
    const b = makeBuilder();
    expect(b.getState().totals.row_subtotals).toBe(true);
    expect(b.getState().totals.grand_total).toBe(true);
  });

  it("setTotals() merges partial update", () => {
    const b = makeBuilder();
    b.setTotals({ grand_total: false });
    expect(b.getState().totals.grand_total).toBe(false);
    expect(b.getState().totals.row_subtotals).toBe(true);
  });
});

// ── Validation ────────────────────────────────────────────────────────────────

describe("PivotBuilder.validate", () => {
  it("returns error when no measures added", () => {
    const b = makeBuilder();
    b.addRow("region");
    const errors = b.validate();
    expect(errors).toContain("Add at least one measure");
  });

  it("returns error when no dimensions added", () => {
    const b = makeBuilder();
    b.addMeasure(MODEL.measures[0]);
    const errors = b.validate();
    expect(errors.some((e) => e.includes("dimension"))).toBe(true);
  });

  it("returns no errors for valid state", () => {
    const b = makeBuilder();
    b.addRow("region");
    b.addMeasure(MODEL.measures[0]);
    expect(b.validate()).toHaveLength(0);
  });

  it("returns error when model not loaded", () => {
    const b = new PivotBuilder();
    expect(b.validate()).toContain("No model loaded");
  });
});

// ── buildRequest ──────────────────────────────────────────────────────────────

describe("PivotBuilder.buildRequest", () => {
  it("builds a valid PivotRequest with all state fields", () => {
    const b = makeBuilder();
    b.addRow("region");
    b.addColumn("order_date", "month");
    b.addMeasure(MODEL.measures[0]);
    b.setTotals({ grand_total: true, row_subtotals: true });

    const req = b.buildRequest("sales_model");
    expect(req.api_version).toBe("1");
    expect(req.model_id).toBe("sales_model");
    expect(req.rows).toEqual([{ field: "region" }]);
    expect(req.columns).toEqual([{ field: "order_date", date_granularity: "month" }]);
    expect(req.measures[0].agg).toBe("sum");
    expect(req.totals.grand_total).toBe(true);
    expect(req.request_id).toMatch(/^[0-9a-f-]{36}$/i);
  });

  it("throws when state is invalid", () => {
    const b = makeBuilder();
    expect(() => b.buildRequest("sales_model")).toThrow();
  });

  it("generates unique request_id on each call", () => {
    const b = makeBuilder();
    b.addRow("region");
    b.addMeasure(MODEL.measures[0]);
    const r1 = b.buildRequest("sales_model");
    const r2 = b.buildRequest("sales_model");
    expect(r1.request_id).not.toBe(r2.request_id);
  });
});
