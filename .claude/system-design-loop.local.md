---
active: true
target: /home/paulo/Projetos/usetheo/theo-data/theo-db/theodb_rs
scope: ''
current_phase: 3
phase_name: data_flow
phase_iteration: 3
global_iteration: 10
max_global_iterations: 80
completion_promise: SYSTEM DESIGN AUDIT COMPLETE
started_at: '2026-07-16T15:23:51Z'
output_dir: /home/paulo/Projetos/usetheo/theo-data/theo-db/system-design-output
db_path: /home/paulo/Projetos/usetheo/theo-data/theo-db/system-design-output/system-design.db
mode: full
severity_threshold: medium
modules_total: 0
boundary_findings_total: 0
data_flows_total: 0
state_findings_total: 0
scaling_findings_total: 0
deletion_findings_total: 0
tradeoff_decisions_total: 0
system_design_findings_total: 0
findings_critical: 0
findings_high: 0
---

# Staff-Level System Design Audit

You are running an autonomous system design audit. Apply distributed system
principles at the **codebase level** — not infrastructure. Think like a Staff/
Principal engineer evaluating whether this codebase is designed for scale,
maintainability, safe deletion, and pragmatic trade-offs.

## Engagement parameters

- **Target:** `/home/paulo/Projetos/usetheo/theo-data/theo-db/theodb_rs`
- **Scope:** ``
- **Mode:** `full`
- **Output directory:** `/home/paulo/Projetos/usetheo/theo-data/theo-db/system-design-output`
- **Database:** `/home/paulo/Projetos/usetheo/theo-data/theo-db/system-design-output/system-design.db`
- **Severity threshold:** `medium`
- **Completion promise:** `SYSTEM DESIGN AUDIT COMPLETE`
- **Max iterations:** `80`

## Tool availability

| Ecosystem | Tool | Status | Use for |
|---|---|---|---|
| Python | pydeps | present | Dependency graph visualization |
| Python | import-linter | present | Contract-based import enforcement |
| Python | tach | absent | AST boundary enforcement (Rust-powered) |
| TS/JS | madge | present | Circular dependency detection |
| TS/JS | skott | absent | Dependency graph + circular deps |
| TS/JS | dependency-cruiser | present | Rule-based dependency validation |
| TS/JS | fallow | absent | Composite health score (0-100) + boundary presets |
| Go | goda | absent | Package dependency analysis |
| Rust | cargo-modules | absent | Module dependency tree |
| Rust | cargo-coupling | absent | Khononov 3D coupling analysis (S-F grade) |
| Cross | scc | present | LOC + complexity counting |
| Cross | tokei | absent | LOC counting |
| Cross | piranha | absent | Stale feature flag cleanup (Uber) |

## The 5 dimensions of Staff System Design

| # | Dimension | What to look for |
|---|---|---|
| 1 | **Boundaries** | DDD bounded contexts, modular monolith hygiene, cross-boundary imports, god modules, interface contracts |
| 2 | **Data Flow & State** | Synchronous vs async paths, queue usage, cache strategy, state mutation patterns, backpressure, circuit breakers |
| 3 | **Scaling Readiness** | Vertical-first mindset, N+1 queries, connection pooling, pagination, memory efficiency, rate limiting, blocking I/O in hot paths |
| 4 | **Deletion Safety** | Feature flag hygiene, deprecation markers, module excisability, dead paths, zombie dependencies |
| 5 | **Trade-offs & Pragmatism** | Documented decisions, YAGNI adherence, premature complexity, consistency model choices, over-engineering |

## Mode contract

| Mode | Phases executed | Skipped |
|---|---|---|
| full | 1, 2, 3, 4, 5, 6 | none |
| boundaries | 1, 2, 6 | 3, 4, 5 |
| data-flow | 1, 3, 6 | 2, 4, 5 |
| scaling | 1, 4, 6 | 2, 3, 5 |
| deletion | 1, 5, 6 | 2, 3, 4 |
| tradeoffs | 1, 5, 6 | 2, 3, 4 |

## Operating rules

1. **Sub-agents per phase.** Each phase is delegated to a specialist agent:
   chief-system-designer (Phase 1), boundary-analyst (Phase 2),
   data-flow-analyst (Phase 3), scaling-auditor (Phase 4),
   deletion-readiness-auditor + tradeoff-analyst (Phase 5),
   quality-evaluator (gates), report-writer (Phase 6).

2. **Database is source of truth.** Every finding MUST be persisted via the
   database CLI before advancing. No finding lives only in markdown.

3. **Structured-column rule.** `add-boundary-finding`, `add-state-finding`,
   `add-scaling-finding`, `add-deletion-finding` MUST include `file` AND
   `line` as separate JSON keys. NULL values fail the quality gate.

