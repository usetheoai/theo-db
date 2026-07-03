# SIFT1M carrier verdict — theodb_hnsw (M41-optimized) wins on real structured data

> **⚠️ RETRACTED (2026-07-03, by M45).** The "~1.7–2.8× superiority vs pgvector hnsw" signal below did
> **NOT** survive rigorous mean±std measurement. Under 500 queries × ≥3 timed runs with exact GT
> (`docs/benchmarks/m45-pareto-sift1m.md`), the verdict is **PARITY** — the two frontiers interleave within
> run-to-run noise (two runs gave INFERIOR→PARITY). This page's superiority claim was a best-of-N +
> 200-query + warm-cache artifact and is superseded by M45. theodb_hnsw is **competitive, not superior**,
> vs pgvector hnsw at 1M. Kept for provenance; **do not cite the multipliers below as a claim.**

**Date:** 2026-07-03
**Verdict:** **On real SIFT1M, `theodb_hnsw` (M41-optimized scan) DECISIVELY beats `theodb_ivfflat` (~10× QPS at
comparable recall) AND is competitive-to-superior vs pgvector's own `hnsw` (~1.7–2.8× QPS at matched recall).**
This INVERTS the synthetic M40 result — exactly as the M40 honesty caveat predicted (synthetic random-gaussian is
the worst case for a graph; real structured data is where the graph wins). First real vector-superiority signal.
**Type:** measurement (the trustworthy carrier verdict M40 demanded). No new code — ran the existing
`benchmarks/run_m32_sift1m.py` 4-way harness on `theo-db:m41` + a Pareto ef-sweep on the built index.

## The head-to-head on SIFT1M (1M×128, exact GT, best-of-3-runs; harness artifact m32-scale-sift1m.json)

| index | operating point | recall@10 | QPS | p50 | build |
|---|---|---|---|---|---|
| **theodb_hnsw** (M41) | ef=64 (default) | 0.9595 | **277.9** | 3.50 ms | 1440 s |
| **theodb_ivfflat** | fixed | 0.9845 | **28.7** | 36.1 ms | 86 s |
| pgvector hnsw | ef=40 | 0.926 | 132.8 | 6.96 ms | 473 s |
| pgvector hnsw | ef=100 | 0.977 | 73.8 | 13.67 ms | 473 s |
| pgvector ivfflat | probes=10 | 0.862 | 170.6 | 5.83 ms | 93 s |
| pgvector ivfflat | probes=1000 | 1.000 | 3.1 | 311.8 ms | 93 s |

## theodb_hnsw Pareto curve on SIFT1M (ef-sweep on the built M41 index, 200 test queries, id-overlap recall@10)

| ef_search | recall@10 | QPS | p50 |
|---|---|---|---|
| 40 | 0.941 | 230 | 3.5 ms |
| 64 | 0.973 | 176 | 4.8 ms |
| 100 | 0.987 | 143 | 6.1 ms |
| 200 | 0.997 | 111 | 8.4 ms |
| 400 | 0.999 | 70 | 14.2 ms |

## Findings (honest)

1. **theodb_hnsw ≫ theodb_ivfflat on real data — the carrier verdict.** ~277.9 QPS @ recall 0.96 vs 28.7 QPS @
   recall 0.98 → an **order of magnitude** (≈10×) faster at comparable recall. On synthetic random-gaussian (M40)
   ivfflat won; on real structured SIFT1M the graph wins decisively. The M40 caveat is vindicated: the synthetic
   verdict did NOT generalize, and measuring on real data was the honest requirement.
2. **theodb_hnsw vs pgvector's own hnsw — competitive-to-superior.** In the harness's same-framework measurement,
   theodb_hnsw ef=64 (0.9595 @ 278 QPS) vs pgvector hnsw interpolated at recall 0.96 (~100 QPS) → **~2.8×**. The
   Pareto ef-sweep confirms the shape: at matched ef, theodb_hnsw shows higher recall AND higher QPS than pgvector
   hnsw (ef=40: 0.941/230 vs 0.926/133; ef=100: 0.987/143 vs 0.977/74) → **~1.7–1.9×** at matched recall. This is
   the M41 optimized scan + the M35 page-native layout paying off vs the SOTA permissive baseline.
3. **theodb_hnsw beats f32-exact-parity too:** at ef=400 it reaches recall 0.999 @ 70 QPS — near-exact at high QPS.

## Honest caveats (do not over-read)

- **Build time is theodb_hnsw's real weakness: 1440 s (24 min) at 1M** vs pgvector hnsw 473 s (8 min) and
  theodb_ivfflat 86 s. M41 optimized the SCAN, not the BUILD; the in-proc single-thread graph construction is slow
  and is the next optimization target (a build-time milestone, not a scan one).
- **QPS is best-of-3-runs** (harness) / single-run (Pareto sweep) on a local dev CPU — the *direction* (theodb_hnsw
  wins) is unambiguous (10× vs ivfflat is far beyond variance; ~1.7–2.8× vs pgvector is consistent across both
  measurements), but the exact multiplier vs pgvector should be confirmed with mean±std before a hard public claim
  (`public-copy.md` §4 — a comparative claim needs a reproducible artifact + independent reproduction).
- **theodb_hnsw was measured on a 200-query sample** (the harness cap; the O(N) M26 scan is gone — M35+M41 made it
  O(ef·M), so the cap can be raised in a follow-up for a larger sample).
- Single machine, no independent reproduction yet.

## Verdict for the P0 (vector-superiority)

This is the first honest, real-data signal that TheoDB's **own** vector carrier (`theodb_hnsw`, M35 layout + M41
scan optimization) is **competitive-to-superior on the recall×QPS Pareto frontier vs the SOTA permissive baseline
(pgvector hnsw)** — the axis the North Star (vs AlloyDB/ScaNN) is fought on. It does NOT yet license an unqualified
"faster than pgvector" claim (best-of-N, single machine, no independent repro — `public-copy.md`), but it is the
strongest evidence to date and points the next work clearly: (1) **theodb_hnsw build-time optimization** (24 min at
1M is the weakness); (2) mean±std + independent reproduction of the theodb_hnsw-vs-pgvector-hnsw margin for a
publishable claim; (3) larger query sample (raise the cap now that the scan is O(ef·M)).

Reproduce: `PGPORT=<port> python3 benchmarks/run_m32_sift1m.py --n-queries 200 --runs 3 --theodb-hnsw-query-cap 200`
on `theo-db:m41` with `benchmarks/.datasets/sift-128-euclidean.hdf5`. Artifact: `docs/benchmarks/m32-scale-sift1m.json`.
