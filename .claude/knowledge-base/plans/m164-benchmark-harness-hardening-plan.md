---
slug: m164-benchmark-harness-hardening
milestone_id: M164
created_at: 2026-07-27
goal: Add three false-green/infra guards to benchmarks/run_m128_clickbench.py so a stale sample, a silently-declined A/B arm, and an undersized box each fail or warn loudly instead of producing a false DONE.
---

# M164 — Endurecer o harness de benchmark (guards de falso-verde + pre-flight de infra)

## Goal

Harden `benchmarks/run_m128_clickbench.py` with three guards so the measured DoD triple — sample-count integrity,
A/B-routing integrity, and box-sizing adequacy — is asserted mechanically; success metric: a new
`benchmarks/test_m164_harness_guards.py` passes with cases proving each guard fires (stale-cache re-materialize,
declined-ON-arm flagged, undersized-disk blocks / larger-than-RAM warns).

## Context

Retro items B+C of the M160-M162 retrospective (`/roadmap-feature`, 2026-07-27). Hard evidence from that session,
recorded in memory `m162-100m-load-gotchas`:

- **(B1)** `_ensure_sample` reused a **1M cache as "100M"** with no count check — a false `DONE` caught only by hand.
- **(B2)** an A/B with `ORDER BY` removed became SORTED → `diverged=0` **trivially** because the ON arm declined (ran
  native), proving nothing about the pushdown.
- **(C)** the 15 GB box (reused from M160) was undersized for 100M and **OOMed 2×**; hours lost to mechanizable noise.

M163 already applied the routing-assert principle to the *type-coverage* harness (`columnar_type_ab.py`). M164 applies
the same rigor to the *ClickBench* harness (`run_m128_clickbench.py`), which is a different script and still trusts a
non-empty cache file and a bare `diverged=0`.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Why it exists |
|---|---|---|---|
| `benchmarks/run_m128_clickbench.py` | 302 (`wc -l`) | M131 (`git log -1`) | The M128 ClickBench columnar A/B harness — loads `hits`, runs 43 queries 3×, EXPLAIN + byte-identical A/B vs heap. |
| `benchmarks/test_m164_harness_guards.py` | 0 (NEW) | — | Unit tests for the three new guards (pure-logic, no DB). |

### Current callers / dependents (real file:line)

- `_ensure_sample(path, n_rows, strategy)` — `run_m128_clickbench.py:79`. Bug at `:94`:
  `if os.path.isfile(path) and os.path.getsize(path) > 0: return True` — returns a cached file **without checking its
  row count** against `n_rows`. Callers: `run()` at `:197`.
- `_bench_query(...)` — `:147`. Sets `entry["columnar_customscan"]` at `:169`
  (`"theodb_columnar_agg" in plan or "Custom Scan" in plan`) and `entry["result_ab_identical"]` at `:181`.
- `run(args)` — `:189`. Verdict at `:244-252`: `ab_pass = sum(... result_ab_identical is True)` and
  `ab_diverged = sum(... is False)` — **`ab_pass` counts identical regardless of whether the ON arm routed**, so a
  declined query that is trivially identical inflates the pass count (the B2 false-green).
- `main()` argparse — `:262`: `--n` (default 1_000_000), `--sample`, `--cache` (default `benchmarks/.cache`), `--agg`.
- Sampling step: `k = HITS_TOTAL_ROWS // n` (`:105`) then `awk 'NR % k == 0' | head -n {n}` (`:110`) — integer
  division can yield **1 row more** than `n` before the `head` cap (the systematic off-by-one the count assert must tolerate).

### Domain glossary

- **A/B (result)**: run the aggregation on columnar `hits` vs heap `hits_heap`; byte-identical multiset ⇒ storage/pushdown correct.
- **ON arm routed**: the columnar query used a `Custom Scan` (pushdown) — only then is the A/B a *meaningful* pushdown oracle.
- **Trivial A/B / false-green**: `diverged=0` where the ON arm ran the *native* executor over columnar storage — proves round-trip, not pushdown.
- **systematic off-by-one**: `HITS_TOTAL_ROWS // n_rows` integer division can admit one extra row before the `head` cap.

### Architecture boundaries affected

`benchmarks/` is a standalone measurement harness (no DIP layering into the engine). Pure helpers (count-compare,
A/B-classify, sizing) MUST be import-safe without a DB so they unit-test with no I/O (`rules/testing.md § 2` — unit tier).
Read-only against the engine; never mutates `theodb_rs`.

## Prior Art & Related Work

- Memory `m162-100m-load-gotchas` — the three measured traps this milestone guards against.
- Memory `infra-nao-usar-maquina-do-ci` — box-selection discipline (do not reuse the CI runner).
- M163 `benchmarks/columnar_type_ab.py` — the routing-assert pattern (`plan_routes` requires a specific `Custom Scan`)
  reused conceptually here for B2.
- `docs/benchmarks/m162-100m-gap-verdict.md` — the run whose gotchas motivated this.

## ADRs

### ADR-1 — pure helpers + injected environment (testable without a DB or a real box)

