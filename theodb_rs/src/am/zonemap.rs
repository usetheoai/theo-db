//! Zone-map skip-pruning predicate test (plan columnar-zonemap-skip-pruning T1.1). PURE — no `pg_sys` — so the
//! correctness-critical exclusion logic is standalone-`rustc`-testable and isolated from the PG scan plumbing
//! (ADR D1). `theodb_columnar` already WRITES a per-`(chunk_group, column)` min/max into the `ChunkDirEntry`
//! (`columnar_codec::compute_minmax`); this is the missing CONSUMER: given a chunk group's zone-map and a pushed
//! `col <op> const` predicate, decide whether the chunk group can be skipped (no row can match).
//!
//! Encoding contract (MUST match `compute_minmax`): integer kinds (I2/I4/I8/Bool) store the value as `i64`, bits
//! = `i64 as u64`; float kinds (F4/F8) widen to `f64`, bits = `f64::to_bits`. `ZonePredicate::const_bits` is
//! encoded by `admit` in the SAME domain as the column's `MinMaxKind` (ADR D5 — same-domain-or-fallback).
//! Wired: `columnar_agg::extract_zone_predicate` (plan-time) → `decode_columns` skip guard (`chunk_can_match`) +
//! the DataFusion filter (`df_executor::build_filter_expr`, the final authority).
use super::columnar_codec::MinMaxKind;

/// A pushed btree comparison, reduced to the column's min/max domain (ADR D2/D5). `Var(col) <op> const`.
#[derive(Clone, Copy, PartialEq, Debug)]
// `#[repr(i32)]` with EXPLICIT discriminants (review MEDIUM): the agg path serializes `op as i32` into the
// CustomScan `custom_private` (`columnar_agg::encode_private`) and decodes it by literal integer
// (`decode_private`). That is the ONE op↔int mapping the compiler does NOT check — reordering the variants would
// silently corrupt the pushed predicate. Pinning the discriminants makes the encode/decode round-trip stable.
#[repr(i32)]
pub(crate) enum ZoneOp {
    Lt = 0,
    Le = 1,
    Eq = 2,
    Ge = 3,
    Gt = 4,
    /// `<>` — M151. `<>` is NOT a btree strategy of its own (btree defines only 1-5); it is detected as the
    /// NEGATOR of the btree `=` (strategy 3). NEVER used for zone-map pruning (a chunk `[min,max]` with min≠max
    /// almost always contains a value ≠ const). It rides the predicate list ONLY to reach the DataFusion `Filter`
    /// (`build_filter_expr` → `not_eq`); `chunk_can_match` treats it as "must scan" (`excluded(Ne) = false`).
    Ne = 5,
}

/// A single pushable predicate: `column[col] <op> const` where `const_bits` is in the column's `MinMaxKind` domain.
#[derive(Clone, Debug)]
pub(crate) struct ZonePredicate {
    pub col: usize,
    pub op: ZoneOp,
    pub const_bits: u64,
}

/// A pushable TEXT operator (M156). NEVER prunes chunk groups (text has no numeric zone domain here) — it rides
/// only to the DataFusion `Filter` (`build_filter_expr` → `eq`/`not_eq`/`like`/`not_like`). Only PROVABLY-safe
/// shapes reach here: deterministic collation + type text/varchar (bpchar excluded — `bpchareq` trims trailing
/// blanks, M153) + operator in this set. ILIKE (`~~*`) and regex (`~`/`!~`/`~*`/`!~*`) DECLINE upstream
/// (DataFusion's regex is Rust/RE2, ≠ PG POSIX ERE — confirmed M152). Explicit discriminants: the agg path
/// serializes `op as i32` (`columnar_agg::encode_text_preds`) and decodes it by literal integer — reordering the
/// variants would silently corrupt the pushed predicate (same contract as `ZoneOp`).
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(i32)]
pub(crate) enum TextOp {
    Eq = 0,
    Ne = 1,
    Like = 2,
    NotLike = 3,
}

/// A single pushable text predicate: `column[col] <op> 'needle'` on a text/varchar column under a deterministic
/// collation. `needle` is the constant's server-encoded (UTF-8) payload, extracted at plan time from the `Const`.
#[derive(Clone, Debug)]
pub(crate) struct TextPredicate {
    pub col: usize,
    pub op: TextOp,
    pub needle: String,
}

