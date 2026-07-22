//! M139 SPIKE — engine lexical própria sobre Tantivy, vivendo dentro do Postgres.
//!
//! A pergunta-gate do spike (blueprint `tantivy-directory-spike`): conseguimos implementar o trait
//! `Directory` do Tantivy sobre storage do Postgres, com MVCC + WAL, sobrevivendo a crash real? Este módulo
//! avança os gates EM ORDEM. **Gate 1 (provado):** um `Directory` NOSSO indexa+busca sem filesystem. **Gate 2/3
//! (ADR 0051):** o `SegmentStore` de páginas PG (WAL via `GenericXLog`, MVCC-via-catálogo) pluga pela mesma
//! porta. Tudo atrás da feature `spike-lexical` para não inchar o crate shipado antes do veredito GO.

pub mod pg_backing;

// M140.3 — a superfície BM25 de produção (bm25_build + bm25_search) com o cache do Directory.
pub mod engine;

// M140.2 — o núcleo pgrx-free (`MemStore`/`PgDirectory`/`SegmentStore`) mudou para o crate
// `theodb_lexical` (testável com `cargo test` stock, sem link pgrx). Re-exportado aqui para
// preservar os caminhos `crate::lexical::{MemStore,...}` de eventuais consumidores.
pub use theodb_lexical::{MemStore, PgDirectory, SegmentStore};
