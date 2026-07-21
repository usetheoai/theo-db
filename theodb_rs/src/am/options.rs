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

/// M59 — `theodb_hnsw` build knob `WITH (pq_subspaces = M)`: number of anisotropic-PQ subspaces (0 = AQ off /
/// v1|v2, the default, so every existing index is byte-for-byte unchanged). `> 0` enables meta layout v3: an
/// `AqQuantizer` trained at build + `⌈m/2⌉`-byte codes inline. `dim % m == 0` is checked at build (typed error),
/// not here. A scan knob (over_fetch) is reused as the AH rerank pool (GUC), not a reloption.
pub(crate) const DEFAULT_PQ_SUBSPACES: i32 = 0;
const MIN_PQ_SUBSPACES: i32 = 0;
const MAX_PQ_SUBSPACES: i32 = 2048; // one 4-bit code per subspace; 2048 covers the largest realistic dim/1.

/// M59 — `WITH (pq_bits = N)`: bits per subquantizer. Only 4 is supported (the LUT16 `pshufb` sweet spot, ADR
/// D3); the default 4 makes `WITH (pq_subspaces=M)` alone enable AQ. Non-4 is rejected at build (typed error).
pub(crate) const DEFAULT_PQ_BITS: i32 = 4;
const MIN_PQ_BITS: i32 = 4;
const MAX_PQ_BITS: i32 = 4;

/// M59 — `WITH (aq_threshold = N)`: the anisotropic parallel/orthogonal weight ratio `η`, stored **milli-scaled**
/// (`η × 1000`) so a single int reloption carries the float knob (KISS — no second reloption type). Default 1000
/// (`η = 1.0`, isotropic). Clamped to `≥ 1000` at resolve time (`η < 1` is meaningless — `aq.rs` clamps too).
pub(crate) const DEFAULT_AQ_THRESHOLD_MILLI: i32 = 1000;
const MIN_AQ_THRESHOLD_MILLI: i32 = 1000;
const MAX_AQ_THRESHOLD_MILLI: i32 = 1_000_000; // η up to 1000×; far above any useful ScaNN T.

/// M83 (Roadmap v7 D3 spike) — `WITH (separate_storage = 1)`: on an AQ index (`pq_subspaces > 0`), persist the
/// v5 STORAGE-SEPARATED layout (codes and f32 on distinct page ranges) instead of the v4 interleaved layout, so
/// the scan reads only the compact codes for pruning and random-reads f32 only for rerank survivors (ADR-0037
/// lever). 0 = v4 interleaved (default, byte-identical). Int 0/1 (KISS — matches the other int reloptions).
pub(crate) const DEFAULT_SEPARATE_STORAGE: i32 = 0;
const MIN_SEPARATE_STORAGE: i32 = 0;
const MAX_SEPARATE_STORAGE: i32 = 1;

/// M85 (Roadmap v7) — `WITH (refine = 1)`: on a storage-separated AQ index (`separate_storage=1`), persist the v6
/// SQ8-refine layout — the per-list rerank region is SQ8 codes (`dim` B/vec) instead of raw f32 (`dim·4` B/vec),
/// so Stage-2 survivor reads shrink 4× at the high-recall frontier (M84). 0 = v5 f32 rerank (default, exact). Int
/// 0/1 (0=f32, 1=sq8 — KISS, matches the other int reloptions). Only meaningful with `separate_storage=1`.
pub(crate) const DEFAULT_REFINE: i32 = 0;
const MIN_REFINE: i32 = 0;
const MAX_REFINE: i32 = 2; // 0=f32 (v5), 1=sq8 (v6), 2=rabitq (v8, f32-free residual rerank)

/// Vector-research E1 — `WITH (rabitq_bits = N)`: bits-per-dim for the v8 (`refine=2`) extended-multi-bit RaBitQ
/// residual rerank codes. 7 = the paper's f32-free-recall sweet spot (arXiv:2409.09913 — 99% recall with no raw
/// vector access). Only meaningful with `refine=2`.
pub(crate) const DEFAULT_RABITQ_BITS: i32 = 7;
const MIN_RABITQ_BITS: i32 = 1;
const MAX_RABITQ_BITS: i32 = 8;

