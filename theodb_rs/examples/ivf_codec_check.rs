//! M146 (DoD bullet 4) — standalone check of the PURE IVF-format arithmetic (`am/page/ivf_codec.rs`): the
//! fixed-width label encoding and the chunk-straddle span computation.
//!
//! Links WITHOUT a PostgreSQL runtime by `#[path]`-including the pure module — the same convention as
//! `examples/resumable_check.rs` / `benches/scan_hot_path.rs`, because `cargo test` and `cargo pgrx test` do
//! not link in this crate (undefined PG symbols). This binary IS the test: it panics on any violated
//! invariant and prints OK otherwise.
//!
//! Run: `cargo run --example ivf_codec_check`

#[path = "../src/am/page/ivf_codec.rs"]
mod ivf_codec;

use ivf_codec::{LABEL_K, encode_labels_fixed, record_span};

/// Decode what `encode_labels_fixed` wrote: (declared count, the LABEL_K slots).
fn decode(buf: &[u8]) -> (u16, Vec<i16>) {
    assert_eq!(buf.len(), 2 + LABEL_K * 2, "encoding must be exactly 2 + LABEL_K*2 bytes");
    let n = u16::from_le_bytes(buf[0..2].try_into().unwrap());
    let slots = (0..LABEL_K)
        .map(|i| i16::from_le_bytes(buf[2 + i * 2..4 + i * 2].try_into().unwrap()))
        .collect();
    (n, slots)
}

fn check_labels() {
    // EDGE — empty label set: count 0, every slot padded to 0.
    let mut b = Vec::new();
    encode_labels_fixed(&[], &mut b);
    let (n, slots) = decode(&b);
    assert_eq!(n, 0, "empty set declares 0 labels");
    assert!(slots.iter().all(|&x| x == 0), "empty set pads every slot with 0");

    // A real 0 label must be distinguishable from padding — that is the whole point of the count field.
    let mut b = Vec::new();
    encode_labels_fixed(&[0], &mut b);
    let (n, slots) = decode(&b);
    assert_eq!(n, 1, "one label declared, even though its value is 0");
    assert_eq!(slots[0], 0);

    // EDGE — exactly LABEL_K labels: all kept, nothing truncated.
    let exact: Vec<i16> = (1..=LABEL_K as i16).collect();
    let mut b = Vec::new();
    encode_labels_fixed(&exact, &mut b);
    let (n, slots) = decode(&b);
    assert_eq!(n as usize, LABEL_K, "exactly LABEL_K labels are all declared");
    assert_eq!(slots, exact, "exactly LABEL_K labels round-trip unchanged");

    // NEGATIVE (documented boundary) — overflow truncates to the FIRST LABEL_K, and the declared count
    // saturates at LABEL_K. A count > LABEL_K would make a reader walk past the record.
    let over: Vec<i16> = (1..=(LABEL_K as i16 + 5)).collect();
    let mut b = Vec::new();
    encode_labels_fixed(&over, &mut b);
    let (n, slots) = decode(&b);
    assert_eq!(
        n as usize, LABEL_K,
        "count saturates at LABEL_K — never declares more than the payload holds"
    );
    assert_eq!(
        slots,
        over[..LABEL_K].to_vec(),
        "truncation keeps the FIRST LABEL_K labels, deterministically"
    );

    // Negative label values survive the i16 round-trip (the encoding is signed).
    let mut b = Vec::new();
    encode_labels_fixed(&[-1, i16::MIN, i16::MAX], &mut b);
    let (n, slots) = decode(&b);
    assert_eq!(n, 3);
    assert_eq!(&slots[..3], &[-1, i16::MIN, i16::MAX]);

    // APPEND contract — the function must only ever grow `out`, never touch what is already there.
    // Caught by the /review of M146: every call site here passed a FRESH `Vec`, so a mutation inserting
    // `out.clear()` at the top of the function survived the whole suite. The sole production caller
    // (`am/page/ivf.rs:565`) appends AFTER the `[ids]` region of the v7 blob, so that mutation would zero
    // the id region of every v7 IVF index — silently, at write time. A test that only ever passes an empty
    // buffer cannot see the difference between "append" and "overwrite".
    let mut b = vec![0xAA, 0xBB, 0xCC];
    encode_labels_fixed(&[7, 8], &mut b);
    assert_eq!(&b[..3], &[0xAA, 0xBB, 0xCC], "pre-existing bytes must survive untouched");
    assert_eq!(b.len(), 3 + 2 + LABEL_K * 2, "exactly one record appended after the prefix");
    let (n, slots) = decode(&b[3..]);
    assert_eq!(n, 2, "the appended record decodes independently of the prefix");
    assert_eq!(&slots[..2], &[7, 8]);

    // Appending TWICE (what the v7 writer does for a list with several vectors) yields two independent
    // records back to back — no shared state between calls.
    let mut b = Vec::new();
    encode_labels_fixed(&[1], &mut b);
    encode_labels_fixed(&[2, 3], &mut b);
    assert_eq!(b.len(), 2 * (2 + LABEL_K * 2), "two records, fixed width each");
    let rec = 2 + LABEL_K * 2;
    assert_eq!(decode(&b[..rec]).1[0], 1);
    assert_eq!(&decode(&b[rec..]).1[..2], &[2, 3]);
}

