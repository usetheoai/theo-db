# Blueprint: M33 — head-to-head vs AlloyDB/ScaNN (the North Star superiority claim)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS — feasibility confirmed EMPIRICALLY (scann 1.4.2 installs + builds
> an index on this AVX2 host; theodb/pgvector numbers exist in the M34 SIFT1M artifact). Discovery method:
> empirical feasibility probe + the M33 DoD's own sanctioned fallback (ScaNN OSS when AlloyDB access is blocked).

**Slug:** `m33-scann-headtohead` · **Owner:** paulohenriquevn · **Created:** 2026-07-02

## Context

M33 closes the North Star pillar: a MEASURED head-to-head vs the SOTA vector target ("igual ou superior ao AlloyDB
no vetorial"). **AlloyDB is a Google-Cloud MANAGED service** — its `alloydb_scann` index is proprietary and cannot
run in a local container. The M33 DoD explicitly sanctions the fallback: *"AlloyDB ScaNN, **ou ScaNN standalone se o
acesso ao AlloyDB for bloqueado**"*. **ScaNN** (Google Research, `pip install scann`, Apache-2.0) is the OPEN-SOURCE
implementation of the exact algorithm AlloyDB's vector index is built on (anisotropic vector quantization + SOAR
partitioning; Guo et al., ICML 2020, arXiv:1908.10396). So M33 benchmarks theodb's vector index vs **ScaNN OSS** on
the same SIFT1M dataset + hardware, producing the honest SUPERIOR / PARITY / GAP verdict per dimension.

## Coverage Corner 1 — Integration Tests

The deliverable is a measured artifact, not a unit-tested feature. The gate: a committed
`benchmarks/run_m33_scann.py` that builds a ScaNN index on SIFT1M, measures recall@10 + QPS + p50/p95/p99 +
build-time + peak-RSS across a `num_leaves_to_search` sweep, and emits `docs/benchmarks/m33-scann-headtohead.{md,json}`
comparing against theodb_ivfflat + pgvector ivfflat (numbers reused from the M34 SIFT1M artifact — same dataset,
same hardware, same neighbors-GT). A CI-safe unit test asserts the recall-computation helper matches theodb_bench's
`recall_at_k` semantics (distance-thresholded, ANN-Benchmarks) so the two sides are scored identically.

## Coverage Corner 2 — Dependencies

**New dev-only dependency: `scann` (1.4.2, Apache-2.0, Google Research).** Pulls `numpy` 2.x (verified compatible
with `theodb_bench` — recall math + 29 harness tests green under numpy 2.2.6). Runs `/deps-audit` (dev tooling, not
shipped in the extension). SIFT1M HDF5 already cached. ScaNN needs AVX2 (host has it — confirmed).

## Coverage Corner 3 — Tools

`scann.scann_ops_pybind.builder(data, k, "squared_l2").tree(num_leaves, num_leaves_to_search, training_sample_size)
.score_ah(2, ...) / .score_brute_force().build()` — build; `.search_batched(queries)` → (neighbors, distances).
recall vs the SIFT1M HDF5 `neighbors` GT. `resource.getrusage`/`tracemalloc` for peak RSS; `time.perf_counter` for
QPS/latency. The theodb + pgvector rows are read from `docs/benchmarks/m34-ivfflat-reloption.json`.

## Coverage Corner 4 — Techniques

**T1 — the honest apples-to-oranges caveat (the load-bearing scientific-honesty point).** ScaNN is a **pure
in-memory ANN library** — no persistence, no transactions, no SQL, no concurrent writers, no crash recovery.
theodb_ivfflat is a **persistent, transactional PostgreSQL index** (WAL, MVCC, INSERT/DELETE/VACUUM). Comparing
raw search speed is apples-to-apples on the *vector-search-algorithm* axis but apples-to-ORANGES on the *database*
axis. The verdict MUST state both: theodb may show a GAP on raw QPS (ScaNN is a specialized, quantization-heavy,
SIMD-tuned library) while delivering a categorically different product (vector search *inside* a real database).
Per `analysis-golden-rule` this caveat is mandatory (like "disk-backed vs in-memory").

