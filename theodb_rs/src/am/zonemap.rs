//! Zone-map skip-pruning predicate test (plan columnar-zonemap-skip-pruning T1.1). PURE — no `pg_sys` — so the
//! correctness-critical exclusion logic is standalone-`rustc`-testable and isolated from the PG scan plumbing
//! (ADR D1). `theodb_columnar` already WRITES a per-`(chunk_group, column)` min/max into the `ChunkDirEntry`
//! (`columnar_codec::compute_minmax`); this is the missing CONSUMER: given a chunk group's zone-map and a pushed
//! `col <op> const` predicate, decide whether the chunk group can be skipped (no row can match).
//!
//! Encoding contract (MUST match `compute_minmax`): integer kinds (I2/I4/I8/Bool) store the value as `i64`, bits
//! = `i64 as u64`; float kinds (F4/F8) widen to `f64`, bits = `f64::to_bits`. `ZonePredicate::const_bits` is
//! encoded by `admit` in the SAME domain as the column's `MinMaxKind` (ADR D5 — same-domain-or-fallback).
//!
//! Phase 1 of the slice: the pure test is proven off-PG; its production caller (`decode_columns` skip guard +
//! `admit` extraction) lands in Phase 2. `allow(dead_code)` until then (the `ah.rs` precedent).
#![allow(dead_code)]
use super::columnar_codec::MinMaxKind;

/// A pushed btree comparison, reduced to the column's min/max domain (ADR D2/D5). `Var(col) <op> const`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ZoneOp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

/// A single pushable predicate: `column[col] <op> const` where `const_bits` is in the column's `MinMaxKind` domain.
#[derive(Clone, Debug)]
pub(crate) struct ZonePredicate {
    pub col: usize,
    pub op: ZoneOp,
    pub const_bits: u64,
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
            let (min, max, c) = (f64::from_bits(min_bits), f64::from_bits(max_bits), f64::from_bits(pred.const_bits));
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
        ZoneOp::Lt => min >= c, // col < c   : excluded iff every value ≥ c
        ZoneOp::Le => min > c,  // col <= c  : excluded iff every value > c
        ZoneOp::Gt => max <= c, // col > c   : excluded iff every value ≤ c
        ZoneOp::Ge => max < c,  // col >= c  : excluded iff every value < c
        ZoneOp::Eq => c < min || c > max, // col = c : excluded iff c outside [min, max]
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
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 50)), "y<50 → skip");
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Le, 99)), "y<=99 → skip");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 101)), "y<101 → scan");
    }

    #[test]
    fn test_zonemap_excludes_above_range() {
        let (h, lo, hi) = ichunk(100, 200);
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Gt, 200)), "y>200 → skip");
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Ge, 201)), "y>=201 → skip");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Ge, 200)), "y>=200 → scan");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Gt, 199)), "y>199 → scan");
    }

    #[test]
    fn test_zonemap_eq_in_and_out() {
        let (h, lo, hi) = ichunk(100, 200);
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 150)), "y=150 → scan");
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 99)), "y=99 → skip");
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 201)), "y=201 → skip");
        // boundary: c == min and c == max must NOT be excluded.
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 100)), "y=100 (boundary) → scan");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Eq, 200)), "y=200 (boundary) → scan");
    }

    #[test]
    fn test_zonemap_failsafe() {
        let (_h, lo, hi) = ichunk(100, 200);
        assert!(chunk_can_match(false, lo, hi, MinMaxKind::I4, &ipred(0, ZoneOp::Lt, 50)), "has_minmax=false → scan");
        assert!(chunk_can_match(true, lo, hi, MinMaxKind::None, &ipred(0, ZoneOp::Lt, 50)), "kind=None → scan");
        // NaN const (F8) → scan.
        let (h, flo, fhi) = (true, 10.0f64.to_bits(), 20.0f64.to_bits());
        assert!(chunk_can_match(h, flo, fhi, MinMaxKind::F8, &fpred(ZoneOp::Gt, f64::NAN)), "NaN const → scan");
    }

    #[test]
    fn test_zonemap_signed_and_float() {
        // EC-3: negative I8 chunk [-50,-10]; a raw-u64 compare would order -1 as u64::MAX and decide WRONG.
        let (h, lo, hi) = ichunk(-50, -10);
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Lt, -100)), "y<-100 → skip");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Gt, -30)), "y>-30 → scan");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::I8, &ipred(0, ZoneOp::Eq, -20)), "y=-20 → scan");
        // sanity: the raw-u64 order would be (lo=-50→huge, hi=-10→huge) — the typed decode makes this correct.
        assert_eq!(lo as i64, -50);
        // F8 chunk [-0.0, 5.0] compared as f64 values.
        let (fh, flo, fhi) = (true, (-0.0f64).to_bits(), 5.0f64.to_bits());
        assert!(!chunk_can_match(fh, flo, fhi, MinMaxKind::F8, &fpred(ZoneOp::Gt, 5.0)), "y>5 → skip");
        assert!(chunk_can_match(fh, flo, fhi, MinMaxKind::F8, &fpred(ZoneOp::Lt, 1.0)), "y<1 → scan");
    }

    #[test]
    fn test_zonemap_chunk_with_nulls_and_nan() {
        // EC-4: min/max is over PRESENT non-NaN values only (compute_minmax skips NaN). A chunk whose present
        // values are [10,20] (but that also has NULL/NaN rows): y>100 skip is SAFE (NULL/NaN never match y>100).
        let (h, lo, hi) = (true, 10.0f64.to_bits(), 20.0f64.to_bits());
        assert!(!chunk_can_match(h, lo, hi, MinMaxKind::F8, &fpred(ZoneOp::Gt, 100.0)), "y>100 → skip (nulls/nan don't match)");
        assert!(chunk_can_match(h, lo, hi, MinMaxKind::F8, &fpred(ZoneOp::Gt, 15.0)), "y>15 → scan");
        // all-NULL / all-NaN chunk → compute_minmax yields has_minmax=false → never skipped.
        assert!(chunk_can_match(false, 0, 0, MinMaxKind::F8, &fpred(ZoneOp::Gt, 100.0)), "all-null/nan → scan");
    }
}