4. **Honest threshold sourcing.** When citing thresholds, mark the source:
   - `consensus` — N+1 query is always a bug; circuit breaker for external calls (Nygard); connection pooling mandatory
   - `default` — Feature flag max age 30 days (LaunchDarkly); queue for spikes (AWS Well-Architected)
   - `heuristic` — Module LOC ≤ 2000; sync chain depth ≤ 3; cross-boundary ratio ≤ 30%

5. **Vertical before horizontal.** Flag premature distribution — microservice
   split, distributed cache, or message broker introduced before the monolith
   hit a measurable bottleneck.

6. **Markers from DB queries.** The chief-system-designer emits phase markers
   by querying the database, NEVER from sub-agent text.

7. **Staff-level pragmatism.** Not every finding is a problem. A codebase that
   uses global state for a CLI tool's config is fine. Context matters. Every
   finding must pass the "would a Staff engineer at a FAANG company actually
   flag this in a design review?" test.

8. **Crying Wolf prevention.** Research shows practitioners ignore all warnings
   after too many false positives (Li, Liang, Avgeriou — ACM TOSEM 2025).
   Every finding MUST explain WHY it matters, not just WHERE it is. Prefer
   fewer high-confidence findings over many speculative ones.

## Known ecosystem tools (use when available)

- **Python boundaries:** Tach (AST-based import enforcement), import-linter (contract-based)
- **TS/JS boundaries:** Fallow (composite health score 0-100, architecture presets), madge, dependency-cruiser
- **Rust boundaries:** cargo-coupling (Khononov 3D coupling, S-F grading), cargo-modules
- **Feature flags:** Uber Piranha (polyglot stale flag cleanup), grep-based detection
- **Dead code:** Knip (JS/TS unused files/exports/deps, 155+ framework plugins)

## Database CLI reference

```bash
DB="/home/paulo/Projetos/usetheo/theo-data/theo-db/system-design-output/system-design.db"
DB_CLI="${CLAUDE_PLUGIN_ROOT}/scripts/system_design_database.py"

python3 "$DB_CLI" --db-path "$DB" add-module --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-boundary-finding --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-data-flow --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-state-finding --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-scaling-finding --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-deletion-finding --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-tradeoff-decision --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-system-design-finding --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-meeting --json '{...}'
python3 "$DB_CLI" --db-path "$DB" add-quality-gate --json '{...}'
python3 "$DB_CLI" --db-path "$DB" count --table TABLE [--where CLAUSE --params '[...]']
python3 "$DB_CLI" --db-path "$DB" coverage-stats
python3 "$DB_CLI" --db-path "$DB" scoring-summary
python3 "$DB_CLI" --db-path "$DB" thresholds
```

---

## Phase guide

### Phase 1 — Baseline (max 3 iterations)

**Goal:** Exhaustive inventory of modules, bounded contexts, and entry points.

**Sub-phase 1a — File inventory:**
- Enumerate every source file via `find` (Python, TypeScript, JavaScript, Go, Rust, YAML, JSON, Shell)
- Register in `files_inventoried` with: path, LOC, language, is_test, is_excluded
- Exclude: node_modules, vendor, __pycache__, dist, build, .git

**Sub-phase 1b — Module identification:**
- Identify top-level modules/packages/bounded contexts
- Register with: name, kind, path, language, LOC, file_count, public_api_size
- Tag `domain_tag` (billing, auth, orders, etc.)
- Tag `boundary_type`: bounded_context, shared_kernel, anti_corruption_layer, generic_subdomain
- Identify entry points (main files, API routers, CLI handlers)

**Sub-phase 1c — Manifests & frameworks:**
- Read package.json, go.mod, pyproject.toml, Cargo.toml
- Detect frameworks (Django, FastAPI, Express, Gin, Axum, etc.)
- Note: framework detection informs later phases about conventions

**Advance criterion:** `modules` ≥ 1 AND `files_inventoried` ≥ 1

**Markers:**
```
<!-- PHASE_1_COMPLETE -->
<!-- MODULES_TOTAL:N -->
<!-- FILES_INVENTORIED:N -->
```

---

### Phase 2 — Boundaries & DDD (max 4 iterations)

**Goal:** Audit module boundaries as a Staff engineer would in a design review.

**What to detect:**

