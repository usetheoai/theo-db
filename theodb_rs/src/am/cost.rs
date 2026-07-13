//! Honest `amcostestimate` visit-ratio (M48 T5.1 / D5).
//!
//! The planner leans on `index_startup_cost` for `ORDER BY <-> LIMIT k` queries (an ordered index scan can
//! stop after k rows). We set `startup = genericcostestimate_total * ratio`, where `ratio` is the fraction of
//! index tuples an ordered ANN scan actually visits. A large index visits a tiny fraction (`ratio << 1` →
//! small startup → the index wins the LIMIT); a tiny index visits nearly all of them (`ratio ≈ 1` → startup ≈
//! total → seqscan+sort wins). The old stub returned cost 0, so the index ALWAYS won — it lied to the planner
//! (deep-view gap G6).
//!
//! The ratio math follows pgvector's cost model (`hnsw.c` / `ivfflat.c`): only the load-bearing
//! `startup = total * ratio` core is ported. pgvector's secondary corrections — IVF's `sequentialRatio` page
//! adjustment and the HNSW/IVF TOAST `startupPages` refinement — are intentionally omitted: they only shift
//! cost slightly *toward* the index and never flip the index-vs-seqscan choice this feature exists to make
//! honest. Omitting them biases conservatively toward seqscan. Constant tuning is out of scope (D5).
use crate::am::build::HNSW_M;
use crate::am::hnsw_page::HNSW_STRUCT_MAGIC;
use crate::am::{guc, page};
use pgrx::pg_sys;

/// IVF: the fraction of lists an ordered scan probes. `probes / lists`, clamped to 1 (probes may exceed lists
/// on a tiny index). `lists == 0` (unreadable/degenerate meta) → 1.0 (no advantage over seqscan).
pub(crate) fn ivf_visit_ratio(probes: usize, lists: usize) -> f64 {
    if lists == 0 {
        return 1.0;
    }
    (probes as f64 / lists as f64).min(1.0)
}

/// HNSW: pgvector's `hnsw.c` tuple-visit estimate over `N` tuples. `scalingFactor = 0.55`; `HnswGetMl(m) =
/// 1/ln(m)`; layer-0 fan-out `HnswGetLayerM(m,0) = 2m`. Degenerate inputs (`N <= 0`, `m <= 1`) → 1.0. This is
/// the *ratio* only — the caller applies `startup = total * ratio`; pgvector's secondary TOAST `startupPages`
/// correction is omitted (see the module docstring). Not full-parity with `hnswcostestimate`.
pub(crate) fn hnsw_visit_ratio(tuples: f64, m: usize, ef_search: usize) -> f64 {
    if tuples <= 0.0 || m <= 1 {
        return 1.0;
    }
    let mf = m as f64;
    let ef = ef_search.max(1) as f64;
    let ln_n = tuples.ln();
    let entry_level = (ln_n / mf.ln()).floor(); // entryLevel = ln(N) * HnswGetMl(m), HnswGetMl(m) = 1/ln(m)
    let layer0_max = 2.0 * mf * ef; // HnswGetLayerM(m, 0) = 2m, times ef_search
    let layer0_selectivity = 0.55 * ln_n / (mf.ln() * (1.0 + ef.ln()));
    ((entry_level * mf + layer0_max * layer0_selectivity) / tuples).min(1.0)
}

/// Pure dispatch — the fail-safe fallback matrix, unit-testable with forged inputs (no `Relation` needed).
/// An unreadable meta (`magic == None` because `peek_magic` returned `Err`), an unknown magic (v1/blob/legacy),
/// or `tuples <= 0` all degrade to 1.0 — the ratio a plain generic estimate would use (EC-3: NEVER error).
pub(crate) fn ratio_for(magic: Option<u32>, tuples: f64, probes: usize, lists: usize, m: usize, ef: usize) -> f64 {
    if tuples <= 0.0 {
        return 1.0;
    }
    match magic {
        Some(x) if x == page::IVF_STRUCT_MAGIC => ivf_visit_ratio(probes, lists),
        Some(x) if x == HNSW_STRUCT_MAGIC => hnsw_visit_ratio(tuples, m, ef),
        _ => 1.0,
    }
}

