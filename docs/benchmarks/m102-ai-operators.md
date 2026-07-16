# M102 — AI operators as optimizable plan nodes (benchmark)

**Date:** 2026-07-16 · **Host:** DigitalOcean droplet `theo-m98-pgrx19` (8 vCPU, 15 GiB) · **PG:** 17.10 / pgrx 0.19.0
**Harness:** `theodb_rs/isolation/bench_m102.sh` (reproducible) · **Raw:** [`m102-ai-operators.json`](./m102-ai-operators.json)

## What is measured

Two artifacts, per ADR D3/D4 of the plan:

1. **Deterministic (HTTP-free, `theodb.llm_test_model='parity'`, CI-safe):** the round-trip win and the
   push-down row-reduction — the *mechanism*, measured without a live LLM so it is exactly reproducible.
2. **Real-AI (OpenAI `gpt-4o-mini`, key from `.env`, 3 runs, K=16):** the wall-clock latency of the batched
   operator (1 round-trip) vs the per-row path (K round-trips) — the *real-world payoff*.

## Results

### Deterministic — batching (N = 1000 rows)

| Path | Round-trips | Wall time |
|---|---|---|
| `ai.if_batch('is even', array_agg(txt))` | **1** | 8.5 ms |
| per-row `ai.if_costly` over all rows | **1000** | 33.1 ms |

The batched operator issues **1 inference round-trip for 1000 rows** (proven by `ai.call_count()`), vs 1000 for
the per-row path. This is the whole point: inference is a round-trip-bound operation, so collapsing N calls into 1
is the dominant lever.

### Deterministic — dependency-safe push-down (N = 1000, cheap qual `id <= 100`)

`SELECT count(*) FROM t WHERE id <= 100 AND ai.if_costly('is even', txt)` evaluates the AI predicate on
**100 survivors**, not all 1000 (`ai.call_count() = 100`). The high `COST 100000` on `ai.if_costly` makes
Postgres's `order_qual_clauses` place the cheap `id <= 100` qual first; the `AND` short-circuits the expensive AI
on the 900 rows the cheap qual already dropped. The push-down is delegated to the planner (Rule 9 — no custom
qual-ordering machinery), and the reduction is observable through the runtime metric.

### Real-AI — latency (OpenAI `gpt-4o-mini`, K = 16, 3 runs)

| Path | Round-trips | Wall time (mean) |
|---|---|---|
| `ai.if_batch` (batched) | **1** | **1.0 s** |
| per-row `ai.if_costly` | **16** | **12.3 s** |
| **Latency ratio** | | **≈ 12.2×** |

Two independent bench runs measured **12.17×** and **11.81×** — consistent. On a real model the win is larger
than the deterministic wall-time ratio because each HTTP round-trip carries real network + model latency that the
batched path pays once instead of K times.

## Honest ceiling (ADR D4)

- This is a **composability / round-trip win with STATISTICAL accuracy** — **orthogonal to vector recall**. It does
  not make TheoDB "faster at vectors"; it makes AI-predicate queries issue O(1) round-trips instead of O(N).
- **Correctness of the mechanism** (batched == per-row, exactly 1 round-trip, NULL→NULL, push-down reduction) is
  proven deterministically by the `parity` test model (4 `pg_test`s, GREEN) — not by a flaky live-LLM assertion.
- **Real-AI answer quality** is a per-model statistical question. The batched and per-row prompts use different
  system framings, so their answers are **not asserted identical** on a live model — that comparison (a LOTUS-style
  proxy/oracle cascade with a recall guarantee) is the ambitious follow-up (plan Unresolved Questions), not a
  slice-1 claim.
- **Not measured:** end-to-end throughput under concurrency; cost-model calibration from sampled telemetry (the
  `COST` is a fixed high constant today, sufficient for the qual-ordering push-down but not a learned 3-axis model).

## Reproduce

```bash
# on the droplet, after `cargo pgrx install --features pg17 --no-default-features`
cd theodb_rs/isolation && bash bench_m102.sh          # deterministic always; real-AI when OPENAI_API_KEY is in .env
```
