// Global Office.js mock for Jest (jsdom environment).
// Loaded via jest.config.js setupFiles.

const roamingStore: Record<string, unknown> = {};

const roamingSettings = {
  get: jest.fn((key: string) => roamingStore[key]),
  set: jest.fn((key: string, value: unknown) => { roamingStore[key] = value; }),
  saveAsync: jest.fn((callback: (result: { status: string }) => void) => {
    callback({ status: "succeeded" });
  }),
  remove: jest.fn((key: string) => { delete roamingStore[key]; }),
};

(global as Record<string, unknown>).Office = {
  onReady: jest.fn((cb: () => void) => cb()),
  context: {
    roamingSettings,
  },
  AsyncResultStatus: {
    Succeeded: "succeeded",
    Failed: "failed",
  },
};

// ── Excel mock ────────────────────────────────────────────────────────────────

function makeRange(): ExcelRangeMock {
  return {
    values: [],
    numberFormat: [],
    format: {
      fill: { color: "" },
      font: { color: "", bold: false },
      horizontalAlignment: "",
    },
    columnWidth: 0,
    merge: jest.fn(),
    clear: jest.fn(),
  };
}

export interface ExcelRangeMock {
  values: unknown[][];
  numberFormat: unknown[][];
  format: { fill: { color: string }; font: { color: string; bold: boolean }; horizontalAlignment: string };
  columnWidth: number;
  merge: jest.Mock;
  clear: jest.Mock;
}

function makeSheet(): ExcelSheetMock {
  return {
    getRangeByIndexes: jest.fn(() => makeRange()),
    worksheets: undefined,
  };
}

export interface ExcelSheetMock {
  getRangeByIndexes: jest.Mock;
  worksheets: unknown;
}

function makeContext(): ExcelContextMock {
  const sheet = makeSheet();
  return {
    workbook: {
      worksheets: {
        getActiveWorksheet: jest.fn(() => sheet),
      },
    },
    sync: jest.fn().mockResolvedValue(undefined),
  };
}

export interface ExcelContextMock {
  workbook: { worksheets: { getActiveWorksheet: jest.Mock } };
  sync: jest.Mock;
}

(global as Record<string, unknown>).Excel = {
  run: jest.fn(async (cb: (ctx: ExcelContextMock) => Promise<void>) => {
    const ctx = makeContext();
    await cb(ctx);
    return ctx;
  }),
  ClearApplyTo: { all: "all" },
};

export { roamingSettings, roamingStore };