**T2 — ScaNN's algorithmic edge (why a GAP is the likely honest outcome).** ScaNN uses **anisotropic vector
quantization** (a loss that preserves inner-product ranking under compression) + **SOAR** (spilling to a 2nd
partition) + AVX-optimized asymmetric-hashing distance. theodb_ivfflat is a straightforward IVFFlat (full-precision
lists, no learned quantization). So ScaNN is expected to reach a given recall at higher QPS. M33 MEASURES the gap
(honest number), it does not assume it. The DoD accommodates GAP explicitly.

**T3 — matched methodology (no cherry-pick).** Same SIFT1M (1M×128, Euclidean), same 1000-query subsample (seed 42),
same neighbors-GT, same recall@10 definition (distance-thresholded), same hardware, single-thread where possible.
Report the FULL recall-QPS frontier for each system (ScaNN's leaves-to-search sweep, theodb's probes sweep,
pgvector's probes sweep). Memory: ScaNN's in-memory footprint vs theodb's index bytes (already measured).

**T4 — the public-copy gate (the milestone's real output).** Per `public-copy.md`, no vector performance claim is
allowed without a linked reproducible benchmark. M33's artifact is that benchmark. If theodb is at parity/superior
on a dimension → a qualified claim becomes permitted (linked). If theodb shows a GAP → the honest status is
`UNBENCHMARKED`→`benchmarked-with-gap` / `meta` (the claim stays aspirational, marked). Either way the North Star
pillar gets its honest measured answer.

## Cross-cutting Comparison

| | theodb_ivfflat (M34) | pgvector ivfflat (M34) | ScaNN OSS (M33) | AlloyDB ScaNN |
|---|---|---|---|---|
| What | persistent PG index | persistent PG index | in-memory ANN lib | managed PG index (proprietary) |
| Algorithm | IVFFlat full-precision | IVFFlat full-precision | anisotropic-quant + SOAR | ScaNN (managed) |
| Local run | ✓ (container) | ✓ | ✓ (pip) | ✗ (GCP-only) |
| Measured here | reuse M34 artifact | reuse M34 artifact | NEW (this milestone) | — (access blocked; ScaNN OSS is the proxy) |

## ADRs

### D1 — ScaNN OSS is the AlloyDB proxy (access blocked)
AlloyDB is GCP-managed; no local/CI run. The DoD sanctions ScaNN OSS as the fallback — it IS the algorithm behind
AlloyDB's index. Rejected: provisioning AlloyDB on GCP (cost, credentials, non-reproducible in CI, out of the
container-based methodology every prior milestone used).

### D2 — reuse the M34 theodb/pgvector numbers (do NOT re-run)
The M34 artifact measured theodb_ivfflat + pgvector ivfflat on the SAME SIFT1M, hardware, neighbors-GT. Re-running
would only add noise. M33 adds the ScaNN column + the consolidated verdict. Rejected: a fresh 3-way run (the theodb
lists=1000 build is ~10 min single-thread; no value over reusing the committed, reproducible M34 numbers).

### D3 — honest GAP is a valid, DoD-sanctioned outcome
The North Star claim may be REFUTED by the data (theodb slower than ScaNN's raw algorithm). Per Rule 3 + the DoD
("sustém OU refuta honestamente"), a measured GAP with the caveat (library vs database) is a complete milestone —
NOT a failure. Rejected: tuning theodb until it "wins" (that would be M35+ engine work / cherry-picking).

## Recommendations

1. `/deps-audit` on `scann` (license Apache-2.0 OK; CVE scan).
2. `benchmarks/run_m33_scann.py`: build ScaNN on SIFT1M full-train, sweep `num_leaves_to_search`, measure
   recall@10 (vs neighbors-GT) + QPS + p50/p95/p99 + build-time + peak RSS.
3. Consolidate with the M34 theodb/pgvector rows → `docs/benchmarks/m33-scann-headtohead.{md,json}` with the
   per-dimension SUPERIOR/PARITY/GAP verdict + the mandatory library-vs-database caveat.
4. Update the North Star status honestly (public-copy gate) — a qualified, benchmark-linked statement or a marked
   `meta`/gap.
