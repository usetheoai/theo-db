//! `PgDirectory` — implementação NOSSA do trait `Directory` do Tantivy (M139 gate 1).
//!
//! Modelado no `RamDirectory` do Tantivy (MIT — estudado, adaptado): arquivos num `HashMap<PathBuf, Vec<u8>>`
//! sob `RwLock`, writer que faz flush no `terminate`, `watch` via `WatchCallbackList`. `lock` usa o default do
//! trait; sob single-writer não precisamos de lock real (molde lancedb/tantivy-object-store, Apache-2.0).
//!
//! **Gate 1 vs gate 3:** aqui o backend é um blob em memória (`Inner.files`). O seam para PG é deliberado — no
//! gate 3 o `HashMap<PathBuf, Vec<u8>>` vira leitura/escrita de páginas via o buffer manager + `GenericXLog`
//! (precedente próprio: `am/hnsw_page.rs`). O contrato do trait não muda; só a fonte dos bytes.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use tantivy::directory::error::{DeleteError, OpenReadError, OpenWriteError};
use tantivy::directory::{
    AntiCallToken, FileHandle, TerminatingWrite, WatchCallback, WatchCallbackList, WatchHandle, WritePtr,
};
use tantivy::directory::OwnedBytes;
use tantivy::Directory;

/// Estado compartilhado: os arquivos (blob backend do gate 1) + a lista de watchers de `meta.json`.
#[derive(Default)]
struct Inner {
    files: HashMap<PathBuf, Vec<u8>>,
    watch_router: WatchCallbackList,
}

/// Um `Directory` do Tantivy cujo storage é NOSSO (não o filesystem). Clonável (Arc) — o Tantivy exige
/// `Directory: Clone + Send + Sync + 'static`.
#[derive(Clone, Default)]
pub struct PgDirectory {
    inner: Arc<RwLock<Inner>>,
}

impl PgDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total de bytes persistidos no backend — usado pelo teste para provar que o storage é o `PgDirectory`
    /// (e não o filesystem): após indexar, isto é > 0.
    pub fn total_bytes(&self) -> usize {
        self.inner.read().unwrap().files.values().map(|v| v.len()).sum()
    }
}

impl fmt::Debug for PgDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PgDirectory(spike, in-memory blob backend)")
    }
}

/// Writer bufferizado que, no `terminate`, grava o blob acumulado no backend (mesmo padrão do `VecWriter` do
/// `RamDirectory`). Fora do `terminate`, nada é visível — reads nunca veem escrita parcial.
struct PgWriter {
    path: PathBuf,
    inner: Arc<RwLock<Inner>>,
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
        self.inner.write().unwrap().files.insert(self.path.clone(), data);
        Ok(())
    }
}

