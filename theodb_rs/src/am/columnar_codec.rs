//! M99 Phase C — pure column-major codec for the `theodb_columnar` TableAM stripe payload.
//!
//! This module is **pure Rust** (no pgrx callback, no C FFI): given per-column raw value bytes + null flags it
//! produces the on-disk byte layout, and vice-versa. Keeping it FFI-free means the bit-layout logic gets fast local
//! `#[test]` coverage (no droplet build), and the dangerous FFI (datum extraction / detoast / tuple reconstruction)
//! stays isolated in `columnar.rs`. Design reviewed by council-index-storage (on-disk format) + council-rust-pgrx
//! (the FFI boundary that feeds this codec).
//!
//! **On-disk stripe layout (magic `TCS1`):**
//! ```text
//! stripe = [ column chunk pages... ][ directory pages ][ stripe_header page (1 item) ]
//!   header  → points at the directory (dir_first_block/dir_n_pages)
//!   directory → grid of ChunkDirEntry, indexed [chunk_group * ncols + col]; each entry points at its column chunk
//!   column chunk (on disk) = zstd( [null_bitmap?][value_stream] )
//! ```
//! The `StripeDesc` in the metapage (unchanged 28-byte format) points at the header block; the header is written
//! LAST and the metapage `StripeDesc` is appended last of all — so a stripe only becomes visible once every byte it
//! references is durable (the crash-safety invariant council-index-storage flagged as sacred).

/// "TCS1" — Theo Columnar Stripe v1. Distinguishes a column-major stripe from the retired row-major zstd blob.
pub(crate) const STRIPE_MAGIC: u32 = 0x54_43_53_31;
pub(crate) const STRIPE_VERSION: u32 = 1;
/// Rows per chunk group — the skip-pruning granule (min/max is per chunk group per column). Hydra/Citus use 10k.
pub(crate) const CHUNK_GROUP_ROWS: usize = 10_000;

pub(crate) const STRIPE_HEADER_LEN: usize = 40;
pub(crate) const CHUNK_DIR_ENTRY_LEN: usize = 44;

/// Directory-entry flag bits.
const FLAG_HAS_NULLS: u32 = 1 << 0;
const FLAG_HAS_MINMAX: u32 = 1 << 1;
const FLAG_ALL_NULL: u32 = 1 << 2;

/// The min/max comparison domain of a column, derived from its `atttypid` in the FFI layer. `None` means the type
/// has no cheap native order here → the entry stores no min/max and a later pruner treats the chunk as "cannot skip"
/// (fail-safe, never fail-wrong — the council-index-storage fallback).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum MinMaxKind {
    None,
    I2,
    I4,
    I8,
    F4,
    F8,
    Bool,
}

/// The stripe header (fixed 40 bytes), written as the stripe's last page item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StripeHeader {
    pub ncols: u16,
    pub n_chunk_groups: u32,
    pub row_count: u32,
    pub first_row_number: u64,
    pub dir_first_block: u32,
    pub dir_n_pages: u32,
    pub dir_len: u32,
}