/// Read the meta (already locked by the planner — NoLock, pgvector pattern) and compute the visit ratio.
/// EC-3 contract: EVERY meta read is fail-safe — `peek_magic` failure → `None` → 1.0; a torn/corrupt IVF meta
/// under a concurrent fold → `dir.len()` unread → `lists == 0` → 1.0. `amcostestimate` must NEVER error, or it
/// would abort ALL query planning while a VACUUM momentarily makes the meta unreadable. HNSW's `m` is a fixed
/// build constant (no reloption), so no second meta read is needed for it — avoiding another `Err` surface.
pub(crate) unsafe fn scan_visit_ratio(rel: pg_sys::Relation, tuples: f64) -> f64 {
    let magic = page::peek_magic(rel).ok();
    let lists = if magic == Some(page::IVF_STRUCT_MAGIC) {
        // v3 f32 OR v4 AQ (M77) — read_ivf_meta rejects v4, so fall back to the v4 meta for its list count. Still
        // fail-safe: either Err → 0 → ratio 1.0 (never aborts planning).
        page::read_ivf_meta(rel)
            .map(|meta| meta.dir.len())
            .or_else(|_| page::read_ivf_aq_meta(rel).map(|meta| meta.dir.len()))
            .or_else(|_| page::read_ivf_aq_meta_split(rel).map(|meta| meta.dir.len())) // M83 v5
            .or_else(|_| page::read_ivf_aq_meta_split_sq8(rel).map(|meta| meta.dir.len())) // M85 v6 storage-separated
            .unwrap_or(0)
    } else {
        0
    };
    ratio_for(magic, tuples, guc::probes(), lists, HNSW_M, guc::ef_search())
}

// ---- M95: honest cost model for the vecfilter Custom Scan node ----
//
// The node's cost = term_B (the bitmap sub-plan that PRODUCES the membership) + term_V (the vector scan that
// filters by it). term_B is read directly from the child `bitmapqual.total_cost` by the planner hook. term_V is
// the part that must be honest about the M91 adaptive probing: a selective filter probes MORE lists at runtime
// until `rerank_pool` matching candidates accumulate (`scan.rs:641`), so term_V CANNOT reuse the child
// IndexPath's default-probe cost — it re-derives the effective probe count from the bitmap selectivity here.

/// M95 — the runtime probe count the M91 loop reaches under a filter of selectivity `s` (= matching fraction).
/// A candidate matches with probability `s`; each probed list yields `avg_list_size` candidates; the loop keeps
/// probing nearest lists until `rerank_pool` MATCHING candidates accumulate → `probes ≈ rerank_pool /
/// (s * avg_list_size)`, never below the fixed default (the loop's `probed >= probes` short-circuit) and bounded
/// by the total list count. Selective `s` → many probes (costlier, higher recall); loose `s` → the default
/// already fills the pool (cheap — the byte-identical no-filter path).
///
/// Fail-safe (EC-3): degenerate inputs (`s <= 0`, `avg_list_size <= 0`, `lists == 0`) return `0.0`, the sentinel
/// the planner hook treats as "meta unreadable → do not add the node". NEVER panics / divides by zero — a cost
/// function that aborts would take down ALL query planning.
pub(crate) fn effective_probes(
    s: f64,
    lists: usize,
    avg_list_size: f64,
    rerank_pool: f64,
    probes_default: usize,
) -> f64 {
    if s <= 0.0 || avg_list_size <= 0.0 || lists == 0 {
        return 0.0; // fail-safe sentinel
    }
    let needed = rerank_pool / (s * avg_list_size);
    needed.max(probes_default as f64).clamp(1.0, lists as f64)
}

/// M95 — the vector-scan-with-membership cost (term_V), priced with the same page/CPU globals `cost_index` uses.
/// Stage-1 reads `eff_probes` code-page lists; Stage-2 reranks `rerank_pool` survivors with random f32 reads;
/// per-candidate AH scoring + the centroid sort are CPU. `eff_probes == 0.0` (the fail-safe sentinel) → `0.0`
/// so the caller detects the degenerate case and falls back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn vecfilter_scan_cost(
    eff_probes: f64,
    pages_per_list: f64,
    avg_list_size: f64,
    rerank_pool: f64,
    lists: usize,
    spc_random_page_cost: f64,
    cpu_operator_cost: f64,
) -> f64 {
    if eff_probes <= 0.0 {
        return 0.0; // propagate the fail-safe sentinel
    }
    let code_reads = eff_probes * pages_per_list;
    let candidates = eff_probes * avg_list_size;
    code_reads * spc_random_page_cost                // Stage-1 code-page reads (partial, O(probes))
        + rerank_pool * spc_random_page_cost         // Stage-2 rerank random f32 reads
        + candidates * cpu_operator_cost             // AH scoring per candidate
        + lists as f64 * cpu_operator_cost // Stage-0 centroid sort
}

#[cfg(test)]
mod tests {
    use super::*;

    // A magic that is neither IVF nor HNSW (forged — a v1/legacy/blob or garbage page).
    const UNKNOWN_MAGIC: u32 = 0xDEAD_BEEF;

    // ---- M95 vecfilter cost model ----

    #[test]
    fn m95_effective_probes_loose_is_default() {
        // s=0.5, avg=1000 → needed=64/(0.5*1000)=0.128 → max(8, 0.128)=8. Loose filter → default probes.
        assert_eq!(effective_probes(0.5, 1024, 1000.0, 64.0, 8), 8.0);
    }

