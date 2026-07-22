//! M139 gate 2/3 — persistência PG do `SegmentStore`, via **buffer-then-flush** (ADR 0051).
//!
//! Achado empírico (probe): o Tantivy chama o `Directory` de MÚLTIPLAS threads (rayon/merge) — logo o buffer
//! (`MemStore`) NÃO pode tocar o PG. Aqui, na **main thread** (após `writer.commit()` retornar), fazemos flush do
//! buffer para a tabela heap `theodb.lexical_files(index_id, path, data bytea)`. O heap do PG dá **MVCC + WAL +
//! TOAST de graça** (Rule 9 — não reinventar): visibilidade por snapshot (gate 2) e crash-safety por replay do
//! WAL do heap (gate 3), sem código de página/WAL custom. `load` lê do heap para um buffer novo (respeita o
//! snapshot da txn → MVCC automático).

use pgrx::prelude::*;
use std::path::Path;

use crate::lexical::pg_directory::{MemStore, SegmentStore};

/// Cria o catálogo heap se ausente. `bytea` TOASTa segmentos grandes automaticamente.
fn ensure_table() {
    Spi::run("CREATE SCHEMA IF NOT EXISTS theodb").expect("lexical: create schema");
    Spi::run(
        "CREATE TABLE IF NOT EXISTS theodb.lexical_files (\
         index_id bigint NOT NULL, path text NOT NULL, data bytea NOT NULL, \
         PRIMARY KEY (index_id, path))",
    )
    .expect("lexical: create lexical_files");
}

/// Flush do buffer para o heap, na txn corrente (main thread, pós-commit do Tantivy). MVCC/WAL do PG.
pub fn flush(index_id: i64, store: &MemStore) {
    ensure_table();
    for (path, data) in store.files() {
        let p = path.to_string_lossy().to_string();
        Spi::run_with_args(
            "INSERT INTO theodb.lexical_files(index_id, path, data) VALUES ($1, $2, $3) \
             ON CONFLICT (index_id, path) DO UPDATE SET data = EXCLUDED.data",
            &[index_id.into(), p.into(), data.into()],
        )
        .expect("lexical: flush insert");
    }
}

/// Load do heap para um `MemStore` novo (main thread). Respeita o snapshot da txn — um leitor com snapshot
/// anterior NÃO vê linhas de txn não-commitada (é a visibilidade MVCC do gate 2, herdada do heap do PG).
pub fn load(index_id: i64) -> MemStore {
    let store = MemStore::default();
    Spi::connect(|c| {
        let rows = c
            .select(
                "SELECT path, data FROM theodb.lexical_files WHERE index_id = $1 ORDER BY path",
                None,
                &[index_id.into()],
            )
            .expect("lexical: load select");
        for r in rows {
            let p: String = r.get::<String>(1).expect("path col").expect("path not null");
            let d: Vec<u8> = r.get::<Vec<u8>>(2).expect("data col").expect("data not null");
            store.write(Path::new(&p), d);
        }
    });
    store
}

/// Demo do gate 2/3 (passo B): prova o round-trip completo **através do storage PG**. Indexa 3 docs num
/// `PgDirectory` bufferizado, faz flush para o heap, LÊ de volta um buffer novo do heap, abre o índice e busca
/// `term`. Retorna o nº de hits. Se retorna 1 para `'lazy'`, os bytes sobreviveram ao round-trip pelo PG.
///
/// O `flush`/`load` rodam na main thread (esta função); as threads do Tantivy só tocam o buffer em memória —
/// nenhum SPI de thread worker (a restrição que o probe descobriu).
#[pg_extern]
fn lexical_spike_roundtrip(term: &str) -> i64 {
    use std::sync::Arc;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{Schema, STORED, TEXT};
    use tantivy::{doc, Index};

    use crate::lexical::pg_directory::PgDirectory;

    let index_id: i64 = 424_242; // id fixo do spike

    // 1. Indexa num PgDirectory bufferizado (buffer = MemStore, thread-safe), depois flush ao PG (main thread).
    let mut sb = Schema::builder();
    let body = sb.add_text_field("body", TEXT | STORED);
    let schema = sb.build();
    let store = Arc::new(MemStore::default());
    let index = Index::create(
        PgDirectory::with_store(store.clone()),
        schema,
        tantivy::IndexSettings::default(),
    )
    .expect("create index");
    {
        let mut w = index.writer_with_num_threads(1, 15_000_000).expect("writer");
        w.add_document(doc!(body => "the quick brown fox")).unwrap();
        w.add_document(doc!(body => "the lazy dog sleeps")).unwrap();
        w.add_document(doc!(body => "bright vixens jump")).unwrap();
        w.commit().expect("commit");
    } // writer dropado — de volta na main thread
    flush(index_id, &store);

    // 2. LÊ um buffer NOVO do heap PG, abre o índice e busca — prova que os bytes vieram do storage PG.
    let store2 = Arc::new(load(index_id));
    let index2 = Index::open(PgDirectory::with_store(store2)).expect("open from PG");
    let body2 = index2.schema().get_field("body").expect("field body");
    let reader = index2.reader().expect("reader");
    let searcher = reader.searcher();
    let qp = QueryParser::for_index(&index2, vec![body2]);
    let query = qp.parse_query(term).expect("parse");
    let hits = searcher
        .search(&query, &TopDocs::with_limit(10).order_by_score())
        .expect("search");
    hits.len() as i64
}

