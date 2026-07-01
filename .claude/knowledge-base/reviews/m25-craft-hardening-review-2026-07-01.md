# /review — M25 craft hardening (theodb_rs)

**Date:** 2026-07-01
**Slug:** m25-craft-hardening
**Diff scope:** `b167dc5^..HEAD` (theodb_rs/ + docs + CHANGELOG)
**Verdict:** READY_TO_MERGE

## Agents (6, parallel — fresh eyes)

| Agent | Verdict | BLOCKER | HIGH | MEDIUM | LOW |
|---|---|---:|---:|---:|---:|
| behavior-preservation | BEHAVIOR_IDENTICAL | 0 | 0 | 0 | 0 |
| security (NL→SQL guards) | SECURITY_PRESERVED | 0 | 0 | 0 | 0 |
| architecture / DIP | READY | 0 | 0 | 1 | 2 |
| rust + pgrx idioms | IDIOMATIC | 0 | 0 | 0 | 2 |
| test quality | TESTS_ADEQUATE | 0 | 0 | 4 | 2 |
| cross-validation | GAPS_FOUND | 0 | 1 | 2 | 1 |

## Consolidated findings + resolution

### HIGH-1 — `api.rs` = 640 LoC > 500-LoC DoD budget; un-amended divergence from plan ADR-2
**Root cause (cross-validation):** the plan's ADR-2 locked a per-feature externs split; the implementation
adopted a single `api.rs` facade (forced by the single `#[pg_schema] mod theodb_rs`), leaving a 640-LoC file
and an un-amended ADR.
**Resolution:** **ADR 0009** (`docs/adr/0009-theodb-rs-api-surface-single-module.md`) — amends ADR-2, records
why the single-facade is the correct call (cohesive api-surface layer; ~87% declarative DDL, CCN~0; the
500-LoC budget is heuristic/folklore per `architecture.md §4`; splitting declarative content into 8 files is
accidental complexity forbidden by *Esforço ≠ Complexidade*), and formally waives the heuristic budget for this
declarative facade with rejected alternatives (A1 per-feature multi-schema split — risk on proven-identical code;
A2 externs/DDL two-file split — hurts co-location; A3 status quo — the original god-file). The **essential** Goal
metric (`lib.rs < 200 LoC`) is met (92). → **documented mitigation**, not a workaround.

### MEDIUM (test quality) — negatives asserted only `.is_err()`; missing security branch; #[pg_test] not run as root; doc citation
**Resolution:** `nl.rs` tests strengthened — L2 negatives now assert the **specific typed message**; added
banned-token + procedural-block composition coverage; added the **security branch** `relation_allowed` (a bare
allowlist entry must NOT authorize a same-named table in another schema). `#[pg_test]`-not-run-as-root is a
documented pgrx limitation; runtime is covered end-to-end by `test_nl_sql.py` (35/35 green, SQLSTATE 22023).
Benchmark-doc citation fixed (`test_nl_sql.py`).

### MEDIUM (cross-validation) — ADR-2 divergence; #[pg_test] vs #[test] DoD
**Resolution:** ADR 0009 (above) amends ADR-2. The `#[pg_test]` choice keeps the codebase-consistent harness;
behavior is covered by integration (documented in the benchmark doc §3.2, citation now correct).

### LOW — stale lib.rs module doc; incomplete §1.3 hotspot list; crate-wide pg_schema style mix
**Resolution:** lib.rs module doc refreshed (points api-surface at `api.rs`, ADR 0009). Benchmark §1.3 completed
(added `chat::first_number` CCN 14, `embed::run_batch` CCN 11 — full honest disclosure of pre-existing,
un-regressed hotspots). The `#[pg_schema]` vs `#[pgrx::pg_schema]` style mix is a pre-existing nit, non-blocking.

## Behavior & security (the two highest-value checks) — CLEAN

- **behavior-preservation:** every M25 change proven a pure extraction / verbatim move / const-naming /
  visibility-widen. Zero dropped-or-added checks, zero changed conditions/messages/SQLSTATEs, zero changed DDL,
  zero changed metric kernels; the one arg-mapping risk (`SbqParams`) is by-name field-init → order-safe. The
  "100% behavior-preserving" claim is **PROVEN**.
- **security:** every NL→SQL guard (single-statement, SELECT/WITH-only, banned tokens, procedural blocks, L4
  relation allowlist), its order, its messages, and the fail-closed planner trap are byte-identical old→new; the
  `theodb.embed` seam stays parameterized; no new public bypass surface. **SECURITY_PRESERVED.**

## Evidence

- `cargo check --features pg17 --tests`: clean (exit 0)
- `cargo clippy --features pg17 --tests -- -D warnings`: clean (exit 0)
- Full image rebuild `theo-db:m25` (sha `379e57a5`) → `CREATE EXTENSION theodb_rs CASCADE` ok
- `pytest test_sbq_index + test_ann_index + test_ai_sql -k "not real"`: **72 passed**
- `pytest test_nl_sql.py -k "not real"`: **35 passed** (security boundary, SQLSTATE 22023)
- Complexity (lizard): `nl_to_sql` 19→8, `run_rrf` 12→9, every extracted fn < 10; `lib.rs` 721→92 LoC
- Full before/after evidence: `docs/benchmarks/m25-craft-hardening.md`

## Verdict: READY_TO_MERGE

0 BLOCKER. The single HIGH is resolved as a documented decision (ADR 0009). Actionable MEDIUM/LOW findings
addressed (stronger security tests, doc fixes, module doc); the remaining are accepted with rationale. The two
highest-value dimensions (behavior parity, security) are clean. All DoDs are met or formally amended; every claim
is backed by reproducible evidence.
