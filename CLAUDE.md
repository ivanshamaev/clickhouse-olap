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

## Development Environment Policy

**All development is done in the local project clone.** Install tools, runtimes, and dependencies locally — do not rely on global or system-wide installs where a local alternative exists. Specifically:
- Rust toolchain via `rustup` (manages per-project toolchain via `rust-toolchain.toml`).
- Node.js version via `.nvmrc` or `engines` field in `package.json`; use `nvm use` or `corepack` where applicable.
- ClickHouse runs in Docker (not a shared remote) so integration tests are fully self-contained.
- `office-addin-dev-certs` certificates are installed per-user, not system-wide.

Do not add global `npm install -g` steps to scripts or CI — use `npx` or local `./node_modules/.bin/` paths instead.

---

## Local Development Setup

### Prerequisites

- **Rust** — install via [rustup](https://rustup.rs/), stable toolchain.
- **Node.js ≥ 18 + npm** — for the Office.js add-in.
- **Docker + Docker Compose** — for the local ClickHouse instance.
- **MS Excel** (Windows or macOS desktop) — required to sideload and test the add-in. Excel on the web can be used as an alternative via a localhost HTTPS manifest.
- **Office Add-ins dev certificate** — install once with `npx office-addin-dev-certs install` (needed because Office requires HTTPS even for localhost).

### 1. Start ClickHouse locally

```bash
docker compose -f engine/docker/docker-compose.dev.yml up -d
# ClickHouse will be available at localhost:8123 (HTTP) and localhost:9000 (native)
```

Seed the test dataset used by integration tests:

```bash
engine/scripts/seed_test_data.sh
```

### 2. Run the Rust engine

```bash
cp engine/config/config.example.toml engine/config/config.local.toml
# Edit config.local.toml: set clickhouse.url, port, semantic model path
cargo run --manifest-path engine/Cargo.toml -- --config engine/config/config.local.toml
# Engine listens on http://127.0.0.1:3000 by default (local mode, loopback only)
```

### 3. Build and sideload the add-in

```bash
cd addin
npm install
npm run build:dev          # development build with source maps
npm run start              # starts local HTTPS dev server on https://localhost:3100
```

Sideload into Excel:
- **Windows/macOS desktop:** In Excel → Insert → Add-ins → Upload My Add-in → select `addin/manifest.xml`.
- **Excel on the web:** Use the [sideloading guide](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing).

In the add-in connection settings, set the engine endpoint to `http://127.0.0.1:3000`.

### Commands

**Engine (Rust):**
```
cargo build                            # build
cargo test                             # all tests (unit + integration)
cargo test <test_name>                 # single test
cargo clippy -- -D warnings            # lint
cargo test --test integration -- --ignored   # integration tests (requires local ClickHouse)
```

**Add-in (TypeScript + Office.js):**
```
npm install          # install dependencies
npm run build:dev    # development build
npm run build        # production build
npm test             # unit tests
npm run lint         # ESLint
```

---

## CI/CD and GitHub Actions

### Workflow structure

```
.github/workflows/
  ci.yml          # runs on every PR: lint + unit tests + integration tests
  release.yml     # runs on tag push (v*.*.*): builds release artifacts and publishes a GitHub Release
```

### `ci.yml` — PR checks

Triggers on `push` to any branch and `pull_request` to `main`.

Steps:
1. `cargo clippy -- -D warnings` and `cargo fmt --check` on the engine.
2. `npm run lint` and `npm test` on the add-in.
3. Start ClickHouse via `docker compose` service container, run `cargo test --test integration`.

### `release.yml` — Release pipeline

Triggers on tag push matching `v*.*.*` (e.g., `v0.3.0`). Tag on `main` only.

Steps:
1. **Build engine binaries** via `cross` for three targets:
   - `x86_64-unknown-linux-gnu` (server deployment)
   - `x86_64-pc-windows-gnu`
   - `x86_64-apple-darwin` + `aarch64-apple-darwin` (universal macOS via `lipo`)
2. **Build add-in** — `npm run build` (production, minified). Output: `addin/dist/`.
3. **Package**:
   - Engine: `clickhouse-olap-engine-<version>-<target>.tar.gz` (binary + `config.example.toml`).
   - Add-in: `clickhouse-olap-addin-<version>.zip` (contents of `addin/dist/` + `manifest.xml`).
4. **Create GitHub Release** with auto-generated changelog from conventional commits; attach all archives as release assets.

### Versioning

Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, etc.). Tags are created manually by the maintainer after review; CI does not auto-tag.

The API version in the contract (`/v1`, `/v2`) is independent of the release version — bump it only on breaking contract changes (NFR-COMPAT-03).

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
