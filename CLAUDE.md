# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

OLAP analysis of ClickHouse data in MS Excel via a Rust engine. A thin Office.js add-in lets business users build pivot tables; the Rust engine translates the request into aggregating SQL, sends it to ClickHouse, and shapes the compact pre-aggregate into a pivot result. No raw data leaves ClickHouse; no calculations happen in the add-in.

Source of truth for all requirements: `project_requirements/` (documents `00`–`04`). Always cross-reference FR-*/NFR-* identifiers when implementing features.

---

## Architectural Invariants — NEVER violate these

1. **Aggregation only in ClickHouse (D1).** Raw data (hundreds of millions of rows) is never pulled into Rust for aggregation. Any code that fetches raw rows into Rust for aggregation is wrong.
2. **Rust engine is stateless (FR-ENGINE-01).** No per-user session state between requests (cache excepted). This enables horizontal scaling.
3. **Semantic model is the only allowed access path (FR-QUERY-04).** Only fields and tables declared in the semantic model may be queried. Arbitrary SQL from the client is forbidden.
4. **All untrusted values must be parameterized (FR-QUERY-03).** String-concatenating untrusted values into SQL is forbidden — it is a SQL injection vulnerability.
5. **Cardinality control before execution (FR-QUERY-05, R2).** Requests that would produce a non-compact pre-aggregate (e.g., GROUP BY on a high-cardinality field) must be rejected with `PREAGGREGATE_TOO_LARGE` *before* the heavy query is sent to ClickHouse.
6. **Non-additive totals come from ClickHouse (FR-PIVOT-04b, R5).** `count distinct`, percentiles, and `avg` totals must be computed by ClickHouse at the appropriate aggregation level (via `WITH ROLLUP`, `GROUPING SETS`, or separate queries). Summing subtotals in Rust for these measures is always wrong.
7. **API contract is versioned and stable (NFR-COMPAT-03).** Breaking changes require a new API version (`/v2`). The contract schema in `docs/contract/` is the single source of truth for both the engine and the add-in.
8. **Thin client (D4).** The Office.js add-in only serializes the user's intent into a structured request and renders the returned result. No aggregation, no recalculation in the add-in.

---

## Repository Structure

```
engine/          # Rust engine — stateless pivot shaper
addin/           # Office.js add-in (TypeScript, MS Excel only)
docs/contract/   # Versioned JSON schemas for the API contract (single source of truth)
project_requirements/  # Requirements documents 00–04 (read-only source of truth)
.claude/
  skills/        # Loaded-on-demand skill docs (clickhouse-sql, pivot-shaping, etc.)
  agents/        # Subagent definitions (code-reviewer, invariant-checker, researcher)
```

---

## Commands

> The engine and add-in directories do not exist yet (Phase 0 of the dev plan). Commands below are the intended targets; update this section once the skeleton is created.

**Engine (Rust):**
```
cargo build                  # build
cargo test                   # all tests (unit + integration)
cargo test <test_name>       # single test
cargo clippy -- -D warnings  # lint
```

**Add-in (TypeScript + Office.js):**
```
npm install    # install dependencies
npm run build  # build
npm test       # run tests
```

**ClickHouse (integration tests):**
Integration tests run against ClickHouse in Docker. See `engine/tests/` for setup once Phase 3 is implemented.

---

## API Contract (doc 03)

The contract lives at `docs/contract/`. Key operations:

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/health` | Health check |
| `GET /v1/models` | List semantic models |
| `GET /v1/models/{id}` | Model metadata (dimensions, measures, guards) |
| `POST /v1/query` | Execute pivot request → returns shaped pivot result |
| `POST /v1/query/{id}/cancel` | Cancel in-flight request |
| `POST /v1/drillthrough` | Detail rows (hard row limit, optional) |

Transport: HTTP/JSON (gRPC optional). Local mode uses loopback; server mode requires TLS.

---

## High-Risk Areas — Prioritize Tests Here

- **Non-additive totals (FR-PIVOT-04b):** Verify that `count distinct`, `avg`, and percentile totals come from ClickHouse, not from summing subtotals. Golden tests must confirm total ≠ sum-of-subtotals where applicable.
- **Cardinality control (FR-QUERY-05, FR-SEM-04):** Reject high-cardinality GROUP BY before touching ClickHouse.
- **SQL generation determinism (NFR-MAINT-02):** Golden SQL tests for every request shape — regressions in generated SQL are silent bugs.

---

## Workflow

- Use subagents for code review (`invariant-checker`, `code-reviewer`) and research (`researcher`) — keeps the main context uncontaminated.
- Run `/clear` before starting an unrelated task.
- Every new feature needs tests. Integration tests run against a real ClickHouse instance (no mocks for the DB layer).
- When implementing totals for non-additive measures, always consult `pivot-shaping` skill and FR-PIVOT-04b.

---

## Context Compaction Instructions

When context is compacted, the following must be preserved:
1. All 8 architectural invariants from this file.
2. The current API version and any in-flight contract changes.
3. The development phase currently in progress and the files touched.
4. Any open architectural decisions (R1–R6 from `project_requirements/00_overview_vision.md`).
