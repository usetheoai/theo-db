//! M35 — page-native structured persistence for `theodb_hnsw`.
//!
//! Replaces the M26 single-blob layout (scan deserializes the whole graph → O(N), `ann/hnsw.rs:243`) with the
//! pgvector-style split-tuple layout so the scan traverses ON DEMAND, reading only visited nodes → O(ef·M) pages.
//!
//! Layout (mirrors pgvector `HnswMetaPageData` / `HnswElementTupleData` / `HnswNeighborTupleData`):
//! - block 0 = **meta**: params + entry point `(blkno,offno,level)` + the four range bounds.
//! - blocks `[1 .. 1+elem_npages)` = **element tuples** (fixed size): `tag,level,tid,nbr_addr,dim,vector`.
//! - blocks `[nbr_first .. nbr_first+nbr_npages)` = **neighbor tuples** (variable, one per node): all layers'
//!   neighbor addresses, laid out top→ground (`start=(level-lc)*m`, ground `m0` slots at the end).
//! - the pending region (INSERTed-after-build rows) follows, located via the meta (unchanged from M26/M31).
//!
//! ADR-1 (blueprint): the graph is IMMUTABLE between VACUUM rebuilds (INSERT→pending, DELETE→rebuild), so there
//! is no on-disk incremental-insert / tombstone / stale-ref machinery — just a codec + an on-demand read path.
//! ADR-2: because the whole graph is in memory at build time, [`pack`] resolves every `(blkno,offno)` up front
//! (element size is fixed → analytic addrs; neighbor tuples packed by a deterministic in-memory packer) and
//! returns fully-formed page images — no placeholder tuple, no `PageIndexTupleOverwrite`.
//!
//! M126 — split into layout/meta/codec/pack/store/search submodules (behavior-preserving).
//! `mod.rs` re-exports every item flat so all `crate::am::hnsw_page::X` call sites are unchanged.
mod layout;
pub(crate) use layout::*;
mod meta;
pub(crate) use meta::*;
mod codec;
pub(crate) use codec::*;
mod pack;
pub(crate) use pack::*;
mod store;
pub(crate) use store::*;
mod search;
pub(crate) use search::*;

#[cfg(any(test, feature = "pg_test"))]
mod tests;
