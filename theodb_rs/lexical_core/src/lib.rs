//! `PgDirectory` — implementação NOSSA do trait `Directory` do Tantivy (M139).
//!
//! Modelado no `RamDirectory` do Tantivy (MIT — estudado, adaptado): arquivos endereçados por path, writer que
//! faz flush no `terminate`, `watch` via `WatchCallbackList`. `lock` usa o default do trait; sob single-writer
//! não precisamos de lock real (molde lancedb/tantivy-object-store, Apache-2.0).
//!
//! **O seam de storage (`SegmentStore`)** separa o CONTRATO do trait (aqui, pgrx-free) da FONTE dos bytes:
//! - `MemStore` — backend blob em memória (gate 1: prova o contrato Tantivy↔Directory sem filesystem).
//! - o backend de **páginas PG** (gate 2/3, ADR 0051) implementa o MESMO trait sobre `am/page` (WAL via
//!   `GenericXLog`) + catálogo MVCC — vive em `pg_page_store.rs`, atrás de pgrx. O contrato do trait não muda.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use tantivy::Directory;
use tantivy::directory::OwnedBytes;
use tantivy::directory::error::{DeleteError, OpenReadError, OpenWriteError};
use tantivy::directory::{
    AntiCallToken, FileHandle, TerminatingWrite, WatchCallback, WatchCallbackList, WatchHandle,
    WritePtr,
};

/// A FONTE dos bytes por trás do `Directory`. Um "arquivo" (segmento Tantivy, `meta.json`) é um blob endereçado
/// por path. O seam permite trocar o backend (memória → páginas PG) sem tocar a lógica do trait. Contrato WORM:
/// `write` publica o blob completo de uma vez; `read` nunca vê meia escrita.
pub trait SegmentStore: Send + Sync + fmt::Debug {
    /// Lê o blob de `path`, ou `None` se não existe.
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
    /// Publica (substitui) o blob de `path` atomicamente.
    fn write(&self, path: &Path, data: Vec<u8>);
    /// Remove `path`. Retorna `true` se existia.
    fn delete(&self, path: &Path) -> bool;
    /// `true` se `path` existe.
    fn exists(&self, path: &Path) -> bool;
    /// Total de bytes persistidos — o teste usa para provar que o storage é este backend (não o fs).
    fn total_bytes(&self) -> usize;
}

/// Backend blob em memória (gate 1). Pgrx-free — testável standalone (`cargo test`), padrão M134/egress.
#[derive(Default, Debug)]
pub struct MemStore {
    files: RwLock<HashMap<PathBuf, Vec<u8>>>,
}

impl SegmentStore for MemStore {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.read().unwrap().get(path).cloned()
    }
    fn write(&self, path: &Path, data: Vec<u8>) {
        self.files.write().unwrap().insert(path.to_path_buf(), data);
    }
    fn delete(&self, path: &Path) -> bool {
        self.files.write().unwrap().remove(path).is_some()
    }
    fn exists(&self, path: &Path) -> bool {
        self.files.read().unwrap().contains_key(path)
    }
    fn total_bytes(&self) -> usize {
        self.files.read().unwrap().values().map(|v| v.len()).sum()
    }
}

impl MemStore {
    /// Snapshot de todos os arquivos `(path, bytes)` — usado pelo flush-to-PG (main thread) para persistir o
    /// buffer no heap `bytea` após `writer.commit()`. Retorna cópias (o backend PG serializa fora do lock).
    pub fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        self.files.read().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

/// Um `Directory` do Tantivy cujo storage é NOSSO (`SegmentStore`), não o filesystem. Clonável (Arc) — o Tantivy
/// exige `Directory: Clone + Send + Sync + 'static`. `watch` é nível-Directory (notifica em `atomic_write` de
/// `meta.json`); o storage fica no `SegmentStore`.
#[derive(Clone)]
pub struct PgDirectory {
    store: Arc<dyn SegmentStore>,
    watch: Arc<WatchCallbackList>,
}

impl Default for PgDirectory {
    fn default() -> Self {
        Self::with_store(Arc::new(MemStore::default()))
    }
}

impl PgDirectory {
    /// Backend em memória (gate 1).
    pub fn new() -> Self {
        Self::default()
    }