/// Can a chunk group whose zone-map is `(has_minmax, min_bits, max_bits, kind)` possibly contain a row matching
/// `pred`? Returns `true` = "must scan" UNLESS the zone-map PROVES no row can match (then `false` = safe to skip).
/// FAIL-SAFE (never fail-wrong): `has_minmax == false`, `kind == None`, or a NaN in the compared floats → `true`.
/// The skip is an admission filter — the real predicate downstream is the final authority (ADR D3).
pub(crate) fn chunk_can_match(
    has_minmax: bool,
    min_bits: u64,
    max_bits: u64,
    kind: MinMaxKind,
    pred: &ZonePredicate,
) -> bool {
    if !has_minmax || kind == MinMaxKind::None {
        return true; // cannot evaluate → must scan
    }
    match kind {
        MinMaxKind::F4 | MinMaxKind::F8 => {
            let (min, max, c) = (
                f64::from_bits(min_bits),
                f64::from_bits(max_bits),
                f64::from_bits(pred.const_bits),
            );
            if min.is_nan() || max.is_nan() || c.is_nan() {
                return true; // unordered → cannot prove exclusion
            }
            !excluded(min, max, c, pred.op)
        }
        // I2 / I4 / I8 / Bool — stored as i64 two's-complement bits (compute_minmax). Decode to i64 to compare in
        // NUMERIC order (a raw-u64 compare would order negatives as huge and decide wrong — EC-3).
        _ => {
            let (min, max, c) = (min_bits as i64, max_bits as i64, pred.const_bits as i64);
            !excluded(min, max, c, pred.op)
        }
    }
}

