import { ResultRenderer } from "../../src/components/ResultRenderer";
import { PivotResponse } from "../../src/api/types";

const RESPONSE: PivotResponse = {
  api_version: "1",
  request_id: "test-1",
  status: "ok",
  from_cache: false,
  data_version: "2026-05-23T00:00:00Z",
  row_axis: {
    fields: ["region", "product_category"],
    members: [
      { key: ["LV", "Electronics"] },
      { key: ["LV", "Clothing"] },
      { key: ["LV", "__TOTAL__"], is_subtotal: true },
      { key: ["__GRAND_TOTAL__"], is_grand_total: true },
    ],
  },
  column_axis: {
    fields: ["order_date@month"],
    members: [{ key: ["2025-01"] }, { key: ["2025-02"] }],
  },
  measures: [{ id: "revenue", format: "currency", type: "number" }],
  cells: [
    { r: 0, c: 0, m: "revenue", v: 12500 },
    { r: 0, c: 1, m: "revenue", v: 13200 },
    { r: 1, c: 0, m: "revenue", v: 5000 },
    { r: 1, c: 1, m: "revenue", v: 5200 },
    { r: 2, c: 0, m: "revenue", v: 17500 },
    { r: 2, c: 1, m: "revenue", v: 18400 },
    { r: 3, c: 0, m: "revenue", v: 22500 },
    { r: 3, c: 1, m: "revenue", v: 23600 },
  ],
  warnings: [],
  stats: { subqueries: 2, engine_ms: 30, clickhouse_ms: 400, preaggregate_rows: 8 },
};

describe("ResultRenderer", () => {
  it("calls Excel.run", async () => {
    const renderer = new ResultRenderer();
    await renderer.render(RESPONSE);
    const excelGlobal = global as unknown as { Excel: { run: jest.Mock } };
    expect(excelGlobal.Excel.run).toHaveBeenCalledTimes(1);
  });

  it("calls sync after writing data", async () => {
    const renderer = new ResultRenderer();
    let syncCalled = false;
    const excelGlobal = global as unknown as { Excel: { run: jest.Mock } };
    const originalRun = excelGlobal.Excel.run;

    originalRun.mockImplementationOnce(async (cb: (ctx: { workbook: { worksheets: { getActiveWorksheet: () => { getRangeByIndexes: () => { values: unknown; numberFormat: unknown; format: unknown; columnWidth: unknown; merge: () => void; clear: () => void }; }; }; }; sync: () => Promise<void> }) => Promise<void>) => {
      const range = () => ({
        values: [] as unknown,
        numberFormat: [] as unknown,
        format: { fill: { color: "" }, font: { color: "", bold: false }, horizontalAlignment: "" },
        columnWidth: 0,
        merge: jest.fn(),
        clear: jest.fn(),
      });
      const ctx = {
        workbook: { worksheets: { getActiveWorksheet: () => ({ getRangeByIndexes: range }) } },
        sync: jest.fn().mockImplementation(() => { syncCalled = true; return Promise.resolve(); }),
      };
      await cb(ctx);
    });

    await renderer.render(RESPONSE);
    expect(syncCalled).toBe(true);
  });

  it("renders without error for empty column axis", async () => {
    const renderer = new ResultRenderer();
    const resp: PivotResponse = {
      ...RESPONSE,
      column_axis: { fields: [], members: [] },
      cells: [{ r: 0, c: 0, m: "revenue", v: 5000 }],
    };
    await expect(renderer.render(resp)).resolves.toBeUndefined();
  });

  it("renders without error for grand-total-only response", async () => {
    const renderer = new ResultRenderer();
    const resp: PivotResponse = {
      ...RESPONSE,
      row_axis: { fields: ["region"], members: [{ key: ["__GRAND_TOTAL__"], is_grand_total: true }] },
      column_axis: { fields: [], members: [] },
      cells: [{ r: 0, c: 0, m: "revenue", v: 99999 }],
    };
    await expect(renderer.render(resp)).resolves.toBeUndefined();
  });

  it("renders without error for empty pivot (no rows, no cols)", async () => {
    const renderer = new ResultRenderer();
    const resp: PivotResponse = {
      ...RESPONSE,
      row_axis: { fields: [], members: [] },
      column_axis: { fields: [], members: [] },
      cells: [],
    };
    await expect(renderer.render(resp)).resolves.toBeUndefined();
  });
});
