# M46 — follow-ups (out of scope for this milestone, surfaced by /review)

Objective findings from the M46 council review that are real but beyond the milestone's scope. Logged here
because the project's issue tracker is not yet configured (see CHANGELOG note).

## FU-1 (HIGH, from council-benchmark) — the QPS verdict needs a SAME-GRAPH harness

**Finding:** the two-container baseline-vs-post A/B cannot cleanly attribute an allocation-only scan change to
QPS, because at any `n > 4096` the M44 parallel build (`ann/hnsw.rs:34`) races and the two containers build
*different graphs*. A quiet box removes the load confound but NOT the graph-difference confound. This run's
recall/pages_read deltas (<0.3%) are that build-race noise.

**Fix (next milestone, not M46):** measure the L1-A/L1-B effect on a **byte-identical graph**. Preferred: a Rust
`criterion` micro-bench over one `HnswIndex::build(seed=42)` graph, comparing pre-size vs `::new()` — no
container, no box-load noise, no build race, cleanest isolation. Alternative: persist one theodb_hnsw index and
restore it into both binaries. Run at SIFT1M scale (where the ~44% ef≥200 variance regime appears). This is the
reproducible artifact for the win/variance verdict deferred by M46.

**Why not now:** M46's scope (ADR-1) is the recall-neutral code change + measurement-first re-measurement; the
same-graph micro-bench harness is new infrastructure (a criterion suite + a graph-snapshot fixture) that warrants
its own plan. M46 ships the correct code + the honest-negative; FU-1 delivers the clean win measurement.

## FU-2 (LOW, from council-rust-pgrx) — optional `cap` clamp against corrupt-meta memory amplification

**Finding:** `cap = ef.saturating_mul(m0.max(1)).max(1)` with `m0` read from on-disk meta (`u16`, up to 65535)
means a *corrupt* meta could drive a large speculative `with_capacity` (~1.2 GB worst case — bounded, no panic,
never OOM-abort under `isize::MAX`, but a memory amplification from untrusted bytes).

**Fix (optional hardening):** `cap.min(<reasonable ceiling>)` — e.g. clamp to `ef * MAX_M0`. Consistent with the
"corrupt meta → bounded, never crash" invariant. Not a defect (the value is bounded and no real index has a
pathological `m0`); a defense-in-depth nicety. Defer unless a fuzzing pass surfaces it.