    /// Injeta um `SegmentStore` arbitrário — o backend de páginas PG (gate 2/3) usa esta porta.
    pub fn with_store(store: Arc<dyn SegmentStore>) -> Self {
        PgDirectory { store, watch: Arc::new(WatchCallbackList::default()) }
    }

    /// Total de bytes no backend — o teste prova que o índice foi para o `SegmentStore` (não o fs): > 0.
    pub fn total_bytes(&self) -> usize {
        self.store.total_bytes()
    }
}

impl fmt::Debug for PgDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PgDirectory(store={:?})", self.store)
    }
}

/// Writer bufferizado que, no `terminate`, publica o blob acumulado no `SegmentStore` (mesmo padrão do
/// `VecWriter` do `RamDirectory`). Fora do `terminate`, nada é visível — reads nunca veem escrita parcial.
struct PgWriter {
    path: PathBuf,
    store: Arc<dyn SegmentStore>,
    buf: Vec<u8>,
}

impl Write for PgWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        // Flush "real" só no terminate — antes disso o arquivo não existe para leitores (WORM).
        Ok(())
    }
}

impl TerminatingWrite for PgWriter {
    fn terminate_ref(&mut self, _: AntiCallToken) -> io::Result<()> {
        let data = std::mem::take(&mut self.buf);
        self.store.write(&self.path, data);
        Ok(())
    }
}

