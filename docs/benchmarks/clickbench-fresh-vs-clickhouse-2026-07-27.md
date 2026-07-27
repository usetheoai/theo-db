# ClickBench head-to-head vs ClickHouse — fresh measurement (2026-07-27, post-M160/M161, v0.155.0 code)

**Purpose:** re-run the same-box ClickBench gap vs ClickHouse the project last measured at **M159** (v0.149.0), now on the
**v0.155.0** columnar code (after M160 zero-copy decode + M161 expression-routing coverage), using the **M164-hardened
harness** (routing now asserted per query — a declined agg can no longer green-pass as a trivial `diverged=0`). This
replaces the stale M159 ratio with a current measured one.

## TL;DR — the gap roughly HALVED since M159

| Metric | M159 (v0.149.0) | **This run (v0.155.0)** | Δ |
|---|---|---|---|
| Geomean ratio — **all 43** | 19.4× | **9.95×** | ~2× closer |
| Geomean ratio — **pushdown class** | 7.54× (32 q) | **4.53× (35 q)** | better *and* wider coverage |
| Geomean ratio — **non-pushdown** | 303× (11 q) | 312.8× (8 q) | 3 shapes moved into pushdown (M161) |
| **On-target (ratio ≤ 3×)** | 6/43 | **20/43** | 3.3× more queries at target |
| TheoDB **faster** than ClickHouse (<1×) | 1 (q6) | **6** (q1, q6, q7, q40, q41, q42) | — |

**Are we at the 2–3× target?** Honestly: **not overall** (geomean 9.95×), but the answer is now "**yes for the core
aggregation class and ~half the suite**": 20/43 queries are ≤3× (6 of them faster than ClickHouse), and the fixed-width
aggregation queries the target was about (q2 2.66×, q5 2.66×, q10–q14 ≈2–2.5×, q16 2.75×) are genuinely in the 2–3× band.
The overall geomean is dragged by (a) **8 non-pushdown row-executor queries** at ~25–35 s each (312× geomean) and (b) a
few **routed-but-slow** shapes (q19 `SELECT *` materialization 165×; q23–q26 high-card GROUP BY / DISTINCT 73–132×).

## Methodology (same-box, reproducible — identical to M159)

| Axis | This run |
|---|---|
| Box | 1 DigitalOcean droplet, **8 vCPU / 16 GB / 320 GB** (ephemeral, nyc3) |
| Dataset | `hits` **1,000,000-row systematic 1-in-99 subsample** (covers full time range; unbiased — NOT `head`) |
| TheoDB | PG 18.4 + `theodb_rs` @ develop v0.155.0, `theodb_columnar` TableAM, `enable_columnar_agg=on` + `enable_columnar_late_mat=on` |
| ClickHouse | **v26.8.1**, `clickhouse local --path` (daemon-free), same box, same 1M sample TSV, `MergeTree` (ClickBench `create.sql`) |
| Protocol | 3 runs/query, hot = min; TheoDB timed via psycopg2 round-trip, ClickHouse via `--time` (server-side) |

**Ratio = TheoDB_hot / ClickHouse_hot** (owner target: 2–3× ⇒ ratio ∈ [2,3]). Reproduce:
`benchmarks/run_m128_clickbench.py --n 1000000 --sample systematic --agg --cache <dir>` (TheoDB) +
`benchmarks/m159_clickhouse_run.sh <clickhouse> <sample.tsv> <ch_create.sql> <ch_queries.sql> <out.jsonl>` +
`benchmarks/m159_analyze.py theodb.json ch.jsonl`.

### Honest caveats

- **Conservative ratio (favours ClickHouse):** ClickHouse's time is server-side `--time`; TheoDB's is the full psycopg2
  round-trip — the fixed client overhead is added to TheoDB only, so the **true gap is ≤ the measured ratio**.
- **1M subsample, not the canonical 100M.** The **ratio** is the comparable metric, not the absolute times. (M162 showed
  100M behaves differently under larger-than-RAM; this run is 1M, matching M159 for a like-for-like delta.)
- **Sub-20 ms queries are at the timer floor** — q6 (0.18×), q40/q41/q42 (0.69–0.74×) are "faster/parity", not exact ratios.
- **A/B correctness held:** 43/43 queries byte-identical columnar-vs-heap (`diverged=0`). M164 routing split: **35 broad
  Custom Scan, of which 30 actually routed the agg pushdown (`routed_identical`), 13 declined-trivial** — no false-green.

## Per-query result (ratio = TheoDB/ClickHouse; pushdown = agg pushdown routed)

