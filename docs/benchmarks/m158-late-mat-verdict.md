# M158 — late-materialization top-k: measured verdict

**Date:** 2026-07-25
**Hardware:** DigitalOcean droplet (theo-m158), PostgreSQL 18.4, `theodb_rs` release build, `max_parallel_workers_per_gather=0`.
**Query:** `SELECT * FROM big ORDER BY wid LIMIT 10` — `big` = 2,000,000 rows × 30 columns (wide `SELECT *`, the M148 form_row-heavy regime).

## Correctness (the gate — byte-identical or it does not ship)

`benchmarks/m158_ec_harness.sql` — LIMIT-preserving symmetric-EXCEPT A/B (`off` = native `Limit→Sort→theodb_columnar_project`; `on` = the top-k CustomScan). Sort key `wid` is UNIQUE → top-k boundary has no ties → deterministic comparison (M155 tie caveat neutralized).

| Query | Shape | `*_ab_mism` | EXPLAIN (on) |
|---|---|---|---|
| Q1 | `SELECT *` ORDER BY wid ASC LIMIT 10 | **0** | `Limit → Custom Scan` (Sort gone) |
| Q2 | `SELECT * WHERE v>=3` ORDER BY wid LIMIT 10 | **0** | `Limit → Custom Scan` |
| Q3 | `SELECT * WHERE s LIKE '%foo%'` ORDER BY wid LIMIT 10 | **0** | `Limit → Custom Scan` |
| Q4 | `SELECT *` ORDER BY wid **DESC** LIMIT 10 | **0** | `Limit → Custom Scan` |
| Q5 | `SELECT wid,cid,f WHERE cid>0` ORDER BY wid LIMIT 15 | **0** | `Limit → Custom Scan` |

**Verdict: byte-identical** across projection-all / projection-subset, numeric filter, text-LIKE filter, ASC, DESC. The `Sort` node disappears when `theodb.enable_columnar_late_mat=on` (proof the swap fired — an earlier false-green, where the plan still showed `Sort`, was traced to the planner_hook only invoking `swap_walk` under the aggregate GUC; fixed to also run under the late-mat GUC).

## Performance (bare-query `\timing`, 1 warm-up + 5 measured, `benchmarks/m158_perf.sql`)

Measured with `\timing` on the BARE query — NOT `EXPLAIN (ANALYZE)`, whose per-row TIMING instrumentation taxes the 2M-tuple OFF path far more than the 10-tuple ON path and inflates the baseline (council-benchmark H2; the earlier ANALYZE run read 1.64× — the clean number below is lower and is the one that ships).

| Path | run1 | run2 | run3 | run4 | run5 | **median** | mean |
|---|---|---|---|---|---|---|---|
| **OFF** (native `Sort` top-N heapsort) | 8778 | 8771 | 8841 | 8940 | 8817 | **8817 ms** | 8829 ms |
| **ON** (late-mat top-k) | 5433 | 5464 | 5506 | 5498 | 5476 | **5498 ms** | 5475 ms |

**Speedup: 1.60× median (8817/5498), 1.61× mean.** Dispersion is tight and the ON series is converged after the warm-up (5433–5506 ms), so the headline is a steady state, not a still-warming run. The native path spends most of its time materializing all 2M rows as PG tuples (`form_row`/`palloc` — the M148 bottleneck, ~80% of the scan, measured at ~8236 ms in the earlier `EXPLAIN ANALYZE` scan node); the top-k path decodes the columns to one Arrow batch (column-major, no per-row palloc), runs DataFusion `sort→limit(k)`, and materializes only the k=10 survivors → the form_row cost is paid for 10 rows, not 2,000,000.

## Honest trade-off

| | native (OFF) | late-mat (ON) |
|---|---|---|
| Time | 8817 ms (median) | 5498 ms (**1.60× faster**) |
| Peak sort/top-k memory | **27 kB** (top-N heapsort, O(k), measured `Sort Method:` line) | **~370 MB estimated** (full Arrow batch, O(N); derived from the batch size, not profiled RSS) |

Late materialization here is a **time-for-memory trade**: it skips the dominant per-row materialization cost, but holds the whole decoded projection in RAM (the DataFusion memory pool is sized to the batch — an earlier run hit `Resources exhausted` at the 4 MB default; fixed by sizing the pool to `batch_bytes*2 + 64 MB`). The native top-N heapsort streams tuples and keeps only k (27 kB). For a wide `SELECT … ORDER BY key LIMIT k` with RAM to spare, the 1.60× win is real; for very large N the O(N) batch is the scaling ceiling (a streaming/refetch late-mat — decode only {key∪filter} for N, refetch the rest for k — would remove it, but is out of M158 scope; blueprint ADR).

## Caveats (council review)

- **Synthetic-data bias (council-benchmark M2):** the `big` generator uses low-cardinality `g%N` columns (highly compressible). This biases the win UP (cheap Arrow decode) and the memory cost DOWN (small batch) vs real high-cardinality data (URLs/user-agents), where the 1.60× would shrink and the batch would balloon. Treat 1.60× as an upper bound on synthetic data.
- **Boundary-tie determinism (council-index-storage INFO):** the byte-identical A/B holds because `wid` is UNIQUE (no k-boundary ties). For a non-unique key, PG top-N heapsort and DataFusion TopK may pick different equal-key rows at the boundary — both SQL-legal (tie order unspecified), so row-for-row parity with native is a property of unique-key data, not a guaranteed invariant.
- **Text sort key (council-index-storage HIGH, fixed):** a text ORDER-BY key is admitted ONLY under C/POSIX collation (byte order == PG order); any other collation (incl. deterministic linguistic ones like `en_US.UTF-8`) declines to the native plan — determinism fixes equality, not sort order.
- **Scale:** correctness is proven at 20k rows (`m158_ec_harness.sql`), performance at 2M (`m158_perf.sql`). Top-k correctness is not scale-dependent; the split is intentional.

## Decision

**Ship the capability, GUC-gated `theodb.enable_columnar_late_mat` default OFF** — the conservative honest default given the O(N) memory cost. **[SUPERSEDED by M167 — the default is now ON.](m167-projection-topk-verdict.md)** M167 measured that q23/q24 route byte-identically at 1M and replaced "default OFF" with a plan-time decode-size guard (ADR-4) as the bound on the O(N) cost. Enabling it delivers a measured 1.60× on wide top-k. Correctness is unconditional (byte-identical + order-identical). Reproduce: `benchmarks/m158_ec_harness.sql` (correctness) + `benchmarks/m158_perf.sql` (perf, includes the exact `big` DDL + deterministic generator).
