# M166 review — wide `SUM(int2 ± const)` pushdown (ClickBench q29)

**Verdict:** READY_TO_MERGE

**Date:** 2026-07-27 · **Commits:** db7040c (feat) + 4016d98 (review fix)
**Reviewers:** council-rust-pgrx (code safety), council-benchmark (measurement honesty)

## Severity matrix

| Severity | council-rust-pgrx | council-benchmark | Resolution |
|---|---|---|---|
| BLOCKER | 0 | 0 | — |
| HIGH | 0 | 0 | — |
| MEDIUM | 1 | 1 | both fixed (4016d98) |
| LOW | 0 | 3 | fixed (4016d98) |
| INFO | 2 | 1 | acknowledged (pre-existing / not M166) |

## council-rust-pgrx (code safety) — no BLOCKER, no HIGH

Verified SOUND: (1) no panic across the C boundary in `classify_sum_int_add_const` (every fallible step is an `Option ?`;
`from_datum` reached only after `constisnull` false, ints only); (2) the deparse `makeVar` hunk (nested Var always valid
before the field reads; non-Var → `null_mut` declines the swap; `exprType(Aggref)` stays int8 — EXPLAIN-cosmetic only);
(3) the wire stride 2→4 (only encode + the single sequential-cursor decode touch the agg section; no hardcoded stride-2
offset elsewhere; Top-N layout has no agg section); (4) the overflow proof (two-extreme domain-wide check ⇒ no per-row
22003, exact widened sum) and the **necessity** of declining int4-base / int8-result.

- **MEDIUM (defense-in-depth):** int-arith gates matched the operator by name without the builtin-only guard
  `classify_text_op` carries — a shadow `OPERATOR +(int2,int4)` + `search_path` manipulation → silent divergence.
  Very-low reachability, pre-existing convention. **FIXED (4016d98):** both gates now decline
  `opno >= FirstNormalObjectId`. Re-validated: type-coverage 31/31, q29 A/B diverged=0 — no regression.
- **INFO:** i64-sum overflow at ~2³² rows (identical to the pre-existing `SumInt` path, not M166); `get_opname` palloc
  (trivial, matches the group gate).

## council-benchmark (measurement honesty) — no BLOCKER, no HIGH

"Core claims measured and honest — not spin." Every ratio reproduces (`0.1027/0.049 = 2.10×`, `27.79/0.1027 = 270×`,
`27.79/0.049 = 567×`); "43/43 byte-identical, 0 regressions" is genuinely backed by the JSON (`result_ab.pass=43,
diverged=0`); routed=32 / declined=11 internally consistent; type-coverage backs the safe class with a live positive
control (diverged=2).

- **MEDIUM:** same-box provenance for the reused ClickHouse baseline was asserted but not in the artifact.
  **FIXED (4016d98):** provenance paragraph added (same droplet 138.197.19.163 / 8 vCPU / 15 GB; ClickHouse not
  re-measured, 0.049 s reused — legitimate as nothing on the ClickHouse side moved).
- **LOW (fixed):** psycopg2-vs-server-side timing asymmetry (ratio is conservative) + q29 variance band (≈2.01–2.32×) now
  cross-referenced; q28 mislabel corrected (q28 = REGEXP GROUP BY; AVG(length)=q27; MIN=q21/q22 — all decline correctly);
  `m166-type-coverage.md` title `M163 → M166`.
- **INFO:** routed count 30 (pre-M165 baseline) → 32 (M166) is M165's q34 landing between runs, now noted.

## Gate outcome

No BLOCKER, no HIGH from either reviewer; both MEDIUMs and all LOWs fixed in 4016d98 and re-validated on the droplet
(type-coverage 31/31, q29 routes byte-identical). **READY_TO_MERGE.**