/// Vector-research E2 — `WITH (degree_bound = R)`: the co-located `theodb_symqg` graph's per-vertex out-degree
/// (a multiple of 32 for FastScan alignment). 32 = HNSW base-layer m0 (no truncation). Larger R → higher recall +
/// bigger rows. The reader rounds a non-multiple-of-32 UP.
pub(crate) const DEFAULT_SYMQG_DEGREE: i32 = 32;
const MIN_SYMQG_DEGREE: i32 = 32;
const MAX_SYMQG_DEGREE: i32 = 512;

/// M86 (Roadmap v7) — `WITH (soar_lambda = N)`: SOAR spill's orthogonality-penalty weight `λ`, stored
/// **milli-scaled** (`λ × 1000`) so one int reloption carries the float knob (KISS, mirrors `aq_threshold`).
/// 0 = SOAR off (default, primary-only assignment, byte-identical). `> 0` spills each vector to a second list
/// (the paper recommends λ=1.0 at 1M, 1.5 at billion-scale). Only meaningful on an AQ index.
pub(crate) const DEFAULT_SOAR_LAMBDA_MILLI: i32 = 0;
const MIN_SOAR_LAMBDA_MILLI: i32 = 0;
const MAX_SOAR_LAMBDA_MILLI: i32 = 5000; // λ up to 5.0 (ScaNN redundancy_factor range top)

