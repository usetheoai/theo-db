//! Probe #153 — regressão de thread-safety: registra QUAIS threads chamam o `SegmentStore` durante um
//! build real do Tantivy, e prova a **separação estrutural** que a disciplina #153 exige.
//!
//! O achado do M139 (review council-rust-pgrx): o Tantivy chama o `Directory` de MÚLTIPLAS threads
//! (rayon/merge). A regra de segurança é que o store NO CAMINHO DESSAS THREADS jamais toque o PG (SPI/
//! pg_sys) — só memória (`MemStore`); o flush/load (pgrx) é main-thread. Este probe TRAVA isso: o
//! `ThreadRecordingStore` vive no crate **pgrx-free** (`theodb_lexical`), então é **impossível** por
//! construção um `SegmentStore` do núcleo tocar o PG de qualquer thread — o crate nem linka pgrx. O teste
//! documenta que o Directory É chamado de threads e que esse caminho é PG-free. Uma regressão que ponha
//! SPI numa worker thread teria de mover código para fora do núcleo → pega no review + no gate zero-pgrx.
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::thread::ThreadId;

use crate::{MemStore, SegmentStore};

/// Um `SegmentStore` que delega a um `MemStore` e registra o `ThreadId` de cada chamada.
#[derive(Debug, Default)]
pub struct ThreadRecordingStore {
    inner: MemStore,
    threads: Mutex<HashSet<ThreadId>>,
}

impl ThreadRecordingStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self) {
        self.threads.lock().unwrap().insert(std::thread::current().id());
    }

    /// Quantas threads distintas chamaram o store (o Tantivy chama de rayon/merge workers).
    pub fn thread_count(&self) -> usize {
        self.threads.lock().unwrap().len()
    }
}

impl SegmentStore for ThreadRecordingStore {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        self.record();
        self.inner.read(path)
    }
    fn write(&self, path: &Path, data: Vec<u8>) {
        self.record();
        self.inner.write(path, data)
    }
    fn delete(&self, path: &Path) -> bool {
        self.record();
        self.inner.delete(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.record();
        self.inner.exists(path)
    }
    fn total_bytes(&self) -> usize {
        self.inner.total_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PgDirectory;
    use std::sync::Arc;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{STORED, Schema, TEXT, Value};
    use tantivy::{Index, TantivyDocument, doc};

    fn build_index(store: Arc<ThreadRecordingStore>, n_docs: usize, threads: usize) {
        let mut sb = Schema::builder();
        let body = sb.add_text_field("body", TEXT | STORED);
        let schema = sb.build();
        let index = Index::create(
            PgDirectory::with_store(store),
            schema,
            tantivy::IndexSettings::default(),
        )
        .expect("create");
        // o Tantivy exige ≥15MB de arena POR thread; o heap total escala com o nº de threads.
        let mut w =
            index.writer_with_num_threads(threads, threads.max(1) * 15_000_000).expect("writer");
        for i in 0..n_docs {
            w.add_document(doc!(body => format!("documento numero {i} sobre raposas preguicosas rapidas {}", i % 13)))
                .expect("add");
            if i % 200 == 199 {
                w.commit().expect("commit"); // múltiplos commits → múltiplos segmentos → merges
            }
        }
        w.commit().expect("final commit");
    }

    #[test]
    fn test_directory_called_from_multiple_threads_all_pgrx_free() {
        // DEMONSTRA o hazard #153 real: um build multi-thread (4 threads + commits repetidos → merges)
        // chama o `SegmentStore` de MÚLTIPLAS threads. A GARANTIA de segurança, porém, é ESTRUTURAL e
        // imposta pelo CI, não por este record: o `ThreadRecordingStore` vive no crate pgrx-free
        // (`theodb_lexical`), e `lint-rust.yml` falha o build se `cargo tree -p theodb_lexical | grep -c
        // pgrx != 0`. Logo é impossível por construção um `SegmentStore` do núcleo tocar o PG (SPI/pg_sys)
        // de QUALQUER thread — uma regressão teria de mover código para fora do núcleo (pega no gate + review).
        let store = Arc::new(ThreadRecordingStore::new());
        build_index(store.clone(), 2000, 4);
        assert!(
            store.thread_count() > 1,
            "o Tantivy deve chamar o Directory de >1 thread num build multi-thread (registrado: {})",
            store.thread_count()
        );
    }

    #[test]
    fn test_recorded_index_is_searchable_over_injected_store() {
        // O caminho pgrx-free produz um índice funcional (o build multi-thread escreve segmentos via o store).
        let store = Arc::new(ThreadRecordingStore::new());
        let mut sb = Schema::builder();
        let body = sb.add_text_field("body", TEXT | STORED);
        let schema = sb.build();
        let index = Index::create(
            PgDirectory::with_store(store.clone()),
            schema,
            tantivy::IndexSettings::default(),
        )
        .expect("create");
        {
            let mut w = index.writer_with_num_threads(2, 30_000_000).expect("writer");
            w.add_document(doc!(body => "a raposa marromxyz salta")).expect("add");
            w.add_document(doc!(body => "o cachorro preguicoso dorme")).expect("add");
            w.commit().expect("commit");
        }
        let reader = index.reader().expect("reader");
        let searcher = reader.searcher();
        let body_f = index.schema().get_field("body").unwrap();
        let qp = QueryParser::for_index(&index, vec![body_f]);
        let q = qp.parse_query("marromxyz").expect("parse");
        let hits = searcher.search(&q, &TopDocs::with_limit(10).order_by_score()).expect("search");
        assert_eq!(hits.len(), 1, "o termo raro casa exatamente 1 doc");
        let (_s, addr) = hits[0];
        let d: TantivyDocument = searcher.doc(addr).expect("doc");
        assert!(d.get_first(body_f).and_then(|v| v.as_str()).unwrap().contains("marromxyz"));
        // o build multi-thread exerceu o store; o total_bytes > 0 prova que os bytes vieram do store injetado
        assert!(store.total_bytes() > 0);
    }
}