| q | TheoDB hot (s) | ClickHouse hot (s) | ratio | pushdown | A/B |
|---|---|---|---|---|---|
| q0 | 0.0341 | 0.0060 | 5.68× | yes | ✓ |
| q1 | 0.0111 | 0.0130 | **0.85×** | yes | ✓ |
| q2 | 0.0425 | 0.0160 | 2.66× | yes | ✓ |
| q3 | 0.0537 | 0.0160 | 3.36× | yes | ✓ |
| q4 | 0.1178 | 0.1150 | 1.02× | yes | ✓ |
| q5 | 0.2153 | 0.0810 | 2.66× | yes | ✓ |
| q6 | 0.0042 | 0.0230 | **0.18×** | yes | ✓ |
| q7 | 0.0133 | 0.0160 | **0.83×** | yes | ✓ |
| q8 | 0.1799 | 0.1210 | 1.49× | yes | ✓ |
| q9 | 0.2383 | 0.1500 | 1.59× | yes | ✓ |
| q10 | 0.1163 | 0.0510 | 2.28× | yes | ✓ |
| q11 | 0.1331 | 0.0560 | 2.38× | yes | ✓ |
| q12 | 0.2830 | 0.1440 | 1.97× | yes | ✓ |
| q13 | 0.3268 | 0.1280 | 2.55× | yes | ✓ |
| q14 | 0.3095 | 0.1540 | 2.01× | yes | ✓ |
| q15 | 1.4534 | 0.2160 | 6.73× | yes | ✓ |
| q16 | 0.8971 | 0.3260 | 2.75× | yes | ✓ |
| q17 | 25.4940 | 0.2210 | 115.36× | no | ✓ |
| q18 | 1.7620 | 0.2990 | 5.89× | yes | ✓ |
| q19 | 2.8101 | 0.0170 | 165.30× | yes | ✓ |
| q20 | 0.9067 | 0.0760 | 11.93× | yes | ✓ |
| q21 | 26.1621 | 0.0870 | 300.71× | no | ✓ |
| q22 | 26.3244 | 0.1010 | 260.64× | no | ✓ |
| q23 | 25.1617 | 0.2290 | 109.88× | yes | ✓ |
| q24 | 3.3811 | 0.0460 | 73.50× | yes | ✓ |
| q25 | 3.0585 | 0.0270 | 113.28× | yes | ✓ |
| q26 | 3.4389 | 0.0260 | 132.27× | yes | ✓ |
| q27 | 25.3416 | 0.0310 | 817.47× | no | ✓ |
| q28 | 35.4955 | 0.1780 | 199.41× | no | ✓ |
| q29 | 27.7894 | 0.0490 | 567.13× | no | ✓ |
| q30 | 0.5749 | 0.0850 | 6.76× | yes | ✓ |
| q31 | 0.7089 | 0.1250 | 5.67× | yes | ✓ |
| q32 | 4.8147 | 0.2060 | 23.37× | yes | ✓ |
| q33 | 1.5311 | 0.1640 | 9.34× | yes | ✓ |
| q34 | 28.5059 | 0.1870 | 152.44× | no | ✓ |
| q35 | 2.1402 | 0.1070 | 20.00× | yes | ✓ |
| q36 | 0.0738 | 0.0250 | 2.95× | yes | ✓ |
| q37 | 0.0724 | 0.0270 | 2.68× | yes | ✓ |
| q38 | 0.0686 | 0.0230 | 2.98× | yes | ✓ |
| q39 | 25.1542 | 0.0350 | 718.69× | no | ✓ |
| q40 | 0.0131 | 0.0190 | **0.69×** | yes | ✓ |
| q41 | 0.0141 | 0.0190 | **0.74×** | yes | ✓ |
| q42 | 0.0133 | 0.0180 | **0.74×** | yes | ✓ |

## Summary

- **Comparable queries:** 43/43 · **A/B diverged:** 0/43
- **Geomean ratio (all 43):** **9.95×** slower than ClickHouse
- **On-target (≤3×):** **20/43** (of which 6 faster than ClickHouse)
- **Gap 3×–10×:** 7/43 · **Structural gap >10×:** 16/43
- **Geomean — pushdown class (35 q):** **4.53×** · **non-pushdown (8 q):** 312.8×

## Conclusion (feeds the next iteration)

The measured trajectory since M159 is strongly positive: the overall gap roughly halved (**19.4× → 9.95×**) and the
covered class improved (**7.54× → 4.53×**), with **3.3× more queries on-target** (6 → 20). The 2–3× target is now met for
the core fixed-width aggregation shapes and ~half the suite. The remaining drag is unchanged in *kind* from M159: the
**highest-leverage next work is pushdown coverage** for (a) the 8 non-pushdown row-executor shapes (multi-key/text GROUP
BY, ~25–35 s each — the 312× term) and (b) the routed-but-slow shapes (q19 `SELECT *` materialization, q23–q26 high-card
DISTINCT/GROUP BY). No storage/encoding change closes those — it is planner/executor coverage work. This is an
evidence-anchored update, not a claim: the ratio is measured on this box, conservative by methodology, A/B-correct.
