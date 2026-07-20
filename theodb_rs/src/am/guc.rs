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

/// M118 — `SET theodb_hnsw.resume_max_mb = N`: memory ceiling (MB) for the resume-from-discarded scan's retained
/// frontier + visited set. When the retained state exceeds this, the scan stops resuming and returns what it holds
/// (fail-safe — correctness preserved: the executor's MVCC recheck + `max_scan_tuples` already bound emission).
/// `0` = disabled (unbounded), consistent with `max_scan_tuples` / `vacuum_fold_max_mb`. Mirrors pgvector 0.8.5's
/// `work_mem` guard on `so->discarded` (EC-2: the `0 = disabled` contract; EC-5: overflow returns, never panics).
///
/// NOTE (review LOW): the check uses `ResumableGround::approx_bytes()`, which counts heap/set *element* bytes and
/// ignores `HashSet` control-byte + load-factor overhead and `BinaryHeap` spare capacity — so real RSS at the trip
/// point is ~2-3× the nominal MB. The ceiling is therefore CONSERVATIVE-PERMISSIVE (uses more than declared); size
/// it with headroom. It is a fail-safe soft guard, not a hard allocator limit — correctness never depends on it.
pub(crate) const DEFAULT_HNSW_RESUME_MAX_MB: i32 = 64;
pub(crate) static HNSW_RESUME_MAX_MB: GucSetting<i32> = GucSetting::<i32>::new(DEFAULT_HNSW_RESUME_MAX_MB);

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

/// M56 fase 2 — `SET theodb.hnsw_slot_reuse = on|off`: when ON, `aminsert` REUSES a tombstoned element slot via a
/// proper in-place insert (search + link) before growing the pending region, bounding relation growth under
/// DELETE+INSERT churn. **Default OFF.** The churn benchmark (`docs/benchmarks/m56-slot-reuse-churn.md`) drove the
/// design: the original slot-reuse SUPPRESSED the ratio-compaction (tombstones consumed before the threshold, so
/// the graph-REPAIRING fold never fired) and recall@10 collapsed to ~0.57. The fix — reuse only clean level-0
/// non-entry slots + trigger the fold on CHURN (`count_churned`, not just tombstones) — makes it recall-SAFE
/// (~0.955). BUT the benchmark then showed the net benefit is MARGINAL: the size win is ~1.04–1.18× and slot-reuse
/// never beats the plain navigate-through + fold path on recall. So it stays OFF by default (the simpler
/// navigate-through + fold is the recommended path), opt-in for niche churn-heavy workloads that want the marginal
/// between-fold size win.
pub(crate) static HNSW_SLOT_REUSE: GucSetting<bool> = GucSetting::<bool>::new(false);

/// Whether `aminsert` should reuse tombstoned slots in place (M56 fase 2).
pub(crate) fn hnsw_slot_reuse() -> bool {
    HNSW_SLOT_REUSE.get()
}

/// M118 — `SET theodb_hnsw.resume = on|off`: when ON (default), the filtered iterative scan RESUMES from the
/// retained beam frontier (resume-from-discarded) instead of re-searching the graph with a doubled `ef`. OFF
/// reverts to the M52 re-search (the pre-M118 path) — kept as an operator escape hatch and for the honest
/// own-path A/B (`docs/benchmarks/m118-resume-discarded.md`). V1 (exact-f32) only; SBQ/AQ always re-search.
pub(crate) static HNSW_RESUME: GucSetting<bool> = GucSetting::<bool>::new(true);

/// Whether the filtered iterative scan uses resume-from-discarded (M118). Default ON.
pub(crate) fn hnsw_resume() -> bool {
    HNSW_RESUME.get()
}

/// E2 FastScan A/B kill-switch: when on (default), the `theodb_symqg` scan uses the batched FastScan 1-bit sign
/// kernel; off forces the scalar `estimate_sign` path (same index/box — isolates the kernel's measured effect).
pub(crate) static SYMQG_FASTSCAN: GucSetting<bool> = GucSetting::<bool>::new(true);

/// Whether the `theodb_symqg` scan uses the FastScan kernel (default on; off = scalar A/B baseline).
pub(crate) fn symqg_fastscan() -> bool {
    SYMQG_FASTSCAN.get()
}

