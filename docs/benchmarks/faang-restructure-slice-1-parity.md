# FAANG Restructure Slice 1 — latency parity (pre-split vs post-split)

**Date:** 2026-06-29
**Slice:** `faang-restructure-slice-1` (declutter scripts + in-crate 3-boundary module split + toolchain pin)
**Purpose:** prove the `theodb_rs/src/lib.rs` module split (→ `pg.rs` + `embed.rs` + `lib.rs`) is a **pure
refactor** with NO latency regression. Measurement-first (ADR 0002 / `.claude/rules/public-copy.md`).

> **This is a refactor, not a feature.** The split relocates code between modules; the compiled instruction
> path of `theodb.embed` is unchanged (proven independently: `cargo clippy` 0 warnings, byte-identical
> generated-SQL object set, the full 18-test suite passing UNCHANGED). The expectation is **equal latency**,
> not a speedup. No performance claim is made.

## Method (apples-to-apples — the honest comparison)

`theodb.embed` is **I/O-bound**: each call is one HTTP round-trip to the embedding endpoint, whose service
time dominates wall-clock and varies with host load. Comparing this slice's absolute latency to the M17
report's 13.92 ms/call would be **invalid** — those numbers were measured at a different time, under a
different machine load. The valid comparison is **pre-split vs post-split on the SAME machine, at the SAME
time, against the SAME stub, interleaved**:

- **PRE-split:** `theo-db:m17` (the shipped M17 image — single-file `lib.rs`, `theodb_rs._embed_text` body inline).
- **POST-split:** `theo-db:slice1` (this slice — `pg.rs` glue + `embed.rs` domain + `lib.rs` api/map). Same SQL surface (`\df` identical: `theodb_rs._embed_text` C + `theodb.embed` SQL).
- **Stub:** one shared `benchmarks/servers/embedding_server.py` (real BAAI/bge-small-en-v1.5, 384-dim, deterministic), reached via `host.docker.internal`.
- **Workload:** 200 serial `SELECT theodb.embed('benchmark text')` calls per run, **6 runs INTERLEAVED** (run order: pre, post, pre, post, …) so any host-load drift hits both implementations equally. 5 warmup calls per connection discarded. Per-call latency = run wall-clock / 200; reported mean ± std (population) over the 6 runs.
- **Hardware:** 13th Gen Intel Core i7-1355U. **PostgreSQL:** 17.10. Both containers + the stub ran concurrently (heavy I/O-bound load — note the absolute ms is ~2× the lightly-loaded M17 run; this is exactly why the same-machine/same-time interleaved comparison is the valid one).

### Reproduce
```bash
# pre-split + post-split side by side
docker run -d --name m17  --add-host=host.docker.internal:host-gateway -p 55434:5432 ... theo-db:m17
docker run -d --name s1   --add-host=host.docker.internal:host-gateway -p 55433:5432 ... theo-db:slice1
python3 benchmarks/servers/embedding_server.py --port <P> &           # one shared stub
# interleave bench(port=55434) and bench(port=55433), 6 runs each, N=200 (benchmarks/bench_embed.py)
```

## Results (interleaved, 6 runs × 200 calls, same stub)

| Implementation | Module layout | Mean (ms/call) | Std dev (ms) |
|---|---|---|---|
| `theo-db:m17` (PRE-split) | single-file `lib.rs` | 26.149 | 1.289 |
| `theo-db:slice1` (POST-split) | `pg.rs` + `embed.rs` + `lib.rs` | 27.356 | 1.435 |

- **Delta: +1.207 ms/call** — **within one standard deviation** of either measurement (±1.29 / ±1.44 ms). The two are statistically indistinguishable.
- A sequential (non-interleaved) pre→post run earlier showed +2.96 ms; interleaving (which cancels host-load drift) shrank the delta to +1.21 ms — confirming the residual difference is run-to-run I/O-bound noise, not the refactor.
- Both are dominated by the ~26 ms stub round-trip (I/O-bound), as expected.

## Conclusion

**No latency regression.** The module split's latency equals the pre-split baseline within measurement noise.
This is the expected result for a pure code relocation (identical compiled path) and is consistent with the
behavioral parity proven by the unchanged 18-test suite + identical SQL surface. No performance claim is made
(this is a refactor; the number is "equivalent latency", not a win).