/// True iff the chunk range `[min, max]` provably contains NO value satisfying `<op> c`.
#[inline]
fn excluded<T: PartialOrd>(min: T, max: T, c: T, op: ZoneOp) -> bool {
    match op {
        ZoneOp::Lt => min >= c,           // col < c   : excluded iff every value ≥ c
        ZoneOp::Le => min > c,            // col <= c  : excluded iff every value > c
        ZoneOp::Gt => max <= c,           // col > c   : excluded iff every value ≤ c
        ZoneOp::Ge => max < c,            // col >= c  : excluded iff every value < c
        ZoneOp::Eq => c < min || c > max, // col = c : excluded iff c outside [min, max]
        // col <> c : NEVER provably empty from min/max alone (a chunk with min≠max holds values ≠ c). M151 —
        // `<>` is a filter-only op (applied by DataFusion), not a pruning op. Fail-safe: never skip.
        ZoneOp::Ne => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipred(col: usize, op: ZoneOp, c: i64) -> ZonePredicate {
        ZonePredicate { col, op, const_bits: c as u64 }
    }
    fn fpred(op: ZoneOp, c: f64) -> ZonePredicate {
        ZonePredicate { col: 0, op, const_bits: c.to_bits() }
    }
    fn ichunk(min: i64, max: i64) -> (bool, u64, u64) {
        (true, min as u64, max as u64)
    }

    #[test]
    fn test_zonemap_excludes_below_range() {
        // I4 chunk [100,200].
        let (h, lo, hi) = ichunk(100, 200);
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 50)),
            "y<50 → skip"
        );
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Le, 99)),
            "y<=99 → skip"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 101)),
            "y<101 → scan"
        );
    }

    #[test]
    fn test_zonemap_excludes_above_range() {
        let (h, lo, hi) = ichunk(100, 200);
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Gt, 200)),
            "y>200 → skip"
        );
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Ge, 201)),
            "y>=201 → skip"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Ge, 200)),
            "y>=200 → scan"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Gt, 199)),
            "y>199 → scan"
        );
    }

    #[test]
    fn test_zonemap_eq_in_and_out() {
        let (h, lo, hi) = ichunk(100, 200);
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 150)),
            "y=150 → scan"
        );
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 99)),
            "y=99 → skip"
        );
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 201)),
            "y=201 → skip"
        );
        // boundary: c == min and c == max must NOT be excluded.
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 100)),
            "y=100 (boundary) → scan"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 200)),
            "y=200 (boundary) → scan"
        );
    }

    #[test]
    fn test_chunk_can_match_ne_never_prunes() {
        // M151 — `<>` is filter-only: a chunk [0,100] must be SCANNED for `col <> 50` even though 50 ∈ [0,100],
        // and equally for a const OUTSIDE the range. `Ne` never proves the chunk empty (fail-safe).
        let (h, lo, hi) = ichunk(0, 100);
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Ne, 50)),
            "col<>50 with 50 inside [0,100] → must scan (Ne never prunes)"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Ne, 999)),
            "col<>999 with 999 outside [0,100] → must scan (Ne never prunes)"
        );
        // even a degenerate all-const chunk [7,7] is NOT pruned by `<> 7` (YAGNI micro-opt not taken).
        let (h2, lo2, hi2) = ichunk(7, 7);
        assert!(
            chunk_can_match(h2, lo2, hi2, MinMaxKind::I4, &ipred(0, ZoneOp::Ne, 7)),
            "col<>7 on all-7 chunk → must scan (conservative; the executor filters it out)"
        );
    }

    #[test]
    fn test_zonemap_failsafe() {
        let (_h, lo, hi) = ichunk(100, 200);
        assert!(
            chunk_can_match(false, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 50)),
            "has_minmax=false → scan"
        );
        assert!(
            chunk_can_match(true, lo, hi, MinMaxKind::None, &ipred(0, ZoneOp::Lt, 50)),
            "kind=None → scan"
        );
        // NaN const (F8) → scan.
        let (h, flo, fhi) = (true, 10.0f64.to_bits(), 20.0f64.to_bits());
        assert!(
            chunk_can_match(h, flo, fhi, MinMaxKind::F8, &fpred(ZoneOp::Gt, f64::NAN)),
            "NaN const → scan"
        );
    }

    #[test]
    fn test_zonemap_signed_and_float() {
        // EC-3: negative I8 chunk [-50,-10]; a raw-u64 compare would order -1 as u64::MAX and decide WRONG.
        let (h, lo, hi) = ichunk(-50, -10);
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Lt, -100)),
            "y<-100 → skip"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Gt, -30)),
            "y>-30 → scan"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Eq, -20)),
            "y=-20 → scan"
        );
        // sanity: the raw-u64 order would be (lo=-50→huge, hi=-10→huge) — the typed decode makes this correct.
        assert_eq!(lo as i64, -50);
        // F8 chunk [-0.0, 5.0] compared as f64 values.
        let (fh, flo, fhi) = (true, (-0.0f64).to_bits(), 5.0f64.to_bits());
        assert!(
            !chunk_can_match(fh, flo, fhi, MinMaxKind::F8, &fpred(ZoneOp::Gt, 5.0)),
            "y>5 → skip"
        );
        assert!(
            chunk_can_match(fh, flo, fhi, MinMaxKind::F8, &fpred(ZoneOp::Lt, 1.0)),
            "y<1 → scan"
        );
    }

    #[test]
    fn test_zonemap_temporal_domain_i8_i4() {
        // Temporal types reuse the I8 (timestamp/timestamptz μs) / I4 (date days) path — the stored bytes ARE the
        // internal int. A chunk group over June 2024 timestamps (μs since 2000-01-01) must prune a Jan-2024 filter.
        let (jan2024, jun2024_lo, jun2024_hi) =
            (757_382_400_000_000i64, 773_020_800_000_000i64, 775_612_800_000_000i64);
        let (h, lo, hi) = ichunk(jun2024_lo, jun2024_hi);
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Lt, jan2024)),
            "ts < Jan2024 → skip (chunk is Jun)"
        );
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Gt, jun2024_hi)),
            "ts > chunk-max → skip"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Ge, jun2024_lo)),
            "ts >= chunk-min → scan"
        );
        // date domain (I4): days since 2000-01-01. 2024-06-01 ≈ day 8918; a chunk [8918,8948] excludes day < 8918.
        let (dh, dlo, dhi) = ichunk(8918, 8948);
        assert!(
            !chunk_can_match(dh, dlo, dhi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 8000)),
            "date < 8000 → skip"
        );
        assert!(
            chunk_can_match(dh, dlo, dhi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 8930)),
            "date = 8930 (in) → scan"
        );
    }

    #[test]
    fn test_zonemap_chunk_with_nulls_and_nan() {
        // EC-4: min/max is over PRESENT non-NaN values only (compute_minmax skips NaN). A chunk whose present
        // values are [10,20] (but that also has NULL/NaN rows): y>100 skip is SAFE (NULL/NaN never match y>100).
        let (h, lo, hi) = (true, 10.0f64.to_bits(), 20.0f64.to_bits());
        assert!(
            !chunk_can_match(h, lo, hi, MinMaxKind::F8, &fpred(ZoneOp::Gt, 100.0)),
            "y>100 → skip (nulls/nan don't match)"
        );
        assert!(
            chunk_can_match(h, lo, hi, MinMaxKind::F8, &fpred(ZoneOp::Gt, 15.0)),
            "y>15 → scan"
        );
        // all-NULL / all-NaN chunk → compute_minmax yields has_minmax=false → never skipped.
        assert!(
            chunk_can_match(false, 0, 0, MinMaxKind::F8, &fpred(ZoneOp::Gt, 100.0)),
            "all-null/nan → scan"
        );
    }
}