    #[test]
    fn m95_effective_probes_selective_grows() {
        // s=0.001, avg=1000 → needed=64/(0.001*1000)=64 → ≈64 probes (pool-fill count).
        assert!(effective_probes(0.001, 1024, 1000.0, 64.0, 8) >= 60.0);
    }

    #[test]
    fn m95_effective_probes_clamped_to_lists() {
        // s=1e-6 → needed huge → clamped to the list count.
        assert_eq!(effective_probes(1e-6, 128, 1000.0, 64.0, 8), 128.0);
    }

    #[test]
    fn m95_effective_probes_degenerate_returns_sentinel() {
        assert_eq!(effective_probes(0.0, 1024, 1000.0, 64.0, 8), 0.0); // s<=0
        assert_eq!(effective_probes(0.5, 1024, 0.0, 64.0, 8), 0.0); // avg_list_size==0
        assert_eq!(effective_probes(0.5, 0, 1000.0, 64.0, 8), 0.0); // lists==0
    }

    #[test]
    fn m95_scan_cost_monotone_in_probes() {
        let c8 = vecfilter_scan_cost(8.0, 5.0, 1000.0, 64.0, 1024, 4.0, 0.0025);
        let c64 = vecfilter_scan_cost(64.0, 5.0, 1000.0, 64.0, 1024, 4.0, 0.0025);
        assert!(c64 > c8, "cost must grow with probes: c8={c8} c64={c64}");
    }

    #[test]
    fn m95_scan_cost_sentinel_propagates() {
        assert_eq!(vecfilter_scan_cost(0.0, 5.0, 1000.0, 64.0, 1024, 4.0, 0.0025), 0.0);
    }

    #[test]
    fn ivf_ratio_is_probes_over_lists_clamped() {
        assert!((ivf_visit_ratio(10, 100) - 0.1).abs() < 1e-9); // 10 probes of 100 lists
        assert_eq!(ivf_visit_ratio(200, 100), 1.0); // probes > lists clamps to 1
    }

    #[test]
    fn ivf_ratio_degenerate_lists_falls_back_to_one() {
        assert_eq!(ivf_visit_ratio(10, 0), 1.0); // unreadable/empty meta → no advantage
    }

    #[test]
    fn hnsw_ratio_shrinks_as_n_grows() {
        // The whole point: a bigger index visits a SMALLER fraction, so its startup cost drops and a LIMIT
        // query prefers it. ratio(50k) must be well below ratio(100), and both within [0, 1].
        let small = hnsw_visit_ratio(100.0, 16, 64);
        let big = hnsw_visit_ratio(50_000.0, 16, 64);
        assert!(big < small, "ratio must shrink with N ({big} !< {small})");
        assert!((0.0..=1.0).contains(&big) && (0.0..=1.0).contains(&small));
    }

    #[test]
    fn hnsw_ratio_clamps_and_handles_degenerate() {
        assert_eq!(hnsw_visit_ratio(0.0, 16, 64), 1.0); // no tuples
        assert_eq!(hnsw_visit_ratio(1.0, 1, 64), 1.0); // m <= 1 (ln(m)=0 guard)
        assert!(hnsw_visit_ratio(4.0, 16, 64) <= 1.0); // tiny N: numerator > N ⇒ clamped to 1
    }

    #[test]
    fn ratio_for_fallback_matrix_never_panics_and_is_one() {
        // EC-3 proof: every unreadable / unknown / degenerate input yields the safe 1.0 fallback — no panic.
        assert_eq!(ratio_for(None, 50_000.0, 10, 100, 16, 64), 1.0); // peek_magic Err → None
        assert_eq!(ratio_for(Some(UNKNOWN_MAGIC), 50_000.0, 10, 100, 16, 64), 1.0); // v1/blob/garbage
        assert_eq!(ratio_for(Some(page::IVF_STRUCT_MAGIC), 0.0, 10, 100, 16, 64), 1.0); // tuples == 0
        assert_eq!(ratio_for(Some(page::IVF_STRUCT_MAGIC), 50_000.0, 10, 0, 16, 64), 1.0); // torn IVF meta → lists 0
    }

    #[test]
    fn ratio_for_dispatches_to_the_right_am() {
        let ivf = ratio_for(Some(page::IVF_STRUCT_MAGIC), 50_000.0, 10, 100, 16, 64);
        assert!((ivf - 0.1).abs() < 1e-9, "IVF branch = probes/lists");
        let hnsw = ratio_for(Some(HNSW_STRUCT_MAGIC), 50_000.0, 10, 100, 16, 64);
        assert!(hnsw < 1.0 && hnsw > 0.0, "HNSW branch = pgvector formula, in (0,1) at 50k");
    }
}
