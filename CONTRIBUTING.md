# Contributing to ClickHouse OLAP

Thank you for your interest in the project! This guide explains how to set up the development environment, submit changes, and follow the conventions used in the codebase.

---

## Table of contents

1. [Code of conduct](#code-of-conduct)
2. [Ways to contribute](#ways-to-contribute)
3. [Development setup](#development-setup)
4. [Project structure](#project-structure)
5. [Workflow](#workflow)
6. [Commit conventions](#commit-conventions)
7. [Coding standards](#coding-standards)
8. [Tests](#tests)
9. [Architectural invariants](#architectural-invariants)
10. [Submitting a pull request](#submitting-a-pull-request)
11. [Reporting bugs](#reporting-bugs)

---

## Code of conduct

Be respectful. Constructive criticism of code is welcome; personal attacks are not. Issues and pull requests that violate this will be closed.

---

## Ways to contribute

| What | Where to start |
|------|---------------|
| Bug reports | Open a [GitHub issue](https://github.com/ivanshamaev/clickhouse-olap/issues) with the **bug** label |
| Feature requests | Open an issue with the **enhancement** label; discuss before coding |
| Documentation fixes | Edit files in `site/` or `docs/` and open a PR |
| Engine (Rust) improvements | See `engine/` — read the invariants below first |
| Add-in (TypeScript) improvements | See `addin/` |
| New semantic model examples | Add a `.toml` file under `engine/config/models/` |

---

## Development setup

### Prerequisites

| Tool | Purpose | How to install |
|------|---------|----------------|
| Rust stable (≥ 1.80) | Engine | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js ≥ 18 + npm | Add-in | [nodejs.org](https://nodejs.org) or `nvm` |
| Docker + Docker Compose | Local ClickHouse | [docs.docker.com](https://docs.docker.com/get-docker/) |
| Git | VCS | [git-scm.com](https://git-scm.com) |

### 1 — Clone and enter the repo

```bash
git clone https://github.com/ivanshamaev/clickhouse-olap.git
cd clickhouse-olap
```

### 2 — Start local ClickHouse

```bash
docker compose -f engine/docker/docker-compose.dev.yml up -d
```

Seed the test dataset (required for integration tests):

```bash
engine/scripts/seed_test_data.sh
```

### 3 — Run the Rust engine

```bash
cp engine/config/config.example.toml engine/config/config.local.toml
# Edit config.local.toml if needed (default points to localhost:8123, no auth)
cargo run --manifest-path engine/Cargo.toml -- --config engine/config/config.local.toml
```

Engine listens at `http://127.0.0.1:3000`. Quick smoke-test:

```bash
curl http://127.0.0.1:3000/v1/health
# → {"status":"ok"}
```

### 4 — Build and run the add-in

```bash
cd addin
npm ci
npm run build:dev        # development build with source maps
npm start                # HTTPS dev server on https://localhost:3100
```

Sideload `addin/manifest.xml` into Excel (see [docs/addin guide](site/docs/addin.html)).

---

## Project structure

```
engine/          # Rust — stateless pivot shaper (axum HTTP server)
  src/
    api/         # Request/response types + axum handlers
    clickhouse/  # HTTP client for ClickHouse (JSONEachRow)
    query/       # SQL planner and generator
    pivot/       # Result shaper (rows → pivot matrix)
    semantic/    # Semantic model loader and validator
    cache.rs     # In-memory result cache (moka)
    config.rs    # Config struct (TOML)
    error.rs     # EngineError → HTTP response mapping
  config/
    models/      # Semantic model TOML files (one per dataset)

addin/           # TypeScript — Office.js Excel add-in
  src/
    api/         # Engine HTTP client + contract types
    components/  # PivotBuilder, ResultRenderer
    taskpane/    # Entry point, HTML template
    utils/       # Settings, UUID helpers

site/            # Static documentation site (GitHub Pages)
  index.html
  site.css
  docs/

docs/            # Markdown docs (deployment guides, API contract)
  contract/      # Versioned JSON schemas — source of truth for the API

project_requirements/   # Requirements documents 00–04 (read-only)
```

---

## Workflow

1. **Create an issue** (or pick an existing one) — confirm the change is wanted before investing time.
2. **Fork** the repository and clone your fork locally.
3. **Create a feature branch** off `main`:
   ```bash
   git checkout -b feat/my-feature
   ```
4. Make your changes; keep commits small and focused.
5. Run all checks locally (see [Tests](#tests)).
6. Push and **open a pull request** against `main`.

> **Do not push directly to `main`.** All changes go through PRs.

---

## Commit conventions

This project follows [Conventional Commits](https://www.conventionalcommits.org/).

```
<type>(<scope>): <short description>

[optional body]

[optional footer]
```

### Types

| Type | When to use |
|------|-------------|
| `feat` | New feature visible to users |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code restructuring, no behaviour change |
| `test` | Adding or fixing tests |
| `docs` | Documentation only |
| `chore` | Build scripts, CI, tooling |
| `ci` | GitHub Actions changes |

### Scopes (optional but helpful)

`engine`, `addin`, `query`, `pivot`, `semantic`, `clickhouse`, `site`, `ci`

### Examples

```
feat(engine): add date granularity support to column axis
fix(pivot): count_distinct totals now come from ClickHouse (FR-PIVOT-04b)
docs(site): add nginx TLS configuration example
chore(ci): cache cargo registry between jobs
```

---

## Coding standards

### Rust (engine)

- `cargo fmt` before every commit — CI enforces this with `cargo fmt --check`.
- `cargo clippy -- -D warnings` must pass — no `#[allow(clippy::...)]` without a comment explaining why.
- **No panics in request-handling code.** Use `Result` and propagate errors to `EngineError`.
- **No raw SQL string concatenation with untrusted values.** Use ClickHouse HTTP parameters (`{p0:Type}` in SQL, `param_p0=value` in URL). This is an architectural invariant — see below.
- Prefer `thiserror`-derived errors over `anyhow` in library code; `anyhow` is fine in `main.rs` for startup.
- New public types should have doc comments (`///`).

### TypeScript (add-in)

- `npm run lint` must pass — ESLint with the project config.
- Strict TypeScript — no `any` without a `// eslint-disable` comment explaining why.
- Keep the add-in **thin**: no aggregation logic, no recalculation. The add-in serialises intent and renders results; the engine does everything else.
- Use the shared `EngineClient` class for all engine communication — do not call `fetch` directly in components.

### Documentation (site/)

- Plain HTML + the shared `site.css` — no frameworks, no build step.
- Keep relative links (`../index.html`, `../site.css`) so pages work both on GitHub Pages and locally.
- Code blocks use `<pre><code>` — do not use JavaScript syntax highlighters.

---

## Tests

### Engine

```bash
# Unit tests (no external dependencies)
cargo test

# Lint
cargo clippy -- -D warnings
cargo fmt --check

# Integration tests (requires local ClickHouse on :8123)
cargo test --test integration -- --ignored
```

**Golden SQL tests** (`engine/tests/sql_golden/`) verify the exact SQL emitted for each query shape — regressions in generated SQL are silent bugs. Add a golden test for any new SQL generation path.

### Add-in

```bash
cd addin
npm test               # Jest unit tests
npm run lint           # ESLint
npm run build          # Must compile without errors
```

### CI

The full CI pipeline runs automatically on every push and pull request. Merging is blocked until all jobs pass:

- `engine` — fmt, clippy, `cargo test`
- `addin` — lint, jest, webpack build

---

## Architectural invariants

These rules reflect fundamental design decisions. A PR that violates them will not be merged regardless of how well it is coded.

| # | Rule | Why |
|---|------|-----|
| **D1** | Aggregation happens only in ClickHouse. Rust never fetches raw rows for aggregation. | Raw tables can have hundreds of millions of rows. |
| **FR-ENGINE-01** | The Rust engine is stateless between requests (cache excepted). | Enables horizontal scaling. |
| **FR-QUERY-03** | All untrusted filter values are parameterised. No string-concatenation into SQL. | SQL injection prevention. |
| **FR-QUERY-04** | Only fields declared in the semantic model may be queried. | Prevents data exfiltration. |
| **FR-QUERY-05** | High-cardinality GROUP BY is rejected *before* the query reaches ClickHouse. | Prevents accidental billion-group queries. |
| **FR-PIVOT-04b** | Non-additive totals (`count_distinct`, `avg`, percentiles) come from ClickHouse at the correct aggregation level — never summed from subtotals in Rust. | Summing `uniq()` results is mathematically wrong. |
| **NFR-COMPAT-03** | The `/v1` API contract is stable. Breaking changes require a new version (`/v2`). | Deployed add-ins must keep working. |
| **D4** | The add-in is thin: no aggregation, no recalculation. | Security and correctness. |

If you are uncertain whether your change touches an invariant, open an issue first.

---

## Submitting a pull request

### Checklist before opening a PR

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes (including any new tests you added)
- [ ] `npm run lint` passes
- [ ] `npm test` passes
- [ ] `npm run build` succeeds
- [ ] No architectural invariants are violated
- [ ] New behaviour is covered by tests
- [ ] If the change affects the API contract (`docs/contract/`), the schema is updated

### PR description template

```
## What
<one paragraph describing what the change does>

## Why
<link to the issue this closes, or explain the motivation>

## How
<brief explanation of the approach; call out non-obvious decisions>

## Testing
<how you tested this; paste relevant test output if useful>
```

### Review process

- PRs are reviewed by the maintainer within a few days.
- Reviewers may request changes — please address them or explain why you disagree.
- Once approved and CI is green, the maintainer will merge.

---

## Reporting bugs

Open a [GitHub issue](https://github.com/ivanshamaev/clickhouse-olap/issues) and include:

1. **Steps to reproduce** — the exact request (JSON body) or user actions.
2. **Expected behaviour** — what should have happened.
3. **Actual behaviour** — what happened instead (include error messages, logs).
4. **Environment** — engine version, ClickHouse version, OS, Excel version if relevant.

For security vulnerabilities, please **do not** open a public issue — email [ivan.shamaev@gmail.com](mailto:ivan.shamaev@gmail.com) instead.

---

## Questions?

Open a [discussion](https://github.com/ivanshamaev/clickhouse-olap/discussions) or an issue with the **question** label. You can also reach the maintainer at [ivan-shamaev.ru/cv](https://ivan-shamaev.ru/cv).
