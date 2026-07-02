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

/// Register `theodb_ivfflat.probes` + `theodb_hnsw.ef_search`. Called once from `_PG_init`.
pub(crate) fn init() {
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