The three guards are implemented as **pure functions** that take their environment as arguments
(`rows_in_file`, `columnar_customscan`+`result_ab_identical`, `disk_free_bytes`+`ram_bytes`), so unit tests inject
fakes and assert behavior with zero I/O. **Alternative rejected:** live checks reading the real filesystem/DB inside
the guards — untestable deterministically, and the M162 lesson is precisely that un-asserted mechanizable state hides
bugs. Cites `rules/testing.md § 3` (deterministic tests, inject the clock/env) and Unbreakable Rule 8 (fail-fast at the boundary).

### ADR-2 — disk BLOCKS, RAM only WARNS

Pre-flight refuses (BLOCK) when the estimated **on-disk** sample exceeds a safe fraction of free disk (a load that
cannot fit is pure waste), but only **WARNS** when the estimated in-DB working set exceeds RAM headroom, because
**larger-than-RAM is an intentional TheoDB test regime** (M162 measured exactly that). **Alternative rejected:** block
on RAM too — would forbid the legitimate larger-than-RAM run the project deliberately does (roadmap Risk (a)). Cites the
M164 roadmap Risks and `CLAUDE.md` North Star (billion-scale / larger-than-RAM).

### ADR-3 — B2 classifies, does not force every query to route

The A/B guard adds a per-query classification (`routed_identical` / `declined_trivial` / `diverged` / `n/a`) and a
run-level flag when `--agg` is set yet **zero** queries routed (the whole point was pushdown, nothing was exercised).
**Alternative rejected:** fail any query whose ON arm declines — wrong, because many of the 43 ClickBench queries
legitimately decline (storage path) and that is not an error. The guard flags *trivial-passes masquerading as
pushdown-correctness*, not honest declines. Cites the M162 evidence (the specific declined-ON false-green).

## Dependency Graph

Phase 1 (B1 count assert), Phase 2 (B2 A/B-routing classify), Phase 3 (C pre-flight sizing) are **independent** — each
is a self-contained guard + its unit tests. Phase 4 (integration validation) depends on 1-3. No inter-phase barrier.

## Phase 1 — B1: `_ensure_sample` asserts row count

### T1.1 — count-check helper + re-materialize on mismatch

#### Why this step
The action: add a pure helper `sample_is_fresh(rows_in_file, n_rows)` and make `_ensure_sample` re-stream when the
cache row count does not match `n_rows` (within the systematic off-by-one tolerance). The reasoning: `:94` trusts a
non-empty file; the M162 false-100M came from exactly this — a 1M cache served as 100M. Baseline Context row
`_ensure_sample:94` is the defect; ADR-1 makes it testable.

#### Files to edit
- `benchmarks/run_m128_clickbench.py` (`_ensure_sample` + new `sample_is_fresh`)
- `benchmarks/test_m164_harness_guards.py` (NEW)

#### TDD
- RED `test_sample_is_fresh_rejects_1m_cache_for_100m`: `sample_is_fresh(1_000_000, 100_000_000)` is False.
- RED `test_sample_is_fresh_accepts_exact`: `sample_is_fresh(1_000_000, 1_000_000)` is True.
- RED `test_sample_is_fresh_tolerates_systematic_off_by_one`: `sample_is_fresh(1_000_001, 1_000_000)` is True
  (integer-division may admit one extra) but `sample_is_fresh(1_000_050, 1_000_000)` is False.
- GREEN: implement `sample_is_fresh(rows_in_file, n_rows, tol=1)`; `_ensure_sample` computes `rows_in_file` via a
  streaming `wc -l` on the cache and re-materializes when not fresh (drops the early `return True`).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `pytest benchmarks/test_m164_harness_guards.py -k sample_is_fresh` green.
- `_ensure_sample` no longer returns a stale-count cache: a 1M cache with `--n 100000000` triggers a re-stream branch.

## Phase 2 — B2: A/B asserts routing (no trivial diverged=0)

### T2.1 — classify each A/B outcome by whether the ON arm routed

#### Why this step
The action: add a pure `classify_ab(columnar_customscan, result_ab_identical)` and surface `ab_routed_identical` +
a `no_pushdown_exercised` flag in `run`'s verdict. The reasoning: `run:244` counts `ab_pass` from `result_ab_identical`
alone, so a declined-but-identical query inflates the pushdown pass count — the M162 SORTED/`diverged=0` false-green.
ADR-3 scopes it to flag trivial passes, not honest declines.

#### Files to edit
- `benchmarks/run_m128_clickbench.py` (`classify_ab` + verdict counters in `run`)
- `benchmarks/test_m164_harness_guards.py`

#### TDD
- RED `test_classify_ab_declined_identical_is_trivial`: `classify_ab(False, True) == "declined_trivial"` — the "ON não roteou" guard.
- RED `test_classify_ab_routed_identical_is_meaningful`: `classify_ab(True, True) == "routed_identical"`.
- RED `test_classify_ab_diverged`: `classify_ab(True, False) == "diverged"` and `classify_ab(False, False) == "diverged"`.
- RED `test_classify_ab_na`: `classify_ab(None, None) == "n/a"`.
- GREEN: implement `classify_ab`; in `run`, add `ab_routed_identical = sum(classify_ab(...) == "routed_identical")`
  and, when `args.agg` and `ab_routed_identical == 0`, set `result_ab["no_pushdown_exercised"] = True` with a loud note.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `pytest benchmarks/test_m164_harness_guards.py -k classify_ab` green.
