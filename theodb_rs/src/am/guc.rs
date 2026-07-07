//! M34 — the `theodb_ivfflat` scan-time GUC: `SET theodb_ivfflat.probes = N`.
//!
//! `probes` is the per-QUERY recall/speed knob (how many of the nearest lists a scan reads), so it is a GUC (tunable
//! per session without rebuilding), NOT a build reloption — mirroring pgvector's `ivfflat.probes`. Copies the pgrx
//! 0.16.1 `GucRegistry` pattern from pgvectorscale (`access_method/guc.rs`) — Unbreakable Rule 9.
//!
//! Default preserves M26/M31 behavior: unset → `DEFAULT_PROBES` (10). The structured scan clamps the value to the
//! actual list count, so an over-large `probes` is a safe no-op.
use pgrx::{GucContext, GucFlags, GucRegistry, GucSetting};

/// Default probes when the GUC is unset — identical to the pre-M34 fixed `SCAN_PROBES`, so an untuned scan behaves
/// exactly as before.
pub(crate) const DEFAULT_PROBES: i32 = 10;
const MIN_PROBES: i32 = 1;
const MAX_PROBES: i32 = 32768; // matches the lists reloption ceiling

pub(crate) static PROBES: GucSetting<i32> = GucSetting::<i32>::new(DEFAULT_PROBES);

/// M35 — the `theodb_hnsw` scan-time recall/speed knob: `SET theodb_hnsw.ef_search = N` (pgvector's
/// `hnsw.ef_search`). Default preserves the pre-M35 fixed `SCAN_EF` (64), so an untuned scan behaves as before.
pub(crate) const DEFAULT_EF_SEARCH: i32 = 64;
const MIN_EF_SEARCH: i32 = 1;
const MAX_EF_SEARCH: i32 = 1000; // pgvector's hnsw.ef_search ceiling

pub(crate) static EF_SEARCH: GucSetting<i32> = GucSetting::<i32>::new(DEFAULT_EF_SEARCH);

/// M51 — `SET theodb_hnsw.over_fetch = N`: for an SBQ index, widen the Hamming-ranked candidate pool by ×N before
/// the exact f32 rerank, so the true NN survives the approximate ranking (recall recovery, M40). Default 1 (the
/// `ef_search` pool is reranked as-is); higher trades scan cost for recall. No effect on a v1 f32-only index.
pub(crate) const DEFAULT_OVER_FETCH: i32 = 1;
const MIN_OVER_FETCH: i32 = 1;
const MAX_OVER_FETCH: i32 = 64;
pub(crate) static OVER_FETCH: GucSetting<i32> = GucSetting::<i32>::new(DEFAULT_OVER_FETCH);

/// M52 — `SET theodb_hnsw.max_scan_tuples = N`: the iterative-scan ceiling. Under a selective `WHERE`, the
/// executor keeps pulling tuples the filter rejects; the `theodb_hnsw` scan then re-searches with a growing `ef`
/// (RELAXED order, pgvector-0.8 style) until `max_scan_tuples` distinct candidates have been emitted, preserving
/// recall under the filter. `0` = iterative scan OFF (the pre-M52 behavior: at most `ef_search` tuples).
///
/// DELIBERATE DIVERGENCE from pgvector: pgvector gates iterative scan behind a SEPARATE `hnsw.iterative_scan` GUC
/// that defaults to `off`; here a non-zero `max_scan_tuples` (default 20000) enables it directly, so theodb's
/// iterative scan is ON by default. Rationale: filtered ANN with preserved recall is the North-Star behavior;
/// unfiltered `LIMIT k` (k ≤ ef_search) never triggers the grow, so there is no unfiltered regression. Set 0 to
/// reproduce pgvector's default-OFF semantics.
pub(crate) const DEFAULT_MAX_SCAN_TUPLES: i32 = 20000;
pub(crate) static MAX_SCAN_TUPLES: GucSetting<i32> = GucSetting::<i32>::new(DEFAULT_MAX_SCAN_TUPLES);

/// M48 (T3.1) — `SET theodb.vacuum_pending_threshold = N`: a VACUUM folds the pending region into the main
/// structure when it exceeds N pages, even with zero dead tuples, so an insert-only workload's scan returns to
/// O(structure) instead of paying O(pending) forever. Operational knob (Userset), NOT a build reloption. Default
/// 16 is an educated guess; the M48 benchmark (T6.1) measures the scan degradation per pending page.
pub(crate) const DEFAULT_VACUUM_PENDING_THRESHOLD: i32 = 16;
pub(crate) static VACUUM_PENDING_THRESHOLD: GucSetting<i32> = GucSetting::<i32>::new(DEFAULT_VACUUM_PENDING_THRESHOLD);

