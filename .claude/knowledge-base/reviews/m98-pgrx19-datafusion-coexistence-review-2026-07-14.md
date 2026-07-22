# Review — M98 pgrx 0.19 upgrade + DataFusion/Arrow coexistence GATE

**Date:** 2026-07-14 · **Slug:** m98-pgrx19-datafusion-coexistence · **Milestone:** M98 · **Verdict:** `READY_TO_MERGE` (fixes applied)

council-rust-pgrx reviewed the pgrx upgrade + the DataFusion FFI seam. First pass `NEEDS_FIXES` (1 BLOCKER + 2 HIGH); all fixed and re-verified.

## Findings → dispositions

| # | Sev | Finding | Disposition |
|---|---|---|---|
| B1 | BLOCKER | Committed `Cargo.lock` was stale (pinned pgrx 0.16.1, zero datafusion/arrow) — the coexistence proof (the resolved dep graph) was not in the repo, only on the droplet | **FIXED** — committed the droplet-regenerated lock: pgrx 0.19.0 + datafusion 54.0.0 + arrow-* all v58.3.0 (single major); the single-arrow-major claim is now reproducible from `develop` |
| H1 | HIGH | The `HeldInterrupts` comment misstated the hazard (`proc_exit`) — `HOLD_INTERRUPTS` defers a query-cancel `ereport(ERROR)`/siglongjmp, NOT a `proc_exit` FATAL | **FIXED** — comment rewritten to the correct mechanism (the exact invariant M100 inherits) |
| H2 | HIGH | Exception-safety hinged on `panic = "unwind"` (the guard's `Drop` restoring `InterruptHoldoffCount` only runs if the panic unwinds) | **FIXED** — `drop(held); drop(rt);` BEFORE the `error!` path, so the holdoff is restored + the runtime dropped before any unwind; no coupling to the panic strategy (the pattern M100 copies) |
| M1 | MEDIUM | Holding across the whole `block_on` is fine for a 3-row smoke but wrong as an M100 pattern (uncancellable full scan) | **FIXED** — explicit `M100 NOTE` in the module doc: hold only around the runtime hand-off, service interrupts between batches |
| M2 | MEDIUM | `TypeOrigin::External`⇄the `extension_sql!` bootstrap coupling is load-bearing + undocumented | **FIXED** — comment at the bootstrap: it is the SOLE creator of `vector` (External means pgrx does not emit CREATE TYPE) |
| M3 | MEDIUM | The GATE proves CORE-feature coexistence only (`default-features=false`); M100 needs expression/pushdown features | **DOCUMENTED** — scope caveat in `docs/benchmarks/m98-coexistence.md` |
| L1 | LOW | `cargo fix --edition` unsafe wrappings + `&raw` | ACCEPTED — spot-checked clean (semantically inert; `&raw` was pre-M69, not introduced here) |
| L2 | LOW | `unsafe impl Send` / thread-safety | ACCEPTED — current-thread runtime, no PG-touching thread pool (M100 multi-partition is out of scope) |

## Council verdict

**"The GATE is legitimately passed in substance"** — 279 tests GREEN + DataFusion-in-backend is genuinely proven; the FFI/interrupt seam is sound (query-cancel longjmp correctly held off; no path longjmps through the tokio runtime). B1 (reproducibility) + H1/H2 (the seam M100 inherits) fixed → READY_TO_MERGE.

## Gates

- **279 tests GREEN, 0 failed** on pgrx 0.19.0 + datafusion 54 + arrow 58 (droplet), re-verified after the H2 fix.
- Coexistence: `cargo tree` / `Cargo.lock` — arrow-* all v58 (single major), no ABI/version conflict. Reproducible from the committed lock.
- No page-format change; `public.vector` SQL name byte-identical (no REINDEX). `docs/benchmarks/m98-coexistence.md`.
- No commits to main; no Co-Authored-By trailer; CHANGELOG updated.

## Verdict

`READY_TO_MERGE`. The pillar GATE passed: pgrx 0.16.1→0.19.0 upgrade clean (277 tests byte-identical), DataFusion 54 + Arrow 58 coexist (single arrow major), DataFusion executes inside a PG backend under the corrected interrupt discipline. Proceed to `/release`. Honest-negative (ADR D3) was the fallback — not needed. M99-M103 unblocked.