/// Columnar zone-map skip-pruning kill-switch: when on (default), a WHERE-filtered columnar aggregate skips
/// chunk groups whose min/max cannot satisfy the predicate; off = full decode (same-table A/B baseline).
pub(crate) static COLUMNAR_ZONEMAP_SKIP: GucSetting<bool> = GucSetting::<bool>::new(true);

/// Whether the columnar scan consults the min/max zone-map to skip chunk groups (default on; off = A/B baseline).
pub(crate) fn columnar_zonemap_skip() -> bool {
    COLUMNAR_ZONEMAP_SKIP.get()
}

/// M92 spike — whether the arbitrary-WHERE Custom Scan Provider pathlist hook is active. Default OFF: a planner
/// hook that misbehaves breaks EVERY query, so the spike stays inert until explicitly enabled.
pub(crate) static ENABLE_VECFILTER: GucSetting<bool> = GucSetting::<bool>::new(false);

/// Whether the M92 vecfilter Custom Scan Provider hook is enabled (spike kill-switch).
pub(crate) fn vecfilter_enabled() -> bool {
    ENABLE_VECFILTER.get()
}

/// M95 — force the vecfilter node's selection by pricing it below the cheapest base path (the pre-M95 posture),
/// bypassing the honest cost model. Default OFF: the honest cost is the default. This is an explicit user
/// override — the same rationale as Postgres's `enable_*` knobs — for the case the planner cannot see: a
/// selective filter where the node's higher HONEST cost loses to the probe-blind native post-filter, yet the
/// node wins on RECALL (measured, M92). Also the deterministic switch the membership-mechanism tests use to
/// exercise the node independently of planner selection.
pub(crate) static VECFILTER_FORCE: GucSetting<bool> = GucSetting::<bool>::new(false);

