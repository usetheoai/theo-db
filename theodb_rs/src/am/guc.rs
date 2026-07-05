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
}

/// The effective probes for a scan: the GUC value (never below 1). The caller still clamps to the actual list count.
pub(crate) fn probes() -> usize {
    PROBES.get().max(MIN_PROBES) as usize
}

/// The effective `ef_search` for a theodb_hnsw scan (never below 1).
pub(crate) fn ef_search() -> usize {
    EF_SEARCH.get().max(MIN_EF_SEARCH) as usize
}