/// Gate 2 (MVCC), passo 1: indexa 3 docs (um contendo `term`) e faz FLUSH ao heap na txn corrente — SEM commit
/// (o caller controla a txn). Retorna o nº de arquivos persistidos. Um segundo backend com snapshot anterior
/// não verá estes arquivos até o COMMIT (a visibilidade é a do heap do PG).
#[pg_extern]
fn lexical_spike_flush_only(index_id: i64, term: &str) -> i64 {
    use std::sync::Arc;
    use tantivy::schema::{Schema, STORED, TEXT};
    use tantivy::{doc, Index};

    use crate::lexical::pg_directory::PgDirectory;

    let mut sb = Schema::builder();
    let body = sb.add_text_field("body", TEXT | STORED);
    let schema = sb.build();
    let store = Arc::new(MemStore::default());
    let index = Index::create(
        PgDirectory::with_store(store.clone()),
        schema,
        tantivy::IndexSettings::default(),
    )
    .expect("create index");
    {
        let mut w = index.writer_with_num_threads(1, 15_000_000).expect("writer");
        w.add_document(doc!(body => format!("the {term} dog sleeps"))).unwrap();
        w.add_document(doc!(body => "the quick brown fox")).unwrap();
        w.add_document(doc!(body => "bright vixens jump")).unwrap();
        w.commit().expect("commit");
    }
    flush(index_id, &store);
    store.files().len() as i64
}

/// Gate 4 (custo): indexa `n` docs sintéticos determinísticos num `PgDirectory` bufferizado e faz flush ao
/// heap. Retorna o total de bytes do índice (o "tamanho" para o head-to-head vs `pg_textsearch`). Corpus
/// determinístico (mesmo texto por `i`) para o `pg_textsearch` indexar o MESMO corpus e a comparação ser justa.
#[pg_extern]
fn lexical_spike_bulk_index(index_id: i64, n: i32) -> i64 {
    use std::sync::Arc;
    use tantivy::schema::{Schema, STORED, TEXT};
    use tantivy::{doc, Index};

    use crate::lexical::pg_directory::PgDirectory;

    let mut sb = Schema::builder();
    let body = sb.add_text_field("body", TEXT | STORED);
    let schema = sb.build();
    let store = Arc::new(MemStore::default());
    let index = Index::create(
        PgDirectory::with_store(store.clone()),
        schema,
        tantivy::IndexSettings::default(),
    )
    .expect("create index");
    {
        let mut w = index.writer_with_num_threads(1, 50_000_000).expect("writer");
        for i in 0..n {
            // Corpus determinístico — o mesmo texto que o smoke SQL insere no pg_textsearch (gate 4 justo).
            let txt = format!("document number {i} about lazy quick fox jumping over sleeping dogs and vixens {}", i % 97);
            w.add_document(doc!(body => txt)).unwrap();
        }
        w.commit().expect("commit");
    }
    flush(index_id, &store);
    store.total_bytes() as i64
}

/// Gate 2 (MVCC), passo 2: LÊ o índice do heap no snapshot da txn corrente e busca `term`. Retorna o nº de hits
/// (0 se o `index_id` não tem arquivos VISÍVEIS ao snapshot — ex.: outra txn ainda não commitou o flush).
#[pg_extern]
fn lexical_spike_search(index_id: i64, term: &str) -> i64 {
    use std::sync::Arc;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::{Index, TantivyDocument};

    use crate::lexical::pg_directory::PgDirectory;

    let store = Arc::new(load(index_id));
    if store.total_bytes() == 0 {
        return 0; // nada visível neste snapshot (MVCC) — índice não existe para este leitor
    }
    let index = match Index::open(PgDirectory::with_store(store)) {
        Ok(i) => i,
        Err(_) => return 0,
    };
    let body = index.schema().get_field("body").expect("field body");
    let reader = index.reader().expect("reader");
    let searcher = reader.searcher();
    let qp = QueryParser::for_index(&index, vec![body]);
    let query = qp.parse_query(term).expect("parse");
    let hits = searcher
        .search(&query, &TopDocs::with_limit(10).order_by_score())
        .expect("search");
    // Toca o doc store p/ garantir que os segmentos vieram MESMO do PG (não um índice vazio que "abriu").
    if let Some((_s, addr)) = hits.first() {
        let _d: TantivyDocument = searcher.doc(*addr).expect("doc");
    }
    hits.len() as i64
}
