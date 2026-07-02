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

/// Register `theodb_ivfflat.probes`. Called once from `_PG_init`.
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
}

/// The effective probes for a scan: the GUC value (never below 1). The caller still clamps to the actual list count.
pub(crate) fn probes() -> usize {
    PROBES.get().max(MIN_PROBES) as usize
}
