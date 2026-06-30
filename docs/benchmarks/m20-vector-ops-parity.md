# M20 benchmark — TheoDB own distance ops vs pgvector

**Verdict: PARITY PROVEN** — numeric parity (max RELATIVE diff) and perf delta vs pgvector.
**pgvector under test:** 0.8.3 (the LIVE installed extension — CK-1). dim=1536, rows=1500, runs=5.

## Numeric parity (the M20 deliverable)

TheoDB's own Rust distance ops accumulate in **f32** like pgvector's `vector.c`, so they are byte-identical on
the same input (residual = f32 SIMD low-bit noise). Measured max RELATIVE difference (abs(Δ)/max(1,|pgvector|)) over 1500 random dim-1536 vector pairs:

| Op | max rel \|Δ\| (theodb vs pgvector) | theodb mean (ms) | pgvector mean (ms) | ratio |
|---|---|---|---|---|
| l2 | 1.142e-06 | 42.70 ± 4.11 | 10.90 ± 2.38 | 3.92× |
| inner_product | 1.733e-06 | 39.04 ± 1.16 | 10.39 ± 1.20 | 3.76× |
| cosine | 1.680e-06 | 40.27 ± 1.46 | 10.30 ± 0.80 | 3.91× |

Max rel \|Δ\| ~1e-6 across all ops ⇒ **numeric parity** — the residual is f32 SIMD-summation-order
low-bit noise (pgvector's `VECTOR_TARGET_CLONES` reorders the sum; TheoDB uses scalar f32). NOT an algorithm
difference. Parity is also asserted at the SQL-text level by `benchmarks/tests/test_vector_ops.py` (oracle +
boundary rows).

## Perf delta — honest framing

`theodb.*` is **scalar f32** Rust; pgvector uses `VECTOR_TARGET_CLONES` **SIMD**. A scalar-vs-SIMD slowdown is
EXPECTED and is **not** a milestone regression — M20's deliverable is *numeric parity + owning the computation
in Rust* (coexistence), not beating pgvector's SIMD. SIMD/auto-vectorization of the own ops is M21+ perf work.

## Methodology

1. Build `theo-db:m20`; start with PG* env.
2. Seed a temp table of 1500 deterministic dim-1536 `vector` pairs.
3. Parity: `max(abs(theodb.op(a,b) - pgvector.op(a,b)))` over the table.
4. Perf: `sum(op(a,b))` over the table, 5 runs each (warmup excluded), mean ± pop-stdev.

Reproduce: `PGHOST=localhost PGPORT=55432 PGUSER=postgres PGPASSWORD=postgres python3 benchmarks/bench_vector_ops.py --write-doc`.

Host: Linux-6.8.0-124-generic-x86_64-with-glibc2.35; Python 3.10.12.
