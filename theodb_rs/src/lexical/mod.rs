//! M139 SPIKE — engine lexical própria sobre Tantivy, vivendo dentro do Postgres.
//!
//! A pergunta-gate do spike (blueprint `tantivy-directory-spike`): conseguimos implementar o trait
//! `Directory` do Tantivy sobre storage do Postgres, com MVCC + WAL, sobrevivendo a crash real? Este módulo
//! avança os gates EM ORDEM. **Gate 1 (este passo):** um `Directory` NOSSO (não `MmapDirectory`) indexa N docs
//! e recupera o certo por busca — **sem tocar o filesystem**. Backend: blob em memória, com seam para páginas
//! PG (gate 3). Tudo atrás da feature `spike-lexical` para não inchar o crate shipado antes do veredito GO.

pub mod pg_directory;
