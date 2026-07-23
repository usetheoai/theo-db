//! Pure (PostgreSQL-free) arithmetic of the IVF/AQ on-disk format — the part of `am/page/ivf.rs` that can be
//! exercised without a running backend.
//!
//! WHY THIS MODULE EXISTS (M146, DoD bullet 4). `am/page/ivf.rs` had zero in-file tests, and the two pieces of
//! logic most likely to be silently wrong are pure arithmetic: the fixed-width label encoding (whose overflow
//! behaviour is a documented, load-bearing truncation) and the chunk-straddle span computation (an off-by-one
//! there reads the wrong bytes for a rerank without ever failing loudly). Both were inlined in a module that
//! imports `pg_sys` 29×, so no test could reach them: `cargo test` / `cargo pgrx test` do not link in this
//! crate (undefined PG symbols), which is why the project's convention is a `#[path]`-included pure module
//! driven by an `examples/` binary — the same trick `ann/scan_core.rs` + `examples/resumable_check.rs` use.
//!
//! Nothing here may import `pgrx`/`pg_sys`; that is the property that makes `examples/ivf_codec_check.rs`
//! link. The I/O that surrounds this arithmetic stays in `ivf.rs`.

/// Number of fixed-width label slots per vector in the v7 layout.
pub(crate) const LABEL_K: usize = 8;

/// M90 — encode one vector's label set into `2 + LABEL_K*2` bytes: a u16 count, then LABEL_K i16 slots (unused
/// slots are 0). The count disambiguates padding from a real 0 label. Overflow (> LABEL_K labels) is truncated
/// to the first LABEL_K (deterministic; documented boundary — the count also saturates at LABEL_K so a reader
/// never trusts a length the payload cannot back).
#[inline]
pub(crate) fn encode_labels_fixed(labels: &[i16], out: &mut Vec<u8>) {
    let n = labels.len().min(LABEL_K) as u16;
    out.extend_from_slice(&n.to_le_bytes());
    for i in 0..LABEL_K {
        let v = if i < labels.len() { labels[i] } else { 0 };
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// Where record `ordinal` (fixed size `reclen`) lives inside a chunked page range: chunk index `chunk`
/// (relative to the range's first block) and local byte offset `lo` within that chunk's item.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RecordSpan {
    pub(crate) chunk: usize,
    pub(crate) lo: usize,
}

/// M83/M85 — locate ONE fixed-size record in a range of `npages` chunks of `chunk_len` bytes each. Records are
/// laid out contiguously across chunks, so record `i` starts at global offset `i·reclen`, lives in chunk
/// `off/chunk_len` at local offset `off%chunk_len`, and straddles into the next chunk when
/// `lo + reclen > chunk_len`.
///
/// Fails with a typed error (never a silent clamp) when the ordinal is past the range — reading a clamped
/// record would return *plausible bytes for the wrong vector*, which corrupts a rerank result silently
/// instead of surfacing it (`rules/error-handling.md § 2`).
pub(crate) fn record_span(
    ordinal: usize,
    reclen: usize,
    chunk_len: usize,
    npages: u32,
) -> Result<RecordSpan, String> {
    if reclen == 0 || chunk_len == 0 {
        return Err("theodb ivf-aq: zero-length record or chunk".into());
    }
    let off = ordinal * reclen;
    let chunk = off / chunk_len;
    let lo = off % chunk_len;
    if chunk as u32 >= npages {
        return Err("theodb ivf-aq: record ordinal out of range".into());
    }
    // A record longer than a whole chunk would span 3+ items; the reader stitches at most 2, so reject it here
    // rather than let the reader return a short buffer.
    if reclen > chunk_len {
        return Err("theodb ivf-aq: record longer than one chunk".into());
    }
    if lo + reclen > chunk_len && (chunk + 1) as u32 >= npages {
        return Err("theodb ivf-aq: truncated straddled record".into());
    }
    Ok(RecordSpan { chunk, lo })
}
