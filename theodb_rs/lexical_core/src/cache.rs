//! `IndexCache` — the pgrx-free cache of opened Tantivy indexes, keyed by `(index_id, generation)`.
//!
//! The M139 spike reloaded the whole index from the PG heap on EVERY search (the only "naive" it
//! declared). Production caches the opened `tantivy::Index` per `index_id` and rebuilds it ONLY when
//! the generation changes (M140.3 ADR D1). Living here (pgrx-free, `theodb_lexical`) keeps the
//! invalidation LOGIC unit-testable with stock `cargo test` (the M140.2 win); the pgrx layer supplies
//! the generation (read under the txn snapshot — MVCC-correct) and the `build` closure (heap `load`).
use std::collections::HashMap;

use tantivy::Index;

/// A per-backend cache: `index_id -> (generation the index was built at, the opened Index)`.
#[derive(Default)]
pub struct IndexCache {
    map: HashMap<i64, (u64, Index)>,
}

impl IndexCache {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Return the cached `Index` for `(id, generation)`, calling `build` ONLY when the cached entry
    /// is absent or was built at a different generation. MVCC-correctness (ADR D1) comes from the
    /// caller reading `generation` under its snapshot: a reader on an older snapshot sees an older
    /// generation and rebuilds from the heap state ITS snapshot can see — it never serves a newer
    /// build. A decreasing generation (old-snapshot reader after a newer build touched the cache)
    /// also rebuilds, so a stale newer index is never served to an older snapshot.
    pub fn get_or_build<F: FnOnce() -> Index>(
        &mut self,
        id: i64,
        generation: u64,
        build: F,
    ) -> &Index {
        let stale = self.map.get(&id).map(|(g, _)| *g != generation).unwrap_or(true);
        if stale {
            self.map.insert(id, (generation, build()));
        }
        &self.map.get(&id).expect("entry present after insert/hit").1
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tantivy::schema::{Schema, TEXT};

    fn tiny_index() -> Index {
        let mut sb = Schema::builder();
        sb.add_text_field("body", TEXT);
        Index::create_in_ram(sb.build())
    }

    #[test]
    fn test_cache_hit_same_generation_does_not_rebuild() {
        let mut c = IndexCache::new();
        let calls = Cell::new(0u32);
        c.get_or_build(1, 5, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        c.get_or_build(1, 5, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        assert_eq!(calls.get(), 1, "same generation must reuse the cached index");
    }

    #[test]
    fn test_new_generation_rebuilds_once() {
        let mut c = IndexCache::new();
        let calls = Cell::new(0u32);
        c.get_or_build(1, 5, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        c.get_or_build(1, 6, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        assert_eq!(calls.get(), 2, "a new generation must rebuild");
    }

    #[test]
    fn test_decreasing_generation_rebuilds() {
        // An old-snapshot reader (gen 5) after a newer build (gen 6) touched the cache must NOT be
        // served the newer index — it rebuilds from its own snapshot's state.
        let mut c = IndexCache::new();
        let calls = Cell::new(0u32);
        c.get_or_build(1, 6, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        c.get_or_build(1, 5, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        assert_eq!(calls.get(), 2, "a decreasing generation must rebuild (MVCC correctness)");
    }

    #[test]
    fn test_absent_id_builds() {
        let mut c = IndexCache::new();
        let calls = Cell::new(0u32);
        c.get_or_build(42, 1, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn test_distinct_ids_are_independent() {
        let mut c = IndexCache::new();
        let calls = Cell::new(0u32);
        c.get_or_build(1, 1, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        c.get_or_build(2, 1, || {
            calls.set(calls.get() + 1);
            tiny_index()
        });
        assert_eq!(calls.get(), 2, "distinct index_ids are cached independently");
        assert_eq!(c.len(), 2);
    }
}
