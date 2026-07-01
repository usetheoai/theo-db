//! Polymorphic persisted index (M26 Phase 6): the page/scan/vacuum plumbing is algorithm-agnostic — it stores
//! and dispatches an `IvfflatIndex` OR an `HnswIndex` by the magic in the serialized blob. This lets the two AMs
//! (`theodb_ivfflat`, `theodb_hnsw`) share the identical persistence + scan + maintenance layer.
use crate::ann::{HnswIndex, IvfflatIndex, HNSW_MAGIC, IVF_MAGIC};

/// Lists probed per IVFFlat scan (bounds the candidate set; higher = more recall, slower).
const SCAN_PROBES: usize = 10;
/// HNSW `ef_search` per scan (bounds both recall and result count — its native contract).
const SCAN_EF: usize = 64;

pub(crate) enum Persisted {
    Ivf(IvfflatIndex),
    Hnsw(HnswIndex),
}

impl Persisted {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            Persisted::Ivf(i) => i.to_bytes(),
            Persisted::Hnsw(h) => h.to_bytes(),
        }
    }

    /// Dispatch on the leading 4-byte magic. Typed `Err` on an unknown/truncated blob.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("theodb am: truncated index blob".into());
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic == IVF_MAGIC {
            Ok(Persisted::Ivf(IvfflatIndex::from_bytes(bytes)?))
        } else if magic == HNSW_MAGIC {
            Ok(Persisted::Hnsw(HnswIndex::from_bytes(bytes)?))
        } else {
            Err("theodb am: unknown index magic".into())
        }
    }

    pub(crate) fn entries(&self) -> Vec<(i64, Vec<f32>)> {
        match self {
            Persisted::Ivf(i) => i.entries(),
            Persisted::Hnsw(h) => h.entries(),
        }
    }

    /// Run the ORDER-BY scan with variant-appropriate bounds (the executor applies the real LIMIT on top):
    /// - IVFFlat: `probes` bounds the candidate set; k is unbounded so EVERY probed candidate is returned in
    ///   order (no artificial truncation → an unbounded / large-LIMIT query never silently drops a probed row).
    /// - HNSW: `ef_search` bounds BOTH quality and result count (its native contract) — return the ef best. A
    ///   LIMIT > ef returns ef (standard HNSW; tune via a future ef_search GUC). Passing an unbounded k here would
    ///   poison `ef = ef_search.max(k)` into a full-graph flood, so k is bounded to ef for HNSW.
    pub(crate) fn search_merged(&self, q: &[f32], pending: &[(i64, Vec<f32>)]) -> Vec<(i64, f64)> {
        match self {
            Persisted::Ivf(i) => i.search_merged(q, usize::MAX, SCAN_PROBES, pending),
            Persisted::Hnsw(h) => h.search_merged(q, SCAN_EF, SCAN_EF, pending),
        }
    }

    /// Rebuild the SAME variant over `live` (VACUUM fold), reusing the original's parameters.
    pub(crate) fn rebuilt_with(&self, live: &[(i64, Vec<f32>)], seed: u64) -> Persisted {
        match self {
            Persisted::Ivf(i) => Persisted::Ivf(i.rebuilt_with(live, seed)),
            Persisted::Hnsw(h) => Persisted::Hnsw(h.rebuilt_with(live, seed)),
        }
    }
}