/// Parsed reloptions shared by the two AMs (`amoptions` is shared — `mod.rs`). `#[repr(C)]` with the varlena
/// header first (never touch `vl_len_` directly — `build_reloptions` sets it). `lists` is an ivfflat build knob;
/// `sbq_bits` (M51) + `pq_subspaces`/`pq_bits`/`aq_threshold_milli` (M59) are theodb_hnsw build knobs. Each AM
/// reads only the option it uses; new fields default to OFF so an index built without them is byte-identical.
#[repr(C)]
struct TheodbIvfflatOptions {
    vl_len_: i32, // varlena header (managed by build_reloptions)
    lists: i32,
    sbq_bits: i32,
    pq_subspaces: i32,
    pq_bits: i32,
    aq_threshold_milli: i32,
    separate_storage: i32,
    refine: i32,
    soar_lambda_milli: i32,
    rabitq_bits: i32,
    degree_bound: i32,
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
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "pq_subspaces".as_pg_cstr(),
        "Number of anisotropic-PQ subspaces for the theodb_hnsw AQ codes (0 = AQ off, M59)".as_pg_cstr(),
        DEFAULT_PQ_SUBSPACES,
        MIN_PQ_SUBSPACES,
        MAX_PQ_SUBSPACES,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "pq_bits".as_pg_cstr(),
        "Bits per subquantizer for the theodb_hnsw AQ codes (only 4 is supported, M59)".as_pg_cstr(),
        DEFAULT_PQ_BITS,
        MIN_PQ_BITS,
        MAX_PQ_BITS,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "aq_threshold".as_pg_cstr(),
        "Anisotropic parallel/orthogonal weight ratio η × 1000 for the theodb_hnsw AQ codes (1000 = isotropic, M59)".as_pg_cstr(),
        DEFAULT_AQ_THRESHOLD_MILLI,
        MIN_AQ_THRESHOLD_MILLI,
        MAX_AQ_THRESHOLD_MILLI,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "separate_storage".as_pg_cstr(),
        "Persist the v5 storage-separated IVF-AQ layout (codes/f32 on distinct pages) when 1 (M83)".as_pg_cstr(),
        DEFAULT_SEPARATE_STORAGE,
        MIN_SEPARATE_STORAGE,
        MAX_SEPARATE_STORAGE,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "refine".as_pg_cstr(),
        "Rerank the storage-separated IVF-AQ scan on SQ8 codes (v6) when 1, raw f32 (v5) when 0 (M85)".as_pg_cstr(),
        DEFAULT_REFINE,
        MIN_REFINE,
        MAX_REFINE,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "soar_lambda".as_pg_cstr(),
        "SOAR spill orthogonality-penalty λ × 1000 for the IVF-AQ build (0 = off, M86)".as_pg_cstr(),
        DEFAULT_SOAR_LAMBDA_MILLI,
        MIN_SOAR_LAMBDA_MILLI,
        MAX_SOAR_LAMBDA_MILLI,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "rabitq_bits".as_pg_cstr(),
        "Bits-per-dim for the v8 (refine=2) f32-free RaBitQ residual rerank codes (7 = f32-free 0.99 recall, E1)".as_pg_cstr(),
        DEFAULT_RABITQ_BITS,
        MIN_RABITQ_BITS,
        MAX_RABITQ_BITS,
        pg_sys::AccessExclusiveLock as pg_sys::LOCKMODE,
    );
    pg_sys::add_int_reloption(
        RELOPT_KIND,
        "degree_bound".as_pg_cstr(),
        "Per-vertex out-degree for the theodb_symqg co-located graph (multiple of 32; 32 = HNSW m0, E2)".as_pg_cstr(),
        DEFAULT_SYMQG_DEGREE,
        MIN_SYMQG_DEGREE,
        MAX_SYMQG_DEGREE,
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
    let tab: [pg_sys::relopt_parse_elt; 10] = [
        pg_sys::relopt_parse_elt {
            optname: "lists".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, lists) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "sbq_bits".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, sbq_bits) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "pq_subspaces".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, pq_subspaces) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "pq_bits".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, pq_bits) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "aq_threshold".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, aq_threshold_milli) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "separate_storage".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, separate_storage) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "refine".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, refine) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "soar_lambda".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, soar_lambda_milli) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "rabitq_bits".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, rabitq_bits) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
        },
        pg_sys::relopt_parse_elt {
            optname: "degree_bound".as_pg_cstr(),
            opttype: pg_sys::relopt_type::RELOPT_TYPE_INT,
            offset: std::mem::offset_of!(TheodbIvfflatOptions, degree_bound) as i32,
            // M135: PG18 added `isset_offset` to relopt_parse_elt (tracks whether the option was explicitly
            // set). We never consult that tracking, so 0 preserves PG17 semantics exactly. Rust struct literals are
            // exhaustive, so this is required at every literal — same tax pgvectorscale pays (options.rs:113).
            isset_offset: 0,
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

/// M59 — resolve the build-time `pq_subspaces` (m) for a `theodb_hnsw` index: the `WITH (pq_subspaces=M)` value,
/// or 0 (AQ off) when the option is absent (`rd_options` null → v1/v2 byte-identical). A fold reads `m` off the
/// persisted v3 meta, not the reloption, so this is only the initial-build gate.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
// M59 T3.3: wired into `ambuild_hnsw` (`pack_hnsw_for_build`) — decides the v3 AQ layout at initial build.
pub(crate) unsafe fn pq_subspaces_from_relation(indexrel: pg_sys::Relation) -> usize {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return DEFAULT_PQ_SUBSPACES as usize;
    }
    let m = (*(rd_options as *const TheodbIvfflatOptions)).pq_subspaces;
    if m < MIN_PQ_SUBSPACES {
        DEFAULT_PQ_SUBSPACES as usize
    } else {
        m as usize
    }
}

/// M59 — resolve `pq_bits` for a `theodb_hnsw` index: the `WITH (pq_bits=N)` value, or the default 4. Only 4 is
/// valid (the LUT16 path); `build_reloptions` already rejects anything outside [4,4] at DDL, so a resolved value
/// out of range falls back to the 4 default defensively.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
// M59 T3.3: wired into `ambuild_hnsw` (`pack_hnsw_for_build`).
pub(crate) unsafe fn pq_bits_from_relation(indexrel: pg_sys::Relation) -> u8 {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return DEFAULT_PQ_BITS as u8;
    }
    let bits = (*(rd_options as *const TheodbIvfflatOptions)).pq_bits;
    if (MIN_PQ_BITS..=MAX_PQ_BITS).contains(&bits) {
        bits as u8
    } else {
        DEFAULT_PQ_BITS as u8
    }
}