impl StripeHeader {
    pub fn to_bytes(&self) -> [u8; STRIPE_HEADER_LEN] {
        let mut b = [0u8; STRIPE_HEADER_LEN];
        b[0..4].copy_from_slice(&STRIPE_MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&STRIPE_VERSION.to_le_bytes());
        b[8..10].copy_from_slice(&self.ncols.to_le_bytes());
        // b[10..12] reserved
        b[12..16].copy_from_slice(&self.n_chunk_groups.to_le_bytes());
        b[16..20].copy_from_slice(&self.row_count.to_le_bytes());
        b[20..28].copy_from_slice(&self.first_row_number.to_le_bytes());
        b[28..32].copy_from_slice(&self.dir_first_block.to_le_bytes());
        b[32..36].copy_from_slice(&self.dir_n_pages.to_le_bytes());
        b[36..40].copy_from_slice(&self.dir_len.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() < STRIPE_HEADER_LEN {
            return Err(format!("columnar stripe header too short ({} < {STRIPE_HEADER_LEN})", b.len()));
        }
        let magic = u32::from_le_bytes(b[0..4].try_into().unwrap());
        if magic != STRIPE_MAGIC {
            return Err(format!("columnar stripe bad magic {magic:#x} (expected {STRIPE_MAGIC:#x})"));
        }
        let version = u32::from_le_bytes(b[4..8].try_into().unwrap());
        if version != STRIPE_VERSION {
            return Err(format!("columnar stripe unsupported version {version}"));
        }
        Ok(StripeHeader {
            ncols: u16::from_le_bytes(b[8..10].try_into().unwrap()),
            n_chunk_groups: u32::from_le_bytes(b[12..16].try_into().unwrap()),
            row_count: u32::from_le_bytes(b[16..20].try_into().unwrap()),
            first_row_number: u64::from_le_bytes(b[20..28].try_into().unwrap()),
            dir_first_block: u32::from_le_bytes(b[28..32].try_into().unwrap()),
            dir_n_pages: u32::from_le_bytes(b[32..36].try_into().unwrap()),
            dir_len: u32::from_le_bytes(b[36..40].try_into().unwrap()),
        })
    }
}

/// One `(chunk_group, column)` cell of the stripe directory (fixed 44 bytes): where the column chunk lives + its
/// per-chunk stats (null_count for reconstruction; min/max for skip-pruning consumed in C2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChunkDirEntry {
    pub first_block: u32,
    pub n_pages: u32,
    pub comp_len: u32,
    pub raw_len: u32,
    pub row_count: u32,
    pub null_count: u32,
    pub has_nulls: bool,
    pub has_minmax: bool,
    pub all_null: bool,
    pub min_bits: u64,
    pub max_bits: u64,
}

impl ChunkDirEntry {
    fn flags(&self) -> u32 {
        let mut f = 0u32;
        if self.has_nulls {
            f |= FLAG_HAS_NULLS;
        }
        if self.has_minmax {
            f |= FLAG_HAS_MINMAX;
        }
        if self.all_null {
            f |= FLAG_ALL_NULL;
        }
        f
    }

    fn write_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.first_block.to_le_bytes());
        out.extend_from_slice(&self.n_pages.to_le_bytes());
        out.extend_from_slice(&self.comp_len.to_le_bytes());
        out.extend_from_slice(&self.raw_len.to_le_bytes());
        out.extend_from_slice(&self.row_count.to_le_bytes());
        out.extend_from_slice(&self.null_count.to_le_bytes());
        out.extend_from_slice(&self.flags().to_le_bytes());
        out.extend_from_slice(&self.min_bits.to_le_bytes());
        out.extend_from_slice(&self.max_bits.to_le_bytes());
    }

    fn read_from(b: &[u8]) -> Self {
        let flags = u32::from_le_bytes(b[24..28].try_into().unwrap());
        ChunkDirEntry {
            first_block: u32::from_le_bytes(b[0..4].try_into().unwrap()),
            n_pages: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            comp_len: u32::from_le_bytes(b[8..12].try_into().unwrap()),
            raw_len: u32::from_le_bytes(b[12..16].try_into().unwrap()),
            row_count: u32::from_le_bytes(b[16..20].try_into().unwrap()),
            null_count: u32::from_le_bytes(b[20..24].try_into().unwrap()),
            has_nulls: flags & FLAG_HAS_NULLS != 0,
            has_minmax: flags & FLAG_HAS_MINMAX != 0,
            all_null: flags & FLAG_ALL_NULL != 0,
            min_bits: u64::from_le_bytes(b[28..36].try_into().unwrap()),
            max_bits: u64::from_le_bytes(b[36..44].try_into().unwrap()),
        }
    }
}

/// Serialize the `[chunk_group][col]` grid of directory entries (fixed stride → O(1) seek in the reader).
pub(crate) fn serialize_directory(entries: &[ChunkDirEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * CHUNK_DIR_ENTRY_LEN);
    for e in entries {
        e.write_into(&mut out);
    }
    out
}

