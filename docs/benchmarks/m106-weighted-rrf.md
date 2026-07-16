# M106 — Weighted RRF: measured ranking-flip evidence

**Feature:** per-leg weights (`vector_weight`/`text_weight`) on the `ai.hybrid_search(jsonb)` RRF fusion.
**Claim:** the weights measurably change the fused ranking (moving a documented-but-unshipped capability into shipped), and default `1.0/1.0` is byte-identical to the prior unweighted fusion.

## Formula

```
score(d) = vector_weight · 1/(k + rank_vec(d)) + text_weight · 1/(k + rank_fts(d))
```

Weights are validated **finite and ≥ 0** at the boundary (typed `22023` on a negative weight; `0.0` disables a leg). Default `1.0/1.0` ⇒ the pre-M106 pure RRF (`score = 1/(k+rank_vec) + 1/(k+rank_fts)`).

## Measurement (deterministic, reproducible)

Fixture — two SINGLE-LEG docs so the weight alone decides the winner (`k=60`):

| doc | vector leg | FTS leg |
|---|---|---|
| `dv` | rank 1 (`embedding=[1,0,0]`, query_vector `[1,0,0]`) | absent (no `database` term) |
| `df` | absent (`embedding IS NULL`) | rank 1 (`database`×3, query_text `database`) |

| Config | `dv` score | `df` score | Top-1 |
|---|---|---|---|
| `vector_weight=1, text_weight=1` (default) | 1/61 = 0.01639 | 1/61 = 0.01639 | tie → `df` (id asc) |
| **`vector_weight=3, text_weight=1`** | 3/61 = 0.04918 | 1/61 = 0.01639 | **`dv`** ✅ |
| **`vector_weight=1, text_weight=3`** | 1/61 = 0.01639 | 3/61 = 0.04918 | **`df`** ✅ (ranking flipped) |
| `vector_weight=-1` | — | — | typed error `22023` ✅ |

**Result: the weight measurably flips the top result** — upweighting the vector leg lifts `dv` to #1; upweighting the text leg flips it to `df`. This is the ranking control the docs promised (audit gap 06), now shipped.

## Reproduction

Rust pg_tests (`cargo pgrx test pg17 m106`, on a pgrx-0.19 / PG17 host, non-root):

- `hybrid::tests::m106_vector_weight_lifts_vector_leg_top` — asserts `vector_weight=3` → top-1 `dv`.
- `hybrid::tests::m106_text_weight_flips_ranking_to_fts_leg_top` — asserts `text_weight=3` → top-1 `df`.
- `hybrid::tests::m106_negative_weight_rejected` — asserts a negative weight raises.

Offline twin (`pytest benchmarks/tests/test_hybrid.py`):

- `test_rrf_fuse_default_weights_equal_unweighted` — `weights=[1,1]` == no weights (backward-compat).
- `test_rrf_fuse_weight_changes_order` — upweighting a leg lifts its top doc.
- `test_rrf_fuse_zero_weight_disables_leg`, `test_rrf_fuse_rejects_negative_weight`.

SQL-level integration (`test_integration.py::test_hybrid_search_json_weight_changes_ranking` /
`_negative_weight_raises`).

**Measured host:** DigitalOcean `s-4vcpu-8gb` (fra1), pgrx 0.19.0 / PostgreSQL 17.10. Full suite: **324 pg_tests GREEN** (321 prior + 3 M106), 0 regression — the default-weight path preserves every existing hybrid test.