impl Directory for PgDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        let data = self
            .store
            .read(path)
            .ok_or_else(|| OpenReadError::FileDoesNotExist(path.to_path_buf()))?;
        Ok(Arc::new(OwnedBytes::new(data)))
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        if self.store.delete(path) {
            Ok(())
        } else {
            Err(DeleteError::FileDoesNotExist(path.to_path_buf()))
        }
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        Ok(self.store.exists(path))
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        if self.store.exists(path) {
            return Err(OpenWriteError::FileAlreadyExists(path.to_path_buf()));
        }
        let writer =
            PgWriter { path: path.to_path_buf(), store: Arc::clone(&self.store), buf: Vec::new() };
        Ok(BufWriter::new(Box::new(writer)))
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        self.store.read(path).ok_or_else(|| OpenReadError::FileDoesNotExist(path.to_path_buf()))
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        // Atômico por construção: `store.write` publica o blob completo de uma vez — reads nunca veem meia
        // escrita. (No backend de páginas PG, a atomicidade vem de um único `GenericXLogFinish`.)
        self.store.write(path, data.to_vec());
        // O Tantivy detecta mudança de meta.json via watch (`atomic_write` no meta é o sinal).
        self.watch.broadcast();
        Ok(())
    }

    fn sync_directory(&self) -> io::Result<()> {
        // Backend em memória — nada a sincronizar. (No backend PG, isto é o flush de WAL/páginas.)
        Ok(())
    }

    fn watch(&self, watch_callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        Ok(self.watch.subscribe(watch_callback))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{STORED, Schema, TEXT, Value};
    use tantivy::{Index, doc};

    /// Gate 1 do spike M139: o Tantivy indexa 3 docs num `PgDirectory` NOSSO e recupera o certo por busca de
    /// termo — SEM tocar o filesystem. Prova o contrato do trait + a integração Tantivy↔Directory-custom.
    #[test]
    fn test_pg_directory_indexes_and_searches() {
        let mut sb = Schema::builder();
        let body = sb.add_text_field("body", TEXT | STORED);
        let schema = sb.build();

        let dir = PgDirectory::new();
        let index = Index::create(dir.clone(), schema.clone(), tantivy::IndexSettings::default())
            .expect("create index over PgDirectory");

        let mut writer = index.writer(15_000_000).expect("writer");
        writer.add_document(doc!(body => "the quick brown fox")).unwrap();
        writer.add_document(doc!(body => "the lazy dog sleeps")).unwrap();
        writer.add_document(doc!(body => "bright vixens jump")).unwrap();
        writer.commit().expect("commit");

        assert!(dir.total_bytes() > 0, "o índice deve ter sido escrito no SegmentStore, não no fs");

        let reader = index.reader().expect("reader");
        let searcher = reader.searcher();
        let qp = QueryParser::for_index(&index, vec![body]);
        let query = qp.parse_query("lazy").expect("parse");
        let hits =
            searcher.search(&query, &TopDocs::with_limit(3).order_by_score()).expect("search");
        assert_eq!(hits.len(), 1, "só um doc contém 'lazy'");

        let (_score, addr) = hits[0];
        let retrieved: tantivy::TantivyDocument = searcher.doc(addr).expect("doc");
        let got = retrieved.get_first(body).and_then(|v| v.as_str()).unwrap_or("");
        assert!(got.contains("lazy"), "o doc recuperado deve conter 'lazy', got: {got:?}");
    }

    // --- Casos NEGATIVOS e de BORDA do contrato do trait (testing.md § 4.1) ---

    #[test]
    fn test_get_file_handle_absent_is_file_does_not_exist() {
        let dir = PgDirectory::new();
        let err = dir.get_file_handle(Path::new("ausente.term")).unwrap_err();
        assert!(
            matches!(err, OpenReadError::FileDoesNotExist(_)),
            "esperado FileDoesNotExist, got {err:?}"
        );
    }

    #[test]
    fn test_delete_absent_is_file_does_not_exist() {
        let dir = PgDirectory::new();
        let err = dir.delete(Path::new("ausente.idx")).unwrap_err();
        assert!(
            matches!(err, DeleteError::FileDoesNotExist(_)),
            "esperado FileDoesNotExist, got {err:?}"
        );
    }

    #[test]
    fn test_atomic_write_read_roundtrip_and_exists() {
        let dir = PgDirectory::new();
        let p = Path::new("meta.json");
        assert!(!dir.exists(p).unwrap(), "não deve existir antes da escrita");
        dir.atomic_write(p, b"{\"segments\":[]}").unwrap();
        assert!(dir.exists(p).unwrap(), "deve existir após atomic_write");
        assert_eq!(
            dir.atomic_read(p).unwrap(),
            b"{\"segments\":[]}",
            "read deve devolver os bytes exatos"
        );
        dir.atomic_write(p, b"{}").unwrap();
        assert_eq!(dir.atomic_read(p).unwrap(), b"{}", "atomic_write deve substituir, não anexar");
    }

    #[test]
    fn test_open_write_twice_is_file_already_exists() {
        let dir = PgDirectory::new();
        let p = Path::new("seg.store");
        {
            use tantivy::directory::TerminatingWrite;
            let mut w = dir.open_write(p).unwrap();
            w.write_all(b"payload").unwrap();
            w.terminate().unwrap();
        }
        assert!(
            matches!(dir.open_write(p), Err(OpenWriteError::FileAlreadyExists(_))),
            "esperado FileAlreadyExists ao reabrir um path já persistido"
        );
    }

    /// O seam funciona: um `SegmentStore` injetado (aqui um `MemStore` explícito) suporta o mesmo fluxo — prova
    /// que o backend de páginas PG (gate 2/3) plugará pela mesma porta sem tocar a lógica do trait.
    #[test]
    fn test_directory_over_injected_store() {
        let store = Arc::new(MemStore::default());
        let dir = PgDirectory::with_store(store);
        dir.atomic_write(Path::new("x"), b"hello").unwrap();
        assert_eq!(dir.atomic_read(Path::new("x")).unwrap(), b"hello");
        assert_eq!(dir.total_bytes(), 5);
    }
}
