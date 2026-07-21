# Review — M129 official-benchmark OLTP pillar

**Slug:** official-benchmark-oltp
**Milestone:** M129
**Date:** 2026-07-21
**Reviewer:** council-benchmark (Dr. Ethan Brooks lens — "você mediu ou está supondo?"), 2 passes (audit + re-review)
**Verdict:** READY_TO_MERGE

## Scope

The M129 OLTP benchmark artifact: pgbench (TPS) + HammerDB TPROC-C (NOPM) as external out-of-tree drivers against
self-hosted TheoDB PG17, applying ADR-0050 (adopt-and-wrap). Files: `benchmarks/run_m129_oltp.py`,
`benchmarks/oltp/hammerdb_tproc_c.tcl`, `benchmarks/theodb_bench/test_oltp.py`, `docs/benchmarks/m129-oltp.md` +
`m129-oltp-session-{1,2,3}.json`.

## Findings (first pass, commit 91348d7)

| Sev | ID | Finding | Resolution |
|---|---|---|---|
| HIGH | H1 | "3 sessions / 1278.8–1514.8" headline — only 1 of 3 sessions had a committed JSON; upper bound un-backed | RESOLVED — 3 per-session artifacts committed (10 runs each); every cited number resolves to one |
| HIGH | H2 | split-half `paired_significance` at n=2 is vacuous AND misapplied for a single system | RESOLVED — replaced with coefficient of variation (honest single-system dispersion); paired sig documented as the A/B-only capability |
| MEDIUM | M1 | `fsync=on` asserted from argparse default, not measured | RESOLVED — server-reported via `SHOW fsync`/`synchronous_commit` |
| MEDIUM | M2 | markdown cited a co-tenant list absent from the JSON `box` field | RESOLVED — co-tenant detail recorded in each artifact's `box` field |
| MEDIUM | M3 | range was session-means, not full per-run spread | RESOLVED — full per-run spread 1073.8–1481.4 reported |
| LOW | L1 | single 2-min HammerDB run called "claim-grade" | RESOLVED — labeled a functional smoke |

## Re-review (commit 9f084c1) — verification against disk

- **H1 RESOLVED**: all 3 sessions committed (n=10 each); old overwritten `m129-oltp.json` git rm'd (empty `git ls-files`). Session means 1297.4 / 1247.3 / 1328.3 reproduce exactly.
- **H2 RESOLVED**: `paired_significance` removed; `coefficient_of_variation` in place with honest docstring. CV 7.56 / 9.46 / 7.63% and between-session 3.17% reproduce exactly. 8 unit tests green.
- **M1/M2/M3/L1 RESOLVED**: verified against `run_m129_oltp.py`, the artifacts, and the markdown.
- **D1 clean**: no HammerDB source vendored (only our 1.6 KB `.tcl` driving the GPLv3 CLI as an external Docker process); pgbench = PostgreSQL License.
- **INFO (non-blocking)**: `feat` commit 91348d7 subject says NOPM=16372 (pre-fix run); the committed artifact + markdown consistently say NOPM=18269 (post-fix re-run). Internally consistent; no action.

## DoD check (plan `official-benchmark-oltp-plan.md`)

| DoD item | Status |
|---|---|
| Driver + Tcl + unit tests green (8) | Met ✓ |
| MEASURED pgbench TPS over repeated runs + run-to-run stability metric | Met ✓ (3 sessions × 10 runs, CV) |
| HammerDB TPROC-C NOPM measured | Met ✓ (NOPM=18269) |
| Every throughput number paired with crash gate + server-reported `fsync` | Met ✓ |
| HammerDB never vendored/linked (GPLv3) | Met ✓ |

## Verdict

**READY_TO_MERGE.** 0 BLOCKER, 0 residual HIGH. Every measured number reproduces bit-for-bit from a committed
artifact; honest framing (self-hosted NOT canonical hardware, NOPM NOT audited tpmC, no "faster than X"); D1 clean.
