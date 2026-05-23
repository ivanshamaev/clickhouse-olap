import { EngineClient, EngineClientError, IEngineClient, MockEngineClient } from "../api/engineClient";
import { AggType, ModelMetadata, ModelSummary, PivotResponse } from "../api/types";
import { PivotBuilder } from "../components/PivotBuilder";
import { ResultRenderer } from "../components/ResultRenderer";
import { loadSettings, saveSettings } from "../utils/settings";

// Use mock client when running without a real engine (append ?mock=1 to taskpane URL)
const USE_MOCK = new URLSearchParams(window.location.search).get("mock") === "1";

let client: IEngineClient = USE_MOCK
  ? new MockEngineClient()
  : new EngineClient(loadSettings().engineUrl);

const builder = new PivotBuilder();
const renderer = new ResultRenderer();

let currentRequestId: string | null = null;
let isStale = false;

// ── DOM refs ──────────────────────────────────────────────────────────────────

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

const settingsBtn   = $<HTMLButtonElement>("settings-btn");
const settingsPanel = $<HTMLDivElement>("settings-panel");
const engineUrlInput = $<HTMLInputElement>("engine-url");
const testBtn       = $<HTMLButtonElement>("test-btn");
const connStatus    = $<HTMLDivElement>("conn-status");

const modelSelect   = $<HTMLSelectElement>("model-select");
const loadModelBtn  = $<HTMLButtonElement>("load-model-btn");

const rowsPicker    = $<HTMLSelectElement>("rows-picker");
const rowsAddBtn    = $<HTMLButtonElement>("rows-add");
const rowsTags      = $<HTMLDivElement>("rows-tags");

const colsPicker    = $<HTMLSelectElement>("cols-picker");
const colsAddBtn    = $<HTMLButtonElement>("cols-add");
const colsTags      = $<HTMLDivElement>("cols-tags");

const valuesPicker  = $<HTMLSelectElement>("values-picker");
const valuesAddBtn  = $<HTMLButtonElement>("values-add");
const valuesTags    = $<HTMLDivElement>("values-tags");

const chkRowSubtotals = $<HTMLInputElement>("chk-row-subtotals");
const chkGrandTotal   = $<HTMLInputElement>("chk-grand-total");

const refreshBtn    = $<HTMLButtonElement>("refresh-btn");
const cancelBtn     = $<HTMLButtonElement>("cancel-btn");
const statusBar     = $<HTMLDivElement>("status-bar");

// ── Init ─────────────────────────────────────────────────────────────────────

Office.onReady(() => {
  const saved = loadSettings();
  engineUrlInput.value = saved.engineUrl;

  if (USE_MOCK) {
    addBadge();
  }

  wireEvents();
  void loadModelList();
});

function addBadge(): void {
  const badge = document.createElement("span");
  badge.id = "mock-badge";
  badge.textContent = "MOCK";
  statusBar.parentElement?.insertBefore(badge, statusBar);
}

// ── Event wiring ──────────────────────────────────────────────────────────────

function wireEvents(): void {
  settingsBtn.addEventListener("click", () => {
    settingsPanel.classList.toggle("hidden");
  });

  testBtn.addEventListener("click", () => { void testConnection(); });

  engineUrlInput.addEventListener("change", () => {
    const url = engineUrlInput.value.trim();
    client = new EngineClient(url);
    void saveSettings({ engineUrl: url });
    setConnStatus("", "");
  });

  modelSelect.addEventListener("change", () => { void onModelSelected(); });
  loadModelBtn.addEventListener("click", () => { void loadModelList(); });

  rowsAddBtn.addEventListener("click", () => addRow());
  colsAddBtn.addEventListener("click", () => addCol());
  valuesAddBtn.addEventListener("click", () => addValue());

  chkRowSubtotals.addEventListener("change", () => {
    builder.setTotals({ row_subtotals: chkRowSubtotals.checked });
    markStale();
  });
  chkGrandTotal.addEventListener("change", () => {
    builder.setTotals({ grand_total: chkGrandTotal.checked });
    markStale();
  });

  refreshBtn.addEventListener("click", () => { void executeQuery(); });
  cancelBtn.addEventListener("click", () => { void cancelQuery(); });
}