| Category | Severity | What | How |
|---|---|---|---|
| `cross_boundary_import` | high | Module A imports Module B's internal symbols | grep for `_internal`, `_private`, non-public paths |
| `shared_mutable_state` | high | Global mutable shared between bounded contexts | grep for module-level dicts/lists mutated by imports |
| `circular_dependency` | critical | Module A ↔ Module B import cycle | pydeps/madge/dependency-cruiser or grep fallback |
| `god_module` | high | Module with >5 domain responsibilities | Check domain_tag diversity, LOC > 2000, public_api > 20 |
| `leaky_abstraction` | medium | Internal implementation details exposed in public API | Public functions returning internal types |
| `missing_interface` | medium | Direct concrete dependency where interface would help | High-level module importing low-level concrete |
| `tight_coupling` | high | Cross-boundary coupling ratio > 30% | Count cross-domain imports / total imports |

**Boundary analysis protocol:**
1. For each pair of modules with different `domain_tag`, count imports between them
2. Compute cross-boundary ratio = cross-boundary imports / total imports per module
3. Flag modules where ratio > 0.30 (heuristic)
4. Identify missing anti-corruption layers between bounded contexts

**Markers:**
```
<!-- PHASE_2_COMPLETE -->
<!-- BOUNDARY_FINDINGS_TOTAL:N -->
<!-- QUALITY_SCORE:X.XX -->
<!-- QUALITY_PASSED:1 -->
```

---

### Phase 3 — Data Flow & State Management (max 4 iterations)

**Goal:** Map data movement paths and detect state management anti-patterns.

**Sub-phase 3a — Flow mapping:**
- Trace data from entry point (HTTP handler, CLI command, queue consumer) to persistence
- Register each flow in `data_flows` with: source, sink, flow_type, synchronous/async flags
- Classify: http_sync, http_async, queue, event_bus, direct_call, file_io, db_query

**Sub-phase 3b — Resilience audit per flow:**

| Check | Severity | Condition |
|---|---|---|
| Missing circuit breaker | high (consensus) | External service call without circuit breaker pattern |
| Missing rate limiter | medium | Public endpoint without rate limiting |
| Missing backpressure | high | Queue consumer without backpressure mechanism |
| Missing retry with backoff | medium | External call with naive retry or no retry |
| Sync chain too deep | medium (heuristic) | >3 synchronous hops before response |

**Sub-phase 3c — State management:**

| Category | Severity | What |
|---|---|---|
| `global_mutable_state` | high | Module-level mutable state shared across requests |
| `missing_queue` | medium | Spike-prone endpoint without queue decoupling |
| `missing_cache` | low | Repeated expensive computation without caching |
| `cache_invalidation_risk` | medium | Cache without TTL or explicit invalidation |
| `unbounded_collection` | high | In-memory collection that grows without limit |
| `missing_backpressure` | high | Producer faster than consumer without flow control |

**Markers:**
```
<!-- PHASE_3_COMPLETE -->
<!-- DATA_FLOWS_TOTAL:N -->
<!-- STATE_FINDINGS_TOTAL:N -->
<!-- QUALITY_SCORE:X.XX -->
<!-- QUALITY_PASSED:1 -->
```

---

### Phase 4 — Scaling Readiness (max 4 iterations)

**Goal:** Evaluate whether the codebase scales vertically before needing horizontal split.

**Core principle: Vertical before Horizontal.** A well-designed monolith on
a single node can serve surprisingly high traffic. Flag premature distribution.

| Category | Severity | What to detect |
|---|---|---|
| `n_plus_one_query` | critical (consensus) | Loop that queries DB per item instead of batch |
| `missing_pagination` | high | List endpoint returning unbounded results |
| `blocking_io_in_hot_path` | high | Synchronous I/O in request-handling hot path |
| `missing_connection_pool` | high (consensus) | DB connections opened per request |
| `memory_inefficiency` | medium | Loading full dataset when streaming would work |
| `missing_rate_limiter` | medium | Public API without rate limiting |
| `missing_index` | high | Frequent query pattern without supporting index |
| `unbounded_query` | high | Query without LIMIT that can return millions of rows |
| `premature_distribution` | medium | Microservice/distributed cache/message broker without measured bottleneck |

**What Staff engineers look for:**
1. Can a single well-provisioned node handle 10x current load?
2. Are database queries optimized (indexes, batch, pagination)?
3. Is I/O non-blocking in the request path?
4. Are there connection pools for external resources?
5. Is there evidence of profiling/benchmarking before optimization?

**Markers:**
```
<!-- PHASE_4_COMPLETE -->
<!-- SCALING_FINDINGS_TOTAL:N -->
<!-- QUALITY_SCORE:X.XX -->
<!-- QUALITY_PASSED:1 -->
```

---

### Phase 5 — Deletion Safety & Trade-offs (max 4 iterations)

**Goal:** Two sub-dimensions in one phase — deletion readiness and trade-off
documentation.

**Sub-phase 5a — Deletion safety:**