/// The effective pending-fold threshold in pages (never below 1).
pub(crate) fn vacuum_pending_threshold() -> u32 {
    VACUUM_PENDING_THRESHOLD.get().max(1) as u32
}

/// M56 — `SET theodb.hnsw_tombstone_compact_pct = N`: a VACUUM tombstones dead nodes in place (cheap, no O(N),
/// no stall); once tombstones reach N% of the graph, the same VACUUM ALSO runs the (rare, O(N)) compaction fold
/// to reclaim their space and re-densify. Default 20%. `0` disables ratio-compaction (only pending-threshold
/// folds). This is the knob that trades delete-latency (low) against index bloat between compactions.
pub(crate) const DEFAULT_HNSW_TOMBSTONE_COMPACT_PCT: i32 = 20;
pub(crate) static HNSW_TOMBSTONE_COMPACT_PCT: GucSetting<i32> =
    GucSetting::<i32>::new(DEFAULT_HNSW_TOMBSTONE_COMPACT_PCT);

/// The effective tombstone-compaction percentage (0..=100; 0 = disabled).
pub(crate) fn hnsw_tombstone_compact_pct() -> i32 {
    HNSW_TOMBSTONE_COMPACT_PCT.get().clamp(0, 100)
}

/// M56 fase 2 — `SET theodb.hnsw_slot_reuse = on|off`: when ON (default), `aminsert` REUSES a tombstoned element
/// slot via a proper in-place insert (search + link) before growing the pending region, bounding relation growth
/// under DELETE+INSERT churn. OFF forces the legacy pending-append path (the kill-switch, and the A/B toggle the
/// churn benchmark uses to isolate the slot-reuse effect).
pub(crate) static HNSW_SLOT_REUSE: GucSetting<bool> = GucSetting::<bool>::new(true);

/// Whether `aminsert` should reuse tombstoned slots in place (M56 fase 2).
pub(crate) fn hnsw_slot_reuse() -> bool {
    HNSW_SLOT_REUSE.get()
}

// M48 (T2.3) — deterministic crash-injection for the VACUUM fold's crash tests. `injection_points` is NOT
// compiled into the packaged Debian PG17 (blueprint §Q9, verified), so we ship a tiny always-compiled test hook
// instead. Both default to 0 (off) ⇒ ZERO effect in production; both are `Suset` (only a superuser can set them,
// stricter than the `Userset` scan GUCs above — a conscious divergence: this is a fault-injection knob, ADR D6).
pub(crate) static TEST_CRASH_AFTER_PAGES: GucSetting<i32> = GucSetting::<i32>::new(0);
pub(crate) static TEST_CRASH_PHASE: GucSetting<i32> = GucSetting::<i32>::new(0);

/// Phase selector values for [`TEST_CRASH_PHASE`].
pub(crate) const CRASH_PHASE_POST_PIVOT: i32 = 1; // after block 0 is pivoted, before reclaim
pub(crate) const CRASH_PHASE_MID_RECLAIM: i32 = 2; // after the first reclaim (leftover-empty) page

/// Crash the backend right after committing the `pages_written`-th fold body page, IFF it exactly equals the
/// GUC (strict `==`; default 0 ⇒ never fires in production). `std::process::abort()` (SIGABRT) is a REAL backend
/// crash — the postmaster runs crash recovery + WAL replay — unlike `proc_exit`, which runs a clean shutdown and
/// would not exercise the recovery path. Must be called AFTER the page's `GenericXLogFinish` so the WAL record
/// exists (else the test is racy). See ADR 0014 / blueprint §Q9.
pub(crate) fn maybe_crash_after_body_page(pages_written: u32) {
    // SECURITY: abort() is instance-wide, not backend-local — the postmaster treats a SIGABRT'd backend as a
    // crash and terminates ALL backends + runs crash recovery. pgrx 0.16.1 does NOT enforce the GUC's `Suset`
    // context for a custom GUC, so without THIS guard any non-superuser could `SET … ; VACUUM idx` and DoS the
    // whole instance. Gate the actual abort on `superuser()` so the always-compiled test hook is unreachable
    // by ordinary roles (the crash tests connect as `postgres`, so they stay green).
    if !unsafe { pgrx::pg_sys::superuser() } {
        return;
    }
    let g = TEST_CRASH_AFTER_PAGES.get();
    if g > 0 && pages_written == g as u32 {
        std::process::abort();
    }
}