// ── Connection ────────────────────────────────────────────────────────────────

async function testConnection(): Promise<void> {
  setConnStatus("Testing…", "");
  try {
    await client.health();
    setConnStatus("✓ Connected", "ok");
  } catch (e) {
    setConnStatus(`✗ ${errorMessage(e)}`, "fail");
  }
}

function setConnStatus(text: string, cls: "ok" | "fail" | ""): void {
  connStatus.textContent = text;
  connStatus.className = cls;
}

// ── Model loading ─────────────────────────────────────────────────────────────

async function loadModelList(): Promise<void> {
  setStatus("Loading models…", "");
  try {
    const models: ModelSummary[] = await client.listModels();
    populateModelSelect(models);
    setStatus("", "");

    const saved = loadSettings();
    if (saved.lastModelId && models.some((m) => m.model_id === saved.lastModelId)) {
      modelSelect.value = saved.lastModelId;
      await onModelSelected();
    }
  } catch (e) {
    setStatus(`Failed to load models: ${errorMessage(e)}`, "error");
  }
}

function populateModelSelect(models: ModelSummary[]): void {
  while (modelSelect.options.length > 1) modelSelect.remove(1);
  models.forEach((m) => {
    const opt = new Option(m.label ?? m.model_id, m.model_id);
    modelSelect.add(opt);
  });
}

async function onModelSelected(): Promise<void> {
  const id = modelSelect.value;
  if (!id) return;

  setStatus("Loading model…", "");
  try {
    const model: ModelMetadata = await client.getModel(id);
    builder.loadModel(model);
    await saveSettings({ lastModelId: id });
    renderZones();
    refreshBtn.disabled = false;
    setStatus("", "");
  } catch (e) {
    setStatus(`Failed to load model: ${errorMessage(e)}`, "error");
  }
}

// ── Zone rendering ────────────────────────────────────────────────────────────

function renderZones(): void {
  renderDimPicker(rowsPicker, "rows");
  renderDimPicker(colsPicker, "cols");
  renderMeasurePicker(valuesPicker);
  renderTags();
}

function renderDimPicker(picker: HTMLSelectElement, _zone: "rows" | "cols"): void {
  while (picker.options.length > 1) picker.remove(1);
  builder.getAvailableDimensions().forEach((dim) => {
    if (!builder.isDimensionUsed(dim.field)) {
      picker.add(new Option(dim.label ?? dim.field, dim.field));
    }
  });
}

function renderMeasurePicker(picker: HTMLSelectElement): void {
  while (picker.options.length > 1) picker.remove(1);
  builder.getAvailableMeasures().forEach((m) => {
    if (!builder.isMeasureUsed(m.id)) {
      picker.add(new Option(m.label ?? m.id, m.id));
    }
  });
}

function renderTags(): void {
  const state = builder.getState();

  rowsTags.innerHTML = "";
  state.rows.forEach((r) => {
    rowsTags.appendChild(makeDimTag(r.field, () => { builder.removeRow(r.field); renderZones(); markStale(); }));
  });

  colsTags.innerHTML = "";
  state.columns.forEach((c) => {
    colsTags.appendChild(makeDimTag(c.field, () => { builder.removeColumn(c.field); renderZones(); markStale(); }));
  });

  valuesTags.innerHTML = "";
  state.measures.forEach((m) => {
    const meta = builder.getAvailableMeasures().find((x) => x.id === m.id);
    valuesTags.appendChild(makeMeasureTag(m.id, m.agg ?? "sum", meta?.allowed_agg ?? ["sum"], () => {
      builder.removeMeasure(m.id);
      renderZones();
      markStale();
    }, (agg) => {
      builder.setMeasureAgg(m.id, agg);
      markStale();
    }));
  });
}

function makeDimTag(field: string, onRemove: () => void): HTMLElement {
  const tag = document.createElement("span");
  tag.className = "tag";
  tag.textContent = field + " ";
  const btn = document.createElement("button");
  btn.className = "remove-tag";
  btn.textContent = "✕";
  btn.addEventListener("click", onRemove);
  tag.appendChild(btn);
  return tag;
}