/// Decode `count` fixed-stride directory entries. `count` = n_chunk_groups * ncols (from the header) — never a
/// length read out of the untrusted bytes.
pub(crate) fn deserialize_directory(b: &[u8], count: usize) -> Result<Vec<ChunkDirEntry>, String> {
    if b.len() < count * CHUNK_DIR_ENTRY_LEN {
        return Err(format!(
            "columnar directory truncated ({} < {} entries × {CHUNK_DIR_ENTRY_LEN})",
            b.len(),
            count
        ));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * CHUNK_DIR_ENTRY_LEN;
        out.push(ChunkDirEntry::read_from(&b[off..off + CHUNK_DIR_ENTRY_LEN]));
    }
    Ok(out)
}

/// The uncompressed bytes + stats of one encoded column chunk (before zstd). The FFI layer compresses `raw` and
/// fills a `ChunkDirEntry` from the stat fields.
pub(crate) struct EncodedColumn {
    pub raw: Vec<u8>,
    pub null_count: u32,
    pub has_nulls: bool,
    pub has_minmax: bool,
    pub all_null: bool,
    pub min_bits: u64,
    pub max_bits: u64,
}

/// Encode one column of a chunk group: `values[i]` is `Some(raw_bytes)` for a present value or `None` for SQL NULL.
/// For a fixed-width column every `Some` is exactly `attlen_fixed` bytes; for varlena each `Some` is the value's full
/// VARSIZE bytes (header + data). Layout: `[null_bitmap?][value_stream]` where the bitmap (1 = present) is emitted
/// only when there is ≥1 null, and the value stream packs only present values (NULLs take no stream bytes).
pub(crate) fn encode_column(values: &[Option<Vec<u8>>], attlen_fixed: Option<usize>, mm: MinMaxKind) -> EncodedColumn {
    let n = values.len();
    let null_count = values.iter().filter(|v| v.is_none()).count() as u32;
    let has_nulls = null_count > 0;
    let all_null = null_count as usize == n && n > 0;

    let mut raw = Vec::new();
    if has_nulls {
        let bitmap_len = n.div_ceil(8);
        let mut bitmap = vec![0u8; bitmap_len];
        for (i, v) in values.iter().enumerate() {
            if v.is_some() {
                bitmap[i / 8] |= 1 << (i % 8);
            }
        }
        raw.extend_from_slice(&bitmap);
    }
    // value_stream: present values only.
    for v in values.iter().flatten() {
        match attlen_fixed {
            Some(_len) => raw.extend_from_slice(v), // caller guarantees each is exactly `attlen` bytes
            None => {
                raw.extend_from_slice(&(v.len() as u32).to_le_bytes());
                raw.extend_from_slice(v);
            }
        }
    }

    let (has_minmax, min_bits, max_bits) = compute_minmax(values, mm);
    EncodedColumn { raw, null_count, has_nulls, has_minmax: has_minmax && !all_null, all_null, min_bits, max_bits }
}

/// Decode a column chunk's uncompressed `raw` bytes back into `row_count` `Option<Vec<u8>>` values. `attlen_fixed`
/// and `has_nulls` come from the live tupdesc + directory entry — never from the untrusted bytes. Every bounds check
/// returns a typed `Err` (→ `pg_sys::error!` in the caller), never a panic across C.
pub(crate) fn decode_column(
    raw: &[u8],
    attlen_fixed: Option<usize>,
    row_count: usize,
    has_nulls: bool,
) -> Result<Vec<Option<Vec<u8>>>, String> {
    let mut off = 0usize;
    let present: Vec<bool> = if has_nulls {
        let bitmap_len = row_count.div_ceil(8);
        if raw.len() < bitmap_len {
            return Err(format!("columnar chunk null-bitmap truncated ({} < {bitmap_len})", raw.len()));
        }
        let bitmap = &raw[..bitmap_len];
        off = bitmap_len;
        (0..row_count).map(|i| bitmap[i / 8] & (1 << (i % 8)) != 0).collect()
    } else {
        vec![true; row_count]
    };

    let mut out = Vec::with_capacity(row_count);
    for &is_present in &present {
        if !is_present {
            out.push(None);
            continue;
        }
        match attlen_fixed {
            Some(len) => {
                if off + len > raw.len() {
                    return Err(format!("columnar fixed value truncated (off {off} + {len} > {})", raw.len()));
                }
                out.push(Some(raw[off..off + len].to_vec()));
                off += len;
            }
            None => {
                if off + 4 > raw.len() {
                    return Err("columnar varlena length header truncated".into());
                }
                let len = u32::from_le_bytes(raw[off..off + 4].try_into().unwrap()) as usize;
                off += 4;
                if off + len > raw.len() {
                    return Err(format!("columnar varlena body truncated (off {off} + {len} > {})", raw.len()));
                }
                out.push(Some(raw[off..off + len].to_vec()));
                off += len;
            }
        }
    }
    Ok(out)
}

