//! M34 — the `theodb_ivfflat` reloption surface: `WITH (lists = N)`.
//!
//! `lists` is a BUILD parameter (the corpus is partitioned into `lists` k-means cells once, at build), so it is an
//! index reloption (parsed by `amoptions` into `rd_options`), NOT a runtime GUC. This mirrors pgvector's
//! `ivfflat` (`lists` reloption + `ivfflat.probes` GUC) and copies the pgrx 0.16.1 pattern from pgvectorscale
//! (`access_method/options.rs`) — Unbreakable Rule 9 (don't reinvent the AM options wiring). The scan-time
//! `probes` knob is a GUC (see `am/guc.rs`).
//!
//! Default preserves M26/M31 behavior: no `WITH (lists=)` → `rd_options` is null → `DEFAULT_LISTS` (100).
use pgrx::pg_sys::AsPgCStr;
use pgrx::prelude::*;

/// The default list count when `WITH (lists=)` is not given — identical to the pre-M34 fixed constant, so every
/// existing index (built without the option) is byte-for-byte unchanged.
pub(crate) const DEFAULT_LISTS: usize = 100;
const MIN_LISTS: i32 = 1;
const MAX_LISTS: i32 = 32768; // pgvector's IVFFLAT_MAX_LISTS

/// M51 — `theodb_hnsw` build knob `WITH (sbq_bits = N)`: bits-per-dim for the inline SBQ codes (0 = off / v1
/// f32-only, the default, so every existing index is byte-for-byte unchanged). 1..8 mirror the SBQ quantizer
/// range (`sbq.rs` BITS_MAX). A scan knob (over_fetch) is a GUC (`am/guc.rs`), not a reloption.
pub(crate) const DEFAULT_SBQ_BITS: i32 = 0;
const MIN_SBQ_BITS: i32 = 0;
const MAX_SBQ_BITS: i32 = 8;

/// Parsed reloptions shared by the two AMs (`amoptions` is shared — `mod.rs`). `#[repr(C)]` with the varlena
/// header first (never touch `vl_len_` directly — `build_reloptions` sets it). `lists` is an ivfflat build knob;
/// `sbq_bits` is a theodb_hnsw build knob (M51). Each AM reads only the option it uses.
#[repr(C)]
struct TheodbIvfflatOptions {
    vl_len_: i32, // varlena header (managed by build_reloptions)
    lists: i32,
    sbq_bits: i32,
}

static mut RELOPT_KIND: pg_sys::relopt_kind::Type = 0;

/// Register the `theodb_ivfflat` reloption kind + the `lists` int option. Called once from `_PG_init`.
///
/// # Safety
/// Must run exactly once at extension load (single-threaded postmaster init), before any index DDL.
pub(crate) unsafe fn init() {
    RELOPT_KIND = pg_sys::add_reloption_kind();
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "lists".as_pg_cstr(),
        "Number of inverted lists (k-means cells) for the theodb_ivfflat build".as_pg_cstr(),
        DEFAULT_LISTS as i32,
        MIN_LISTS,
        MAX_LISTS,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "sbq_bits".as_pg_cstr(),
        "Bits-per-dim for the theodb_hnsw inline SBQ codes (0 = f32-only, M51)".as_pg_cstr(),
        DEFAULT_SBQ_BITS,
        MIN_SBQ_BITS,
        MAX_SBQ_BITS,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
}

/// The `amoptions` callback: parse `pg_class.reloptions` text[] into the `TheodbIvfflatOptions` bytea that fills
/// `rd_options`. Out-of-range `lists` is rejected here (when `validate`) by `build_reloptions` against the
/// min/max registered in `init` — a typed DDL error, never a scan-time crash.
#[pg_guard]
pub(crate) unsafe extern "C-unwind" fn amoptions(
    reloptions: pg_sys::Datum,
    validate: bool,
) -> *mut pg_sys::bytea {
    let tab: [pg_sys::relopt_parse_elt; 2] = [
        pg_sys::relopt_parse_elt {
            optname: "lists".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, lists) as i32,
        },
        pg_sys::relopt_parse_elt {
            optname: "sbq_bits".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, sbq_bits) as i32,
        },
    ];
    pg_sys::build_reloptions(
        reloptions,
        validate,
        RELOPT_KIND,
        std::mem::size_of::<TheodbIvfflatOptions>(),
        tab.as_ptr(),
        tab.len() as i32,
    ) as *mut pg_sys::bytea
}

/// Resolve the build-time `lists` for an index relation: the `WITH (lists=N)` value, or `DEFAULT_LISTS` when the
/// option is absent (`rd_options` null). Used by `ambuild` and the VACUUM rebuild so a fold preserves the built
/// list count instead of silently reverting to the default.
///
/// # Safety
/// `indexrel` must be a valid open `theodb_ivfflat` index relation.
pub(crate) unsafe fn lists_from_relation(indexrel: pg_sys::Relation) -> usize {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return DEFAULT_LISTS;
    }
    let opts = rd_options as *const TheodbIvfflatOptions;
    let lists = (*opts).lists;
    if lists < MIN_LISTS {
        DEFAULT_LISTS
    } else {
        lists as usize
    }
}

/// Resolve the build-time `sbq_bits` for a `theodb_hnsw` index: the `WITH (sbq_bits=N)` value, or 0 (f32-only)
/// when the option is absent. A fold reads this off the persisted meta (not the reloption), so this is only the
/// initial-build gate.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
pub(crate) unsafe fn sbq_bits_from_relation(indexrel: pg_sys::Relation) -> u8 {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return 0;
    }
    let bits = (*(rd_options as *const TheodbIvfflatOptions)).sbq_bits;
    if (MIN_SBQ_BITS..=MAX_SBQ_BITS).contains(&bits) {
        bits as u8
    } else {
        0
    }
}