function makeMeasureTag(
  id: string,
  currentAgg: AggType,
  allowedAggs: AggType[],
  onRemove: () => void,
  onAggChange: (agg: AggType) => void,
): HTMLElement {
  const tag = document.createElement("span");
  tag.className = "tag";
  tag.textContent = id + ":";

  const sel = document.createElement("select");
  allowedAggs.forEach((a) => {
    const opt = new Option(aggLabel(a), a);
    if (a === currentAgg) opt.selected = true;
    sel.add(opt);
  });
  sel.addEventListener("change", () => onAggChange(sel.value as AggType));

  const btn = document.createElement("button");
  btn.className = "remove-tag";
  btn.textContent = "✕";
  btn.addEventListener("click", onRemove);

  tag.appendChild(sel);
  tag.appendChild(btn);
  return tag;
}

function aggLabel(agg: AggType): string {
  const map: Record<AggType, string> = {
    sum: "Sum", count: "Count", count_distinct: "Distinct",
    min: "Min", max: "Max", avg: "Avg",
  };
  return map[agg] ?? agg;
}

// ── Add field actions ─────────────────────────────────────────────────────────

function addRow(): void {
  const field = rowsPicker.value;
  if (!field) return;
  builder.addRow(field);
  rowsPicker.value = "";
  renderZones();
  markStale();
}

function addCol(): void {
  const field = colsPicker.value;
  if (!field) return;
  const dim = builder.getAvailableDimensions().find((d) => d.field === field);
  const granularity = dim?.type === "date" ? "month" : undefined;
  builder.addColumn(field, granularity);
  colsPicker.value = "";
  renderZones();
  markStale();
}

function addValue(): void {
  const id = valuesPicker.value;
  if (!id) return;
  const meta = builder.getAvailableMeasures().find((m) => m.id === id);
  if (!meta) return;
  builder.addMeasure(meta);
  valuesPicker.value = "";
  renderZones();
  markStale();
}

// ── Query execution ───────────────────────────────────────────────────────────

async function executeQuery(): Promise<void> {
  const errors = builder.validate();
  if (errors.length > 0) {
    setStatus(errors[0], "error");
    return;
  }

  const modelId = modelSelect.value;
  const request = builder.buildRequest(modelId);
  currentRequestId = request.request_id;
  isStale = false;

  refreshBtn.disabled = true;
  cancelBtn.disabled = false;
  setStatus("", "loading");

  const start = Date.now();

  try {
    const response: PivotResponse = await client.query(request);
    currentRequestId = null;

    await renderer.render(response);

    const elapsed = Date.now() - start;
    const cacheLabel = response.from_cache ? " · cache" : "";
    setStatus(
      `✓ ${response.row_axis.members.length} rows · ${response.stats.clickhouse_ms}ms CH${cacheLabel} · ${elapsed}ms total`,
      "ok",
    );
  } catch (e) {
    currentRequestId = null;
    if (e instanceof EngineClientError && e.code === "CANCELLED") {
      setStatus("Cancelled", "warn");
    } else {
      setStatus(errorMessage(e), "error");
    }
  } finally {
    refreshBtn.disabled = false;
    cancelBtn.disabled = true;
  }
}

async function cancelQuery(): Promise<void> {
  if (!currentRequestId) return;
  const id = currentRequestId;
  currentRequestId = null;
  try {
    await client.cancel(id);
  } catch {
    // best-effort
  }
  cancelBtn.disabled = true;
  setStatus("Cancelling…", "warn");
}

// ── Status helpers ────────────────────────────────────────────────────────────

function setStatus(text: string, state: "ok" | "error" | "warn" | "loading" | ""): void {
  statusBar.innerHTML = "";
  statusBar.className = state;
  if (state === "loading") {
    const spinner = document.createElement("span");
    spinner.className = "spinner";
    statusBar.appendChild(spinner);
    if (text) statusBar.append(" " + text);
  } else {
    statusBar.textContent = text;
  }
}

function markStale(): void {
  if (!isStale && statusBar.className === "ok") {
    isStale = true;
    const badge = document.createElement("span");
    badge.className = "stale-badge";
    badge.textContent = "stale";
    statusBar.appendChild(badge);
  }
}

function errorMessage(e: unknown): string {
  if (e instanceof EngineClientError) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
