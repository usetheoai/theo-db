# Vendored code — RaBitQ core from `rabitq-rs`

This directory contains code **vendored verbatim** (then adapted for integration) from the
third-party project **`rabitq-rs`**, used under its permissive license.

## Provenance

| Field | Value |
|---|---|
| Upstream | https://github.com/lqhl/rabitq-rs |
| Vendored commit | `10b9a4e` |
| Vendored on | 2026-07-10 |
| Upstream license | **Apache License 2.0** (see `LICENSE` in this directory) |
| TheoDB license | Apache License 2.0 — **compatible** (D1: `PRD §11`, `docs/adr/0006`) |
| Algorithm origin | RaBitQ (Gao & Long, arXiv:2405.12497); canonical impl: `VectorDB-NTU/RaBitQ-Library` (Apache-2.0) |

## What is vendored (the ALGORITHM CORE — not the whole crate)

| File | Purpose |
|---|---|
| `quantizer.rs` | RaBitQ quantizer — 1-bit binary + multi-bit refinement codes, with theoretical error bound |
| `rotation.rs` | Fast Hadamard Transform (FHT) random orthonormal rotation (precondition for RaBitQ) |
| `fastscan.rs` + `fastscan_kernel.rs` | FastScan batch asymmetric distance (XNOR + popcount LUT over packed codes) |
| `simd.rs` | SIMD distance kernels (x86_64 AVX2/AVX-512; runtime-dispatched) |
| `math.rs` | scalar math helpers (dot, norm, subtract, feature detection) |

## What is NOT vendored (replaced by TheoDB's own infrastructure)

- `ivf.rs` (upstream's file/mmap-based IVF storage) → **replaced** by TheoDB's page-native IVF
  (`theodb_rs/src/ann/ivf.rs` + the `theodb_ivfflat` AM: k-means++ centroids, inverted lists, WAL, buffer manager).
- `mstg/*`, `io.rs`, `memory.rs`, `python_bindings.rs`, `kmeans.rs` (we reuse our own k-means) — not needed.

## Modifications from upstream

Integration edits (import paths `crate::…` → `crate::rabitq::vendor::…`, `Metric`/error-type adaptation to
TheoDB's types, removal of file-I/O couplings) are tracked in git history and summarized in
`docs/adr/0032-vendor-rabitq-rs-core.md`. The Apache-2.0 grant permits modification with attribution; this file +
the retained `LICENSE` constitute the required notice.
