//! Heap TID ⇄ i64 codec (M26). The AM must return heap TIDs from `amgettuple`, but the reused
//! `crate::ann::IvfflatIndex` stores an `i64` id per vector. We pack the 6-byte `ItemPointerData`
//! (block: u32 as bi_hi<<16|bi_lo, offset: u16) into that `i64` slot at build time and unpack it at scan time.
use pgrx::pg_sys;

/// Encode a heap `ItemPointer` into an i64: `(block << 16) | offset`.
pub(crate) unsafe fn encode(tid: pg_sys::ItemPointer) -> i64 {
    let blkid = (*tid).ip_blkid;
    let block = ((blkid.bi_hi as u32) << 16) | (blkid.bi_lo as u32);
    let offset = (*tid).ip_posid;
    ((block as i64) << 16) | (offset as i64)
}

/// Decode an i64 (from [`encode`]) into the caller-provided `ItemPointerData`.
pub(crate) unsafe fn set_on(v: i64, out: *mut pg_sys::ItemPointerData) {
    let block = (v >> 16) as u32;
    let offset = (v & 0xFFFF) as u16;
    (*out).ip_blkid.bi_hi = (block >> 16) as u16;
    (*out).ip_blkid.bi_lo = (block & 0xFFFF) as u16;
    (*out).ip_posid = offset;
}
