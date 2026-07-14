# M97 — Columnar/HTAP viability — PG row-store vs DuckDB columnar

**Hardware:** Intel Xeon Platinum 8168 (DO c-8) · **Dataset:** `hits`-shaped analytical table (ClickBench idiom),
20M rows · **Arms:** vanilla PG row-store vs DuckDB columnar (MIT v1.1.3), same data, same box · **Date:** 2026-07-13.

## Results (warm latency)

| Query | PG row-store | DuckDB columnar | speedup |
|---|---:|---:|---:|
| Q0 — `GROUP BY region, avg(dur), sum(rev)` | 1786 ms | 82 ms | **21.8×** |
| Q1 — `GROUP BY os, browser, avg(rev)` | 1671 ms | 72 ms | **23.2×** |
| Q2 — filtered `count(*)` (`dur>1800 AND rev>50`) | 776 ms | 50 ms | **15.5×** |

## Verdict — columnar value CONFIRMED; DEFER a new pillar

Columnar/vectorized execution is **15–23× faster** than PG row-store on analytical aggregations — columnar has real
value. **But that value is already delivered permissively:** `pg_duckdb` (MIT) was embedded in M61 (ADR 0020) and the
HTAP surface built in M62 (ADR 0021, ~31× OLAP). This benchmark RE-CONFIRMS the shipped route; it discovers no new
differentiator. Combined with the blueprint's license finding (every "go further" peer — moonlink BSL, Hydra/Citus
AGPL — is barred by D1), the honest recommendation is **DEFER** a new columnar pillar (ADR 0041): the permissive
design space is exhausted by what TheoDB already ships.

## Honest caveats

- Single query ≠ a production workload; DuckDB is columnar + vectorized + multicore vs PG's row-store — this measures
  the *columnar advantage* on analytical scans, not a tuned production comparison.
- In-memory (AlloyDB's engine) is not comparable to DuckDB-on-disk; TheoDB's is a lakehouse bet (D2), not AlloyDB's
  in-memory-auto columnar (the paradigm ceiling, per the blueprint).
- NOT a QPS-superiority claim over any closed SOTA (the M73 positioning discipline).