fn check_record_span() {
    const CH: usize = 8000;

    // Record 0 always starts at the beginning of the first chunk.
    let s = record_span(0, 32, CH, 4).expect("ordinal 0 is in range");
    assert_eq!((s.chunk, s.lo), (0, 0));

    // Non-straddling record inside chunk 0: 8000/32 = 250 records fit exactly.
    let s = record_span(249, 32, CH, 4).expect("last record fully inside chunk 0");
    assert_eq!((s.chunk, s.lo), (0, 249 * 32));
    assert!(s.lo + 32 <= CH, "record 249 does not straddle");

    // First record of chunk 1 (the boundary is exact here — 250·32 = 8000).
    let s = record_span(250, 32, CH, 4).expect("first record of chunk 1");
    assert_eq!((s.chunk, s.lo), (1, 0));

    // STRADDLE — reclen that does not divide the chunk: 8000/48 = 166 records + 32 leftover bytes, so
    // record 166 starts at 7968 and needs 16 bytes from the next chunk.
    let s = record_span(166, 48, CH, 4).expect("straddling record with a next chunk available");
    assert_eq!((s.chunk, s.lo), (0, 7968));
    assert!(s.lo + 48 > CH, "record 166 genuinely straddles the chunk boundary");

    // NEGATIVE — the same straddling record when the next chunk does NOT exist: typed error, not a short read.
    let e = record_span(166, 48, CH, 1).expect_err("straddle past the last chunk must fail");
    assert!(e.contains("truncated straddled record"), "unexpected error: {e}");

    // NEGATIVE — ordinal past the range: typed error, never a clamp (a clamp returns plausible bytes for the
    // WRONG vector and corrupts a rerank silently).
    let e = record_span(1000, 32, CH, 4).expect_err("ordinal past the range must fail");
    assert!(e.contains("record ordinal out of range"), "unexpected error: {e}");

    // Boundary of that check: the last valid ordinal of a 4-chunk range vs the first invalid one.
    let last_valid = (4 * CH) / 32 - 1;
    record_span(last_valid, 32, CH, 4).expect("last valid ordinal is accepted");
    let e = record_span(last_valid + 1, 32, CH, 4)
        .expect_err("one past the last valid ordinal must fail");
    assert!(e.contains("record ordinal out of range"), "unexpected error: {e}");

    // NEGATIVE — a record longer than a chunk would span 3+ items; the reader stitches at most 2.
    let e = record_span(0, CH + 1, CH, 8).expect_err("record longer than a chunk must fail");
    assert!(e.contains("longer than one chunk"), "unexpected error: {e}");

    // NEGATIVE — degenerate inputs.
    assert!(record_span(0, 0, CH, 4).is_err(), "zero-length record must fail");
    assert!(record_span(0, 32, 0, 4).is_err(), "zero-length chunk must fail");
    assert!(record_span(0, 32, CH, 0).is_err(), "empty range must fail");
}

fn main() {
    check_labels();
    check_record_span();
    println!("IVF_CODEC_CHECK_OK — label encoding and chunk-straddle span invariants hold");
}