/// M59 — resolve `aq_threshold` (`η`) for a `theodb_hnsw` index: the milli-scaled `WITH (aq_threshold=N)` value
/// divided by 1000 (default `η = 1.0`, isotropic). Clamped to `≥ 1.0` (`η < 1` is meaningless — `aq.rs` clamps
/// too). Read at build to train the codebook; the fold reads the persisted `η`, not this.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
// M59 T3.3: wired into `ambuild_hnsw` (`pack_hnsw_for_build`); the fold reads η off the persisted meta instead.
pub(crate) unsafe fn aq_threshold_from_relation(indexrel: pg_sys::Relation) -> f32 {
    let rd_options = (*indexrel).rd_options;
    let milli = if rd_options.is_null() {
        DEFAULT_AQ_THRESHOLD_MILLI
    } else {
        (*(rd_options as *const TheodbIvfflatOptions)).aq_threshold_milli
    };
    (milli as f32 / 1000.0).max(1.0)
}

/// M83 — resolve `separate_storage` for a `theodb_ivfflat` AQ index: `true` iff `WITH (separate_storage=1)`.
/// Read at initial build to pick the v5 storage-separated layout over the v4 interleaved one. Off (v4) when the
/// option is absent, so every existing index is byte-identical.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
pub(crate) unsafe fn separate_storage_from_relation(indexrel: pg_sys::Relation) -> bool {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return false;
    }
    (*(rd_options as *const TheodbIvfflatOptions)).separate_storage == 1
}

/// M85 — resolve `refine` for a `theodb_ivfflat` AQ index: `true` iff `WITH (refine=1)` (SQ8 rerank, v6). Off
/// (v5 f32 rerank) when absent, so every existing index is byte-identical. Only consulted when `separate_storage=1`.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
pub(crate) unsafe fn refine_sq8_from_relation(indexrel: pg_sys::Relation) -> bool {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return false;
    }
    (*(rd_options as *const TheodbIvfflatOptions)).refine == 1
}

/// E1 — `WITH (refine = 2)` on a storage-separated AQ index: the v8 f32-free residual-RaBitQ rerank path.
///
/// # Safety
/// `indexrel` must be a live open relation.
pub(crate) unsafe fn refine_rabitq_from_relation(indexrel: pg_sys::Relation) -> bool {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return false;
    }
    (*(rd_options as *const TheodbIvfflatOptions)).refine == 2
}

/// E1 — bits-per-dim for the v8 RaBitQ rerank codes (`WITH (rabitq_bits = N)`, default 7).
///
/// # Safety
/// `indexrel` must be a live open relation.
pub(crate) unsafe fn rabitq_bits_from_relation(indexrel: pg_sys::Relation) -> u8 {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return DEFAULT_RABITQ_BITS as u8;
    }
    let bits = (*(rd_options as *const TheodbIvfflatOptions)).rabitq_bits;
    if (MIN_RABITQ_BITS..=MAX_RABITQ_BITS).contains(&bits) {
        bits as u8
    } else {
        DEFAULT_RABITQ_BITS as u8
    }
}

/// E2 — resolve `degree_bound` (R) for a `theodb_symqg` index: the `WITH (degree_bound=R)` value ROUNDED UP to a
/// multiple of 32 (FastScan alignment) and clamped to `[MIN, MAX]`, or the default 32 when absent.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
pub(crate) unsafe fn degree_bound_from_relation(indexrel: pg_sys::Relation) -> usize {
    let rd_options = (*indexrel).rd_options;
    if rd_options.is_null() {
        return DEFAULT_SYMQG_DEGREE as usize;
    }
    let r = (*(rd_options as *const TheodbIvfflatOptions)).degree_bound;
    let r = r.clamp(MIN_SYMQG_DEGREE, MAX_SYMQG_DEGREE);
    (r as usize).div_ceil(32) * 32 // round up to a multiple of 32
}

/// M86 — resolve SOAR `λ` for a `theodb_ivfflat` AQ index: the milli-scaled `WITH (soar_lambda=N)` / 1000. 0.0 =
/// SOAR off (default, primary-only assignment, byte-identical). Read at build to spill; the fold does not re-spill.
///
/// # Safety
/// `indexrel` must be a valid open index relation.
pub(crate) unsafe fn soar_lambda_from_relation(indexrel: pg_sys::Relation) -> f64 {
    let rd_options = (*indexrel).rd_options;
    let milli = if rd_options.is_null() {
        DEFAULT_SOAR_LAMBDA_MILLI
    } else {
        (*(rd_options as *const TheodbIvfflatOptions)).soar_lambda_milli
    };
    (milli.max(0) as f64) / 1000.0
}
