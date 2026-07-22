//! M139 SPIKE — engine lexical própria sobre Tantivy, vivendo dentro do Postgres.
//!
//! A pergunta-gate do spike (blueprint `tantivy-directory-spike`): conseguimos implementar o trait
//! `Directory` do Tantivy sobre storage do Postgres, com MVCC + WAL, sobrevivendo a crash real? Este módulo
//! avança os gates EM ORDEM. **Gate 1 (provado):** um `Directory` NOSSO indexa+busca sem filesystem. **Gate 2/3
//! (ADR 0051):** o `SegmentStore` de páginas PG (WAL via `GenericXLog`, MVCC-via-catálogo) pluga pela mesma
//! porta. Tudo atrás da feature `spike-lexical` para não inchar o crate shipado antes do veredito GO.

pub mod pg_directory;

pub use pg_directory::{MemStore, PgDirectory, SegmentStore};