/// Compute (has_minmax, min_bits, max_bits) over the present values for a native-ordered numeric kind. Non-numeric
/// kinds (`None`), or all-NaN float columns, yield `has_minmax=false` (the pruner then cannot skip — fail-safe).
/// Integers are stored as their i64 two's-complement bits; floats (incl. f4 widened) as f64 bits.
fn compute_minmax(values: &[Option<Vec<u8>>], mm: MinMaxKind) -> (bool, u64, u64) {
    match mm {
        MinMaxKind::None => (false, 0, 0),
        MinMaxKind::I2 | MinMaxKind::I4 | MinMaxKind::I8 | MinMaxKind::Bool => {
            let mut lo: Option<i64> = None;
            let mut hi: Option<i64> = None;
            for v in values.iter().flatten() {
                let x = match mm {
                    MinMaxKind::Bool => *v.first().unwrap_or(&0) as i64,
                    MinMaxKind::I2 if v.len() >= 2 => i16::from_le_bytes([v[0], v[1]]) as i64,
                    MinMaxKind::I4 if v.len() >= 4 => i32::from_le_bytes([v[0], v[1], v[2], v[3]]) as i64,
                    MinMaxKind::I8 if v.len() >= 8 => {
                        i64::from_le_bytes(v[..8].try_into().unwrap())
                    }
                    _ => continue, // malformed width → skip this value's contribution
                };
                lo = Some(lo.map_or(x, |c| c.min(x)));
                hi = Some(hi.map_or(x, |c| c.max(x)));
            }
            match (lo, hi) {
                (Some(l), Some(h)) => (true, l as u64, h as u64),
                _ => (false, 0, 0),
            }
        }
        MinMaxKind::F4 | MinMaxKind::F8 => {
            let mut lo: Option<f64> = None;
            let mut hi: Option<f64> = None;
            for v in values.iter().flatten() {
                let x = match mm {
                    MinMaxKind::F4 if v.len() >= 4 => {
                        f32::from_le_bytes([v[0], v[1], v[2], v[3]]) as f64
                    }
                    MinMaxKind::F8 if v.len() >= 8 => f64::from_le_bytes(v[..8].try_into().unwrap()),
                    _ => continue,
                };
                if x.is_nan() {
                    continue;
                }
                lo = Some(lo.map_or(x, |c| c.min(x)));
                hi = Some(hi.map_or(x, |c| c.max(x)));
            }
            match (lo, hi) {
                (Some(l), Some(h)) => (true, l.to_bits(), h.to_bits()),
                _ => (false, 0, 0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i32v(x: i32) -> Option<Vec<u8>> {
        Some(x.to_le_bytes().to_vec())
    }

    #[test]
    fn fixed_column_roundtrips_no_nulls() {
        let vals: Vec<Option<Vec<u8>>> = (0..25i32).map(i32v).collect();
        let enc = encode_column(&vals, Some(4), MinMaxKind::I4);
        assert!(!enc.has_nulls);
        assert_eq!(enc.null_count, 0);
        let dec = decode_column(&enc.raw, Some(4), vals.len(), enc.has_nulls).unwrap();
        assert_eq!(dec, vals);
    }

    #[test]
    fn fixed_column_roundtrips_with_nulls() {
        let vals: Vec<Option<Vec<u8>>> =
            vec![i32v(10), None, i32v(30), None, None, i32v(60)];
        let enc = encode_column(&vals, Some(4), MinMaxKind::I4);
        assert!(enc.has_nulls);
        assert_eq!(enc.null_count, 3);
        let dec = decode_column(&enc.raw, Some(4), vals.len(), enc.has_nulls).unwrap();
        assert_eq!(dec, vals);
    }

    #[test]
    fn varlena_column_roundtrips_with_nulls() {
        let vals: Vec<Option<Vec<u8>>> = vec![
            Some(b"hello".to_vec()),
            None,
            Some(b"a much longer value than the first".to_vec()),
            Some(Vec::new()), // empty-but-present (distinct from NULL)
        ];
        let enc = encode_column(&vals, None, MinMaxKind::None);
        assert!(enc.has_nulls);
        assert!(!enc.has_minmax);
        let dec = decode_column(&enc.raw, None, vals.len(), enc.has_nulls).unwrap();
        assert_eq!(dec, vals);
    }

    #[test]
    fn minmax_i4_correct_over_present_only() {
        let vals: Vec<Option<Vec<u8>>> = vec![i32v(5), None, i32v(-3), i32v(42), i32v(0)];
        let enc = encode_column(&vals, Some(4), MinMaxKind::I4);
        assert!(enc.has_minmax);
        assert_eq!(enc.min_bits as i64, -3);
        assert_eq!(enc.max_bits as i64, 42);
    }

    #[test]
    fn minmax_f8_correct_and_skips_nan() {
        let vals: Vec<Option<Vec<u8>>> = vec![
            Some(1.5f64.to_le_bytes().to_vec()),
            Some(f64::NAN.to_le_bytes().to_vec()),
            Some((-2.25f64).to_le_bytes().to_vec()),
        ];
        let enc = encode_column(&vals, Some(8), MinMaxKind::F8);
        assert!(enc.has_minmax);
        assert_eq!(f64::from_bits(enc.min_bits), -2.25);
        assert_eq!(f64::from_bits(enc.max_bits), 1.5);
    }

    #[test]
    fn all_null_column_has_no_minmax() {
        let vals: Vec<Option<Vec<u8>>> = vec![None, None, None];
        let enc = encode_column(&vals, Some(4), MinMaxKind::I4);
        assert!(enc.all_null);
        assert!(!enc.has_minmax);
        let dec = decode_column(&enc.raw, Some(4), vals.len(), enc.has_nulls).unwrap();
        assert_eq!(dec, vals);
    }

    #[test]
    fn header_roundtrips() {
        let h = StripeHeader {
            ncols: 3,
            n_chunk_groups: 5,
            row_count: 42000,
            first_row_number: 1_000_000,
            dir_first_block: 7,
            dir_n_pages: 2,
            dir_len: 660,
        };
        assert_eq!(StripeHeader::from_bytes(&h.to_bytes()).unwrap(), h);
    }

    #[test]
    fn bad_magic_is_typed_error() {
        let mut b = [0u8; STRIPE_HEADER_LEN];
        b[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        assert!(StripeHeader::from_bytes(&b).unwrap_err().contains("bad magic"));
    }

    #[test]
    fn directory_roundtrips() {
        let entries = vec![
            ChunkDirEntry {
                first_block: 1,
                n_pages: 1,
                comp_len: 100,
                raw_len: 200,
                row_count: 10000,
                null_count: 0,
                has_nulls: false,
                has_minmax: true,
                all_null: false,
                min_bits: 0,
                max_bits: 9999,
            },
            ChunkDirEntry {
                first_block: 2,
                n_pages: 3,
                comp_len: 24000,
                raw_len: 80000,
                row_count: 5000,
                null_count: 12,
                has_nulls: true,
                has_minmax: false,
                all_null: false,
                min_bits: 0,
                max_bits: 0,
            },
        ];
        let bytes = serialize_directory(&entries);
        assert_eq!(deserialize_directory(&bytes, entries.len()).unwrap(), entries);
    }

    #[test]
    fn decode_rejects_truncated_directory() {
        assert!(deserialize_directory(&[0u8; 10], 1).is_err());
    }

    #[test]
    fn decode_rejects_truncated_fixed_stream() {
        // has_nulls=false, row_count=3, attlen=4 needs 12 bytes; give 6.
        assert!(decode_column(&[0u8; 6], Some(4), 3, false).is_err());
    }
}