- A `--agg` run whose queries all decline reports `no_pushdown_exercised: True` instead of a clean byte-identical verdict.

## Phase 3 — C: pre-flight sizing (disk BLOCK, RAM WARN)

### T3.1 — sizing estimator with injected disk/RAM

#### Why this step
The action: add a pure `preflight_sizing(n_rows, disk_free_bytes, ram_bytes)` returning `{block, warn, reasons}`, and
call it in `run` before the load — BLOCK (return UNBENCHMARKED) on disk, WARN (print + continue) on RAM. The reasoning:
the M162 15 GB box OOMed 2× on 100M; disk-exhaustion is pure waste, RAM-exhaustion is the intentional larger-than-RAM
regime (ADR-2). Injected env per ADR-1.

#### Files to edit
- `benchmarks/run_m128_clickbench.py` (`preflight_sizing` + call site in `run`)
- `benchmarks/test_m164_harness_guards.py`

#### TDD
- RED `test_preflight_blocks_when_disk_too_small`: estimated sample > 0.8 × disk_free → `block is True`.
- RED `test_preflight_warns_but_not_blocks_larger_than_ram`: estimated in-DB > ram_headroom but fits disk →
  `block is False` and `warn is True` (larger-than-RAM is intentional — ADR-2).
- RED `test_preflight_ok_when_both_fit`: fits disk and RAM → `block is False`, `warn is False`.
- GREEN: implement with documented `EST_TSV_BYTES_PER_ROW` / `EST_INDB_BYTES_PER_ROW` constants (derived from the
  ClickBench hits full size ÷ 99.9M rows, cited in a comment); wire the call in `run` (block → UNBENCHMARKED; warn → print).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `pytest benchmarks/test_m164_harness_guards.py -k preflight` green.
- `run` refuses a load that cannot fit disk (UNBENCHMARKED with a sizing reason) and warns-but-proceeds on larger-than-RAM.

## Phase 4 — Integration Validation

### T4.1 — full guard suite + CHANGELOG + import-safety

#### Concurrency tests
(none — single-threaded)

#### Validation
- `pytest benchmarks/test_m164_harness_guards.py -q` all green.
- `python3 -c "import benchmarks.run_m128_clickbench"` (or `importlib`) succeeds with no DB — pure helpers import-safe.
- CHANGELOG `[Unreleased]` updated with the M164 entry.
- Ruff clean on the changed file.

## Coverage Matrix

| Goal claim / DoD item | Task(s) |
|---|---|
| `_ensure_sample` re-materializes when `wc -l cache ≠ n` (stale 1M vs `--n 100M`) with off-by-one tolerance | T1.1 |
| A/B asserts routing — a declined ON arm is flagged, not a trivial `diverged=0` | T2.1 |
| Pre-flight sizing: disk BLOCK, larger-than-RAM WARN | T3.1 |
| CHANGELOG `[Unreleased]` entry | T4.1 |
| Guards are deterministic / unit-tested with no DB | T1.1, T2.1, T3.1 (ADR-1) |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Pre-flight too strict blocks a legitimate larger-than-RAM run | Medium | RAM path is WARN-only, never BLOCK (ADR-2); explicit test `test_preflight_warns_but_not_blocks_larger_than_ram` | me |
| Count assert false-positives on the systematic off-by-one and re-streams a good cache (wastes hours) | Medium | `tol=1` tolerance; test `test_sample_is_fresh_tolerates_systematic_off_by_one` proves the boundary | me |
| `EST_*_BYTES_PER_ROW` constants drift from reality → wrong block/warn | Low | Derived from the published ClickBench hits size ÷ row count, cited in a comment; sizing is advisory precision, not exact | me |

## Unresolved Questions

- Whether to also assert the heap-copy row count equals the columnar row count post-load (a fourth guard). Deferred:
  out of the M164 DoD (which is sample-count, A/B-routing, sizing); a follow-up if a load-integrity gap is ever measured.

## Failure scenarios

(none — no external I/O touched; the harness streams a dataset but M164 adds only in-process guards over local state
(row counts, plan strings, `shutil.disk_usage`/`/proc/meminfo`), not new network or DB calls.)

## Global Definition of Done

- All phase Acceptance Criteria green; `pytest benchmarks/test_m164_harness_guards.py -q` passes.
- Ruff clean on `benchmarks/run_m128_clickbench.py`.
- CHANGELOG `[Unreleased]` has the M164 entry (Unbreakable Rule 6).
- File-size budget respected (`run_m128_clickbench.py` stays well under 500 LoC).
- `/plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS; `/code-quality` ∉ {FAIL_HARD, INVALID}; `/review` READY_TO_MERGE.