| Category | Severity | What |
|---|---|---|
| `stale_feature_flag` | medium | Feature flag older than 30 days (default) still active |
| `dead_feature_path` | medium | Code path behind a permanently-off flag |
| `missing_deprecation_marker` | low | Deprecated behavior without @deprecated or equivalent |
| `tangled_module` | high | Module that cannot be removed without cascading changes to >5 other modules |
| `missing_feature_toggle` | medium | New feature deployed without a kill switch |
| `zombie_dependency` | medium | Dependency imported but only used in dead/deprecated code |

**How to detect:**
- Grep for feature flag patterns: `isEnabled("...")`, `feature_flags.get("...")`, `process.env.FEATURE_*`, `@feature_flag`
- Check git log for flag age (first commit introducing the flag)
- Compute module excisability: if removing module M requires changes in N other modules, and N > 5, flag as `tangled_module`
- Scan for `@deprecated`, `# TODO: remove`, `// DEPRECATED` markers

**Sub-phase 5b — Trade-off analysis:**

For each architectural decision detected or implied:
1. Is it documented? (ADR, comment, README section)
2. What was chosen and what was rejected?
3. Is the rationale still valid?
4. Would a Staff engineer agree with the trade-off given current scale?

Key trade-off dimensions to evaluate:
- `consistency_model` — strong vs eventual consistency
- `sync_vs_async` — synchronous call chains vs event-driven
- `monolith_vs_microservice` — current decomposition level
- `sql_vs_nosql` — data store choice
- `cache_strategy` — what's cached, TTL, invalidation
- `error_handling_strategy` — fail-fast vs retry vs fallback
- `complexity_vs_simplicity` — YAGNI compliance

Register each as `tradeoff_decisions` with `is_documented` flag. Undocumented
decisions with high impact get `suggests_adr=1`.

**Markers:**
```
<!-- PHASE_5_COMPLETE -->
<!-- DELETION_FINDINGS_TOTAL:N -->
<!-- TRADEOFF_DECISIONS_TOTAL:N -->
<!-- QUALITY_SCORE:X.XX -->
<!-- QUALITY_PASSED:1 -->
```

---

### Phase 6 — Report (max 2 iterations)

**Goal:** Consolidate all findings into a Staff-level system design report.

**Output:** `/home/paulo/Projetos/usetheo/theo-data/theo-db/system-design-output/final_report.md`

**Required sections:**

1. **Executive summary** — top 3 system design risks, overall health assessment
2. **Scope & methodology** — what was analyzed, which modes ran, quality gate history
3. **Module inventory** — table of modules with domain_tag, boundary_type, LOC
4. **Findings by severity** — critical → low, with file:line evidence
5. **Findings by dimension:**
   - 5a. Boundaries — boundary violations, coupling analysis
   - 5b. Data Flow & State — flow map, resilience gaps, state issues
   - 5c. Scaling Readiness — vertical capacity, query optimization, I/O patterns
   - 5d. Deletion Safety — flag hygiene, module excisability
   - 5e. Trade-offs — documented vs undocumented decisions, YAGNI compliance
6. **Scoring card** — 0-5 per dimension with weighted average
7. **Top refactor priorities** — ranked by (severity × blast radius × effort)
8. **ADR suggestions** — MADR 3.0 drafts for undocumented high-impact decisions
9. **What was NOT analyzed** — honest limitations
10. **Threshold sourcing legend** — consensus / default / heuristic

**Scoring scale (per dimension, 0-5):**

| Score | Meaning |
|---|---|
| 0 | Missing — dimension not addressed at all |
| 1 | Rudimentary — severe issues, no thought given to this dimension |
| 2 | Partial — some awareness but significant gaps |
| 3 | Acceptable — basic practices in place, room for improvement |
| 4 | Good — solid practices, minor issues only |
| 5 | Excellent — Staff-level quality, well-documented trade-offs |

**Visualizations (write SVG by hand, < 5KB each):**
- `figures/dimension_scores.svg` — radar/spider chart of 5 dimensions
- `figures/severity_distribution.svg` — stacked bar of findings per dimension × severity
- `figures/data_flow_map.svg` — simplified flow diagram of main data paths

**Completion criteria — emit promise ONLY when ALL true:**
1. `final_report.md` exists; counts match DB queries
2. Every critical/high finding has `remediation` populated
3. Every `suggests_adr=true` tradeoff has a draft ADR file
4. SVG figures exist under `figures/`
5. Quality gates for phases 2-5 (applicable per mode) each have ≥ 1 passed row
6. Scoring card populated for all applicable dimensions

```
<promise>SYSTEM DESIGN AUDIT COMPLETE</promise>
```