/// Crash the backend at a named fold phase (post-pivot / mid-reclaim), IFF the GUC selects it. Default 0 = off.
/// Superuser-gated for the same instance-wide-DoS reason as [`maybe_crash_after_body_page`].
pub(crate) fn maybe_crash_at_phase(phase: i32) {
    if !unsafe { pgrx::pg_sys::superuser() } {
        return;
    }
    if phase != 0 && TEST_CRASH_PHASE.get() == phase {
        std::process::abort();
    }
}

/// Register `theodb_ivfflat.probes` + `theodb_hnsw.ef_search` + the M48 test-crash GUCs. Called once from `_PG_init`.
pub(crate) fn init() {
    GucRegistry::define_int_guc(
        c"theodb.test_crash_after_pages",
        c"TEST ONLY: crash the backend after committing N VACUUM-fold body pages (0 = off)",
        c"Deterministic crash-injection for the M48 crash-safe fold tests. Superuser only. Never set in production.",
        &TEST_CRASH_AFTER_PAGES,
        0,
        1_000_000,
        GucContext::Suset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.test_crash_phase",
        c"TEST ONLY: crash the backend at a VACUUM-fold phase (0=off, 1=post-pivot, 2=mid-reclaim)",
        c"Deterministic crash-injection for the M48 crash-safe fold tests. Superuser only. Never set in production.",
        &TEST_CRASH_PHASE,
        0,
        2,
        GucContext::Suset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.vacuum_pending_threshold",
        c"VACUUM folds the theodb index pending region into the main structure above this many pages (even with 0 dead tuples)",
        c"Keeps an insert-only workload's scan at O(structure). Higher = fewer folds but slower scans between them.",
        &VACUUM_PENDING_THRESHOLD,
        1,
        65536,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.hnsw_tombstone_compact_pct",
        c"After tombstones reach this % of the theodb_hnsw graph, a VACUUM also runs the O(N) compaction fold (0 = disabled)",
        c"Deletes tombstone in place (cheap, no stall); compaction reclaims their space. Higher = less compaction but more bloat.",
        &HNSW_TOMBSTONE_COMPACT_PCT,
        0,
        100,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.hnsw_slot_reuse",
        c"When on, theodb_hnsw aminsert reuses a tombstoned slot in place (search + link) before growing pending",
        c"Bounds relation growth under DELETE+INSERT churn (M56 fase 2). Off = legacy pending-append (kill-switch / A/B).",
        &HNSW_SLOT_REUSE,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb_ivfflat.probes",
        c"Number of nearest lists a theodb_ivfflat scan reads",
        c"Higher value increases recall at the cost of speed; clamped to the index's list count.",
        &PROBES,
        MIN_PROBES,
        MAX_PROBES,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb_hnsw.ef_search",
        c"Size of the dynamic candidate list a theodb_hnsw scan keeps at the ground layer",
        c"Higher value increases recall at the cost of speed; bounds both quality and result count.",
        &EF_SEARCH,
        MIN_EF_SEARCH,
        MAX_EF_SEARCH,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb_hnsw.over_fetch",
        c"For an SBQ index, widen the Hamming candidate pool by this factor before the exact f32 rerank",
        c"Higher value increases recall on a quantized index at the cost of scan speed; 1 = rerank the ef_search pool as-is. No effect on an f32-only index.",
        &OVER_FETCH,
        MIN_OVER_FETCH,
        MAX_OVER_FETCH,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb_hnsw.max_scan_tuples",
        c"Iterative-scan ceiling: max distinct candidates a theodb_hnsw scan emits under a selective WHERE (0 = off)",
        c"Under a filter, the scan re-searches with a growing ef until this many tuples are emitted, preserving recall. 0 disables iterative scan (at most ef_search tuples).",
        &MAX_SCAN_TUPLES,
        0,
        10_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );
}

/// The effective probes for a scan: the GUC value (never below 1). The caller still clamps to the actual list count.
pub(crate) fn probes() -> usize {
    PROBES.get().max(MIN_PROBES) as usize
}

/// The effective `ef_search` for a theodb_hnsw scan (never below 1).
pub(crate) fn ef_search() -> usize {
    EF_SEARCH.get().max(MIN_EF_SEARCH) as usize
}

/// The effective SBQ `over_fetch` factor for a theodb_hnsw scan (never below 1).
pub(crate) fn over_fetch() -> usize {
    OVER_FETCH.get().max(MIN_OVER_FETCH) as usize
}

/// The iterative-scan ceiling (M52). 0 ⇒ iterative scan off. The scan grows `ef` until this many distinct
/// candidates are emitted, so a selective `WHERE` still finds its top-k (recall preserved).
pub(crate) fn max_scan_tuples() -> usize {
    MAX_SCAN_TUPLES.get().max(0) as usize
}