impl Directory for PgDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        let inner = self.inner.read().unwrap();
        let data = inner
            .files
            .get(path)
            .ok_or_else(|| OpenReadError::FileDoesNotExist(path.to_path_buf()))?;
        Ok(Arc::new(OwnedBytes::new(data.clone())))
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        let mut inner = self.inner.write().unwrap();
        inner
            .files
            .remove(path)
            .ok_or_else(|| DeleteError::FileDoesNotExist(path.to_path_buf()))?;
        Ok(())
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        Ok(self.inner.read().unwrap().files.contains_key(path))
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        {
            let inner = self.inner.read().unwrap();
            if inner.files.contains_key(path) {
                return Err(OpenWriteError::FileAlreadyExists(path.to_path_buf()));
            }
        }
        let writer = PgWriter {
            path: path.to_path_buf(),
            inner: Arc::clone(&self.inner),
            buf: Vec::new(),
        };
        Ok(BufWriter::new(Box::new(writer)))
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        let inner = self.inner.read().unwrap();
        inner
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| OpenReadError::FileDoesNotExist(path.to_path_buf()))
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        // Atômico por construção: a substituição do valor no HashMap é única e completa sob o write-lock —
        // reads nunca veem meia escrita. (No gate 3, a atomicidade vem do WAL/versionamento.)
        let mut inner = self.inner.write().unwrap();
        inner.files.insert(path.to_path_buf(), data.to_vec());
        // O Tantivy detecta mudança de meta.json via watch (`atomic_write` no meta é o sinal).
        drop(inner);
        self.inner.read().unwrap().watch_router.broadcast();
        Ok(())
    }

    fn sync_directory(&self) -> io::Result<()> {
        // Backend em memória — nada a sincronizar no gate 1 (no gate 3, isto é o flush de WAL/páginas).
        Ok(())
    }

    fn watch(&self, watch_callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        Ok(self.inner.read().unwrap().watch_router.subscribe(watch_callback))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{Schema, Value, STORED, TEXT};
    use tantivy::{doc, Index};

    /// Gate 1 do spike M139: o Tantivy indexa 3 docs num `PgDirectory` NOSSO e recupera o certo por busca de
    /// termo — SEM tocar o filesystem. Prova o contrato do trait + a integração Tantivy↔Directory-custom.
    #[test]
    fn test_pg_directory_indexes_and_searches() {
        // Arrange: schema {body: TEXT} e um índice sobre o NOSSO Directory (não Mmap, não fs).
        let mut sb = Schema::builder();
        let body = sb.add_text_field("body", TEXT | STORED);
        let schema = sb.build();

        let dir = PgDirectory::new();
        let index = Index::create(dir.clone(), schema.clone(), tantivy::IndexSettings::default())
            .expect("create index over PgDirectory");

        // Act: indexa 3 docs; um contém 'lazy'.
        let mut writer = index.writer(15_000_000).expect("writer");
        writer.add_document(doc!(body => "the quick brown fox")).unwrap();
        writer.add_document(doc!(body => "the lazy dog sleeps")).unwrap();
        writer.add_document(doc!(body => "bright vixens jump")).unwrap();
        writer.commit().expect("commit");

        // Assert 1: o storage é o PgDirectory (bytes > 0) — prova que não foi para o filesystem.
        assert!(dir.total_bytes() > 0, "o índice deve ter sido escrito no PgDirectory, não no fs");

        // Assert 2: busca por 'lazy' recupera o doc certo (top-1).
        let reader = index.reader().expect("reader");
        let searcher = reader.searcher();
        let qp = QueryParser::for_index(&index, vec![body]);
        let query = qp.parse_query("lazy").expect("parse");
        let hits = searcher
            .search(&query, &TopDocs::with_limit(3).order_by_score())
            .expect("search");
        assert_eq!(hits.len(), 1, "só um doc contém 'lazy'");

        let (_score, addr) = hits[0];
        let retrieved: tantivy::TantivyDocument = searcher.doc(addr).expect("doc");
        let got = retrieved
            .get_first(body)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(got.contains("lazy"), "o doc recuperado deve conter 'lazy', got: {got:?}");
    }

    // --- Casos NEGATIVOS e de BORDA do contrato do trait (testing.md § 4.1) ---
    // O gate 1 não é só o caminho feliz: um Directory que não falha-tipado nas bordas corrompe o Tantivy em
    // silêncio. Estes provam que o PgDirectory honra os erros tipados que o Tantivy espera.

    #[test]
    fn test_get_file_handle_absent_is_file_does_not_exist() {
        let dir = PgDirectory::new();
        // NEGATIVO: ler um arquivo que não existe → erro tipado FileDoesNotExist (não panic, não None mudo).
        let err = dir.get_file_handle(Path::new("ausente.term")).unwrap_err();
        assert!(matches!(err, OpenReadError::FileDoesNotExist(_)), "esperado FileDoesNotExist, got {err:?}");
    }

    #[test]
    fn test_delete_absent_is_file_does_not_exist() {
        let dir = PgDirectory::new();
        // NEGATIVO: deletar inexistente → DeleteError::FileDoesNotExist.
        let err = dir.delete(Path::new("ausente.idx")).unwrap_err();
        assert!(matches!(err, DeleteError::FileDoesNotExist(_)), "esperado FileDoesNotExist, got {err:?}");
    }

    #[test]
    fn test_atomic_write_read_roundtrip_and_exists() {
        let dir = PgDirectory::new();
        let p = Path::new("meta.json");
        // BORDA: exists é falso antes; atomic_write; read devolve os bytes exatos; exists vira verdadeiro.
        assert!(!dir.exists(p).unwrap(), "não deve existir antes da escrita");
        dir.atomic_write(p, b"{\"segments\":[]}").unwrap();
        assert!(dir.exists(p).unwrap(), "deve existir após atomic_write");
        assert_eq!(dir.atomic_read(p).unwrap(), b"{\"segments\":[]}", "read deve devolver os bytes exatos");
        // BORDA: atomic_write SUBSTITUI (não anexa) — reads nunca veem meia escrita.
        dir.atomic_write(p, b"{}").unwrap();
        assert_eq!(dir.atomic_read(p).unwrap(), b"{}", "atomic_write deve substituir, não anexar");
    }

    #[test]
    fn test_open_write_twice_is_file_already_exists() {
        let dir = PgDirectory::new();
        let p = Path::new("seg.store");
        // Primeiro open_write + terminate persiste o arquivo.
        {
            use tantivy::directory::TerminatingWrite;
            let mut w = dir.open_write(p).unwrap();
            w.write_all(b"payload").unwrap();
            w.terminate().unwrap();
        }
        // NEGATIVO: abrir o MESMO path de novo → FileAlreadyExists (o Tantivy conta com isso p/ não sobrescrever
        // segmentos). `matches!` em vez de `unwrap_err` porque o tipo OK (WritePtr = BufWriter<Box<dyn
        // TerminatingWrite>>) não é Debug, o que `unwrap_err` exigiria.
        assert!(
            matches!(dir.open_write(p), Err(OpenWriteError::FileAlreadyExists(_))),
            "esperado FileAlreadyExists ao reabrir um path já persistido"
        );
    }
}