/// Whether to force the vecfilter node's selection (bypass the honest cost model). See `VECFILTER_FORCE`.
pub(crate) fn vecfilter_force() -> bool {
    VECFILTER_FORCE.get()
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
// M104 — the hardening arc's bounded-memory / resilience knobs. Registered here (not just read via
// `current_setting`) so `SET theodb.<name> = N` actually takes effect and the value shows in `pg_settings` —
// the review's H1: an advertised "configurable bound" that isn't registered silently ignores the SET.
pub(crate) static VACUUM_FOLD_MAX_MB: GucSetting<i32> = GucSetting::<i32>::new(1024);
pub(crate) static ARROW_CACHE_MAX_ENTRIES: GucSetting<i32> = GucSetting::<i32>::new(16);
pub(crate) static VECTORIZER_DEAD_LETTER_MAX: GucSetting<i32> = GucSetting::<i32>::new(1000);
pub(crate) static HTTP_BREAKER_OPEN_MS: GucSetting<i32> = GucSetting::<i32>::new(30_000);
pub(crate) static AI_MAX_BATCH: GucSetting<i32> = GucSetting::<i32>::new(256);

/// VACUUM skips the in-index compaction fold above this on-disk size (MB); 0 disables the guard. Default 1024.
pub(crate) fn vacuum_fold_max_mb() -> u64 {
    VACUUM_FOLD_MAX_MB.get().max(0) as u64
}
/// Max distinct tables held in the per-backend Arrow cache before eviction (never below 1). Default 16.
pub(crate) fn arrow_cache_max_entries() -> usize {
    ARROW_CACHE_MAX_ENTRIES.get().max(1) as usize
}
/// Retained `failed` dead-letter rows per vectorizer job before purge (never below 0). Default 1000.
pub(crate) fn vectorizer_dead_letter_max() -> i32 {
    VECTORIZER_DEAD_LETTER_MAX.get().max(0)
}
/// How long the AI HTTP circuit breaker stays open before a half-open probe (ms; never below 0). Default 30000.
pub(crate) fn http_breaker_open_ms() -> u64 {
    HTTP_BREAKER_OPEN_MS.get().max(0) as u64
}
/// Max prompts per batched AI request before chunking into multiple round-trips (never below 1). Default 256.
pub(crate) fn ai_max_batch() -> usize {
    AI_MAX_BATCH.get().max(1) as usize
}

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
    GucRegistry::define_bool_guc(
        c"theodb_hnsw.resume",
        c"When on (default), the filtered iterative scan resumes from the retained frontier (M118 resume-from-discarded)",
        c"Off reverts to the M52 re-search-with-doubled-ef path (operator kill-switch + own-path A/B). V1 only; SBQ/AQ always re-search.",
        &HNSW_RESUME,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.symqg_fastscan",
        c"When on (default), theodb_symqg scans with the batched FastScan 1-bit sign kernel; off forces scalar estimate_sign",
        c"E2 FastScan kill-switch / same-index A/B baseline (isolates the kernel's effect). No effect on non-symqg indexes.",
        &SYMQG_FASTSCAN,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.columnar_zonemap_skip",
        c"When on (default), a WHERE-filtered theodb_columnar aggregate skips chunk groups whose min/max cannot match",
        c"Columnar zone-map skip-pruning kill-switch / same-table A/B baseline. No effect on non-columnar tables.",
        &COLUMNAR_ZONEMAP_SKIP,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.enable_vecfilter",
        c"When on, the M92 arbitrary-WHERE Custom Scan Provider intercepts filtered vector queries (spike)",
        c"Default off (kill-switch). A planner hook affects every query, so the spike stays inert until enabled.",
        &ENABLE_VECFILTER,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_bool_guc(
        c"theodb.vecfilter_force",
        c"When on, force the vecfilter node's selection (bypass the honest M95 cost model)",
        c"Default off (the honest cost model decides). An explicit override for a selective filter where the node wins on RECALL but its higher honest cost loses to the probe-blind native post-filter.",
        &VECFILTER_FORCE,
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
    GucRegistry::define_int_guc(
        c"theodb_hnsw.resume_max_mb",
        c"Memory ceiling (MB) for the resume-from-discarded scan's retained frontier (0 = disabled/unbounded)",
        c"When the retained beam frontier + visited set exceed this, the scan stops resuming and returns what it holds (fail-safe; correctness preserved by the executor MVCC recheck + max_scan_tuples). 0 disables the cap.",
        &HNSW_RESUME_MAX_MB,
        0,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );
    // M104 bounded-memory / resilience knobs (review H1).
    GucRegistry::define_int_guc(
        c"theodb.vacuum_fold_max_mb",
        c"VACUUM skips the in-index compaction fold above this on-disk size in MB (0 = never skip)",
        c"A large legacy/HNSW index folds O(N) in RAM; above this bound VACUUM WARNs and defers compaction to REINDEX (correctness preserved). Default 1024.",
        &VACUUM_FOLD_MAX_MB,
        0,
        1_048_576,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.arrow_cache_max_entries",
        c"Max distinct tables held in the per-backend Arrow cache before eviction",
        c"Bounds the M101 per-backend columnar Arrow cache. Higher = fewer rebuilds, more RAM. Default 16.",
        &ARROW_CACHE_MAX_ENTRIES,
        1,
        1_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.vectorizer_dead_letter_max",
        c"Retained `failed` dead-letter rows per vectorizer job before the worker purges older ones",
        c"Bounds the on-disk dead-letter so a poison row / bad endpoint cannot accumulate tombstones forever. Default 1000.",
        &VECTORIZER_DEAD_LETTER_MAX,
        0,
        100_000_000,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.http_breaker_open_ms",
        c"How long the AI HTTP circuit breaker stays open before a half-open probe (milliseconds)",
        c"After K consecutive failures the breaker opens and calls fail-fast for this long, then one probe decides re-close. Default 30000.",
        &HTTP_BREAKER_OPEN_MS,
        0,
        3_600_000,
        GucContext::Userset,
        GucFlags::default(),
    );
    GucRegistry::define_int_guc(
        c"theodb.ai_max_batch",
        c"Max prompts per batched AI request before chunking into multiple round-trips",
        c"A huge array becomes several bounded requests instead of one giant request/response. Default 256.",
        &AI_MAX_BATCH,
        1,
        1_000_000,
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

/// M118: the resume frontier memory ceiling in bytes (`0` = disabled/unbounded).
pub(crate) fn hnsw_resume_max_bytes() -> usize {
    (HNSW_RESUME_MAX_MB.get().max(0) as usize).saturating_mul(1024 * 1024)
}
