//! M140.3 — a superfície BM25 own-code de PRODUÇÃO (funções sobre heap, ADR-0052), com o cache do
//! Directory por-geração (ADR D1) que mata o reload-por-query do spike M139.
//!
//! - `bm25_build(index_id, table, id_col, text_col)` — indexa uma tabela real (id+body) no Tantivy,
//!   faz flush ao heap `theodb.lexical_files` (reusa `pg_backing::flush`) e bumpa a geração.
//! - `bm25_search(index_id, query, k)` — lê a geração SOB O SNAPSHOT, usa o `IndexCache` (rebuild só
//!   se a geração mudou — MVCC-correto) e retorna `(id, score)`.
//!
//! Disciplina de threads (#153): tudo aqui roda na main thread do backend; o writer do Tantivy usa 1
//! thread e as threads internas do Tantivy NÃO tocam SPI/pg_sys nem o cache. O `Mutex` do cache é o
//! contrato de segurança (um backend PG é um processo single-thread na main).
use std::sync::{LazyLock, Mutex};

use pgrx::prelude::*;
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{FAST, Field, INDEXED, STORED, Schema, TEXT, Value};
use tantivy::{Index, TantivyDocument};

use theodb_lexical::{IndexCache, MemStore, PgDirectory};

use super::pg_backing::{flush, load};

/// Cache por-backend (processo PG). `LazyLock` estável (rustc ≥ 1.80). Acessado só na main thread; o
/// `Mutex` serializa por contrato (o backend é single-thread). As threads internas do Tantivy não o tocam.
static CACHE: LazyLock<Mutex<IndexCache>> = LazyLock::new(|| Mutex::new(IndexCache::new()));

/// Quoting de identificador à la `quote_ident` do PG (aspas duplas, escapa aspas) — anti-injeção
/// para os nomes dinâmicos de tabela/coluna do `bm25_build`.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Schema de produção: `id` i64 stored+fast (retornável), `body` TEXT (tokenizer default, como o M140.1).
fn build_schema() -> (Schema, Field, Field) {
    let mut sb = Schema::builder();
    let id = sb.add_i64_field("id", STORED | FAST | INDEXED);
    let body = sb.add_text_field("body", TEXT | STORED);
    let schema = sb.build();
    (schema, id, body)
}

fn ensure_meta() {
    Spi::run(
        "CREATE TABLE IF NOT EXISTS theodb.lexical_index_meta (\
         index_id bigint PRIMARY KEY, generation bigint NOT NULL DEFAULT 0)",
    )
    .expect("lexical: create lexical_index_meta");
}

/// Bumpa a geração do índice (INSERT..ON CONFLICT +1). O token de invalidação do cache (ADR D2).
fn bump_generation(index_id: i64) {
    Spi::run_with_args(
        "INSERT INTO theodb.lexical_index_meta(index_id, generation) VALUES ($1, 1) \
         ON CONFLICT (index_id) DO UPDATE SET generation = theodb.lexical_index_meta.generation + 1",
        &[index_id.into()],
    )
    .expect("lexical: bump generation");
}

/// Lê a geração corrente SOB O SNAPSHOT da txn (a chave MVCC do cache, ADR D1). Retorna 0 quando o
/// catálogo ainda não existe (nenhum build) OU o índice não tem linha (índice vazio) — ambos são
/// estados válidos, não erro. O `coalesce` garante exatamente uma linha (evita o `InvalidPosition` de
/// `get_one` sobre result-set vazio); o guard `to_regclass` cobre a tabela ainda inexistente.
///
/// M140.4 (D3, fecha o M140.3 review LOW de straddle E o HIGH do M140.4 review): usa SPI **read-only**
/// (`Spi::connect` + `c.select`), NUNCA `Spi::get_one` — que em pgrx 0.19 é `connect_mut`/`update` →
/// `mark_mutable` → `read_only=false` → abre um snapshot FRESCO por statement (reabrindo o straddle) E marca
/// a txn mutável (quebra em read replica: "cannot assign TransactionId during recovery" + queima um XID por
/// busca). Com `c.select` (read-only), esta leitura E o `load` (também `c.select`, `pg_backing.rs`) reusam o
/// ActiveSnapshot da statement → veem o MESMO snapshot: tag do cache == conteúdo, sob RC e RR; e `bm25_search`
/// roda em read replica sem burn de XID. Retorna 0 quando o catálogo não existe (nenhum build) ou o índice
/// não tem linha (índice vazio) — ambos estados válidos.
fn read_generation(index_id: i64) -> u64 {
    Spi::connect(|c| {
        let exists = c
            .select("SELECT to_regclass('theodb.lexical_index_meta') IS NOT NULL", Some(1), &[])
            .ok()
            .and_then(|t| t.into_iter().next())
            .and_then(|r| r.get::<bool>(1).ok().flatten())
            .unwrap_or(false);
        if !exists {
            return 0u64;
        }
        c.select(
            "SELECT coalesce((SELECT generation FROM theodb.lexical_index_meta WHERE index_id = $1), 0)",
            Some(1),
            &[index_id.into()],
        )
        .ok()
        .and_then(|t| t.into_iter().next())
        .and_then(|r| r.get::<i64>(1).ok().flatten())
        .unwrap_or(0)
        .max(0) as u64
    })
}

/// Abre um `Index` do estado heap VISÍVEL ao snapshot (reusa `load`, que é MVCC — M139 gate 2).
fn open_from_heap(index_id: i64) -> Index {
    let store = Arc::new(load(index_id));
    Index::open(PgDirectory::with_store(store)).unwrap_or_else(|e| {
        error!("bm25: índice heap ilegível/corrompido para index_id={index_id}: {e}")
    })
}

/// Indexa `SELECT id_col, text_col FROM table` no Tantivy, flush ao heap (drop+reinsere atômico),
/// bumpa a geração. Retorna o nº de documentos indexados.
#[pg_extern]
fn bm25_build(index_id: i64, table: &str, id_col: &str, text_col: &str) -> i64 {
    super::pg_backing::ensure_table();
    ensure_meta();

    let (schema, id_f, body_f) = build_schema();
    let store = Arc::new(MemStore::default());
    let index = Index::create(
        PgDirectory::with_store(store.clone()),
        schema,
        tantivy::IndexSettings::default(),
    )
    .expect("lexical: create index");

    let mut count: i64 = 0;
    {
        let mut w = index.writer_with_num_threads(1, 50_000_000).expect("lexical: writer");
        let sql = format!(
            "SELECT ({})::bigint AS id, ({})::text AS body FROM {}",
            quote_ident(id_col),
            quote_ident(text_col),
            quote_ident(table),
        );
        Spi::connect(|c| {
            let rows = c.select(&sql, None, &[]).expect("lexical: build select");
            for r in rows {
                let id: i64 = r.get::<i64>(1).expect("id col").expect("id not null");
                let body: String = r.get::<String>(2).expect("body col").unwrap_or_default();
                w.add_document(tantivy::doc!(id_f => id, body_f => body))
                    .expect("lexical: add_document");
                count += 1;
            }
        });
        w.commit().expect("lexical: commit");
    }

    // drop+reinsere atômico (Q3): o build substitui o índice antigo na MESMA txn.
    Spi::run_with_args("DELETE FROM theodb.lexical_files WHERE index_id = $1", &[index_id.into()])
        .expect("lexical: clear old index");
    flush(index_id, &store);
    bump_generation(index_id);
    count
}

/// Busca BM25 sobre o índice `index_id`. Usa o cache (rebuild só se a geração visível mudou —
/// MVCC-correto). Retorna `(id, score)` ordenado por score desc, top-`k`.
#[pg_extern]
fn bm25_search(
    index_id: i64,
    query: &str,
    k: i32,
) -> TableIterator<'static, (name!(id, i64), name!(score, f64))> {
    if k <= 0 {
        error!("bm25_search: k must be > 0 (got {k})");
    }
    let clean = sanitize_query(query);
    if clean.is_empty() {
        return TableIterator::new(Vec::new().into_iter());
    }

    let generation = read_generation(index_id);
    if generation == 0 {
        // índice sem build — estado válido, 0 linhas.
        return TableIterator::new(Vec::new().into_iter());
    }

    // Recupera o poison (review M140.3 HIGH): o closure de build roda com o guard tomado; um panic
    // nele (ex.: `open_from_heap` sobre bytes de heap inconsistentes) envenenaria o `Mutex` static
    // por-backend, quebrando TODA `bm25_search` futura da sessão. O dado é um `HashMap` sem invariante
    // quebrado num panic pré-insert (`get_or_build` avalia `build()` ANTES do `insert`), então ignorar
    // o poison restaura a disponibilidade com segurança.
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let index = cache.get_or_build(index_id, generation, || open_from_heap(index_id));

    let id_f = index.schema().get_field("id").expect("field id");
    let body_f = index.schema().get_field("body").expect("field body");
    let reader = index.reader().expect("lexical: reader");
    let searcher = reader.searcher();
    let qp = QueryParser::for_index(index, vec![body_f]);
    let parsed = match qp.parse_query(&clean) {
        Ok(q) => q,
        Err(_) => return TableIterator::new(Vec::new().into_iter()),
    };
    let hits = searcher
        .search(&parsed, &TopDocs::with_limit(k as usize).order_by_score())
        .unwrap_or_else(|e| error!("bm25: busca falhou no index_id={index_id}: {e}"));

    let mut out: Vec<(i64, f64)> = Vec::with_capacity(hits.len());
    for (score, addr) in hits {
        let doc: TantivyDocument = searcher
            .doc(addr)
            .unwrap_or_else(|e| error!("bm25: doc ilegível no index_id={index_id}: {e}"));
        if let Some(id) = doc.get_first(id_f).and_then(|v| v.as_i64()) {
            out.push((id, score as f64));
        }
    }
    TableIterator::new(out.into_iter())
}

/// Texto livre → bag de tokens (o parser do Tantivy trata +,-,(),",: como operadores — igual ao
/// M140.1 `lexical_engines.TantivyBM25._sanitize`).
fn sanitize_query(query: &str) -> String {
    query
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// B-011/B-012: sem `#[pgrx::pg_schema]` o pgrx NÃO registra os `#[pg_test]` como funções SQL, e o harness
// falha com `function tests.<nome>() does not exist` — foi o que manteve os 6 testes de BM25 vermelhos.
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    fn setup_docs() {
        Spi::run("DROP TABLE IF EXISTS m140_3_docs").unwrap();
        Spi::run("CREATE TABLE m140_3_docs (id bigint, body text)").unwrap();
        Spi::run(
            "INSERT INTO m140_3_docs VALUES \
             (1, 'the quick brown fox'), \
             (2, 'error timeout blk_zebra9 connection reset'), \
             (3, 'info dfs datanode packetresponder terminating')",
        )
        .unwrap();
    }

    #[pg_test]
    fn test_bm25_build_indexes_and_bumps_generation() {
        setup_docs();
        let n = crate::lexical::engine::bm25_build(100, "m140_3_docs", "id", "body");
        assert_eq!(n, 3, "build indexes 3 rows");
        let generation: Option<i64> =
            Spi::get_one("SELECT generation FROM theodb.lexical_index_meta WHERE index_id = 100")
                .unwrap();
        assert_eq!(generation, Some(1), "first build -> generation 1");
    }

    #[pg_test]
    fn test_bm25_build_rebuild_bumps_to_2() {
        setup_docs();
        crate::lexical::engine::bm25_build(101, "m140_3_docs", "id", "body");
        crate::lexical::engine::bm25_build(101, "m140_3_docs", "id", "body");
        let generation: Option<i64> =
            Spi::get_one("SELECT generation FROM theodb.lexical_index_meta WHERE index_id = 101")
                .unwrap();
        assert_eq!(generation, Some(2), "rebuild -> generation 2");
    }

    #[pg_test]
    fn test_bm25_search_returns_matching_id() {
        setup_docs();
        crate::lexical::engine::bm25_build(102, "m140_3_docs", "id", "body");
        let top: Option<i64> = Spi::get_one(
            "SELECT id FROM bm25_search(102, 'blk_zebra9', 10) ORDER BY score DESC LIMIT 1",
        )
        .unwrap();
        assert_eq!(top, Some(2), "the doc with the rare term ranks first");
    }

    #[pg_test]
    fn test_bm25_search_empty_index_returns_no_rows() {
        let n: Option<i64> =
            Spi::get_one("SELECT count(*) FROM bm25_search(999, 'anything', 10)").unwrap();
        assert_eq!(n, Some(0), "index with no build -> 0 rows, not an error");
    }

    #[pg_test]
    fn test_bm25_search_empty_query_returns_no_rows() {
        setup_docs();
        crate::lexical::engine::bm25_build(103, "m140_3_docs", "id", "body");
        let n: Option<i64> =
            Spi::get_one("SELECT count(*) FROM bm25_search(103, '   ', 10)").unwrap();
        assert_eq!(n, Some(0));
    }

    #[pg_test]
    fn test_bm25_search_sees_new_generation_after_rebuild() {
        Spi::run("DROP TABLE IF EXISTS m140_3_docs2").unwrap();
        Spi::run("CREATE TABLE m140_3_docs2 (id bigint, body text)").unwrap();
        Spi::run("INSERT INTO m140_3_docs2 VALUES (1, 'alpha')").unwrap();
        crate::lexical::engine::bm25_build(104, "m140_3_docs2", "id", "body");
        // adiciona um doc com um termo novo e reconstrói
        Spi::run("INSERT INTO m140_3_docs2 VALUES (2, 'betamax')").unwrap();
        crate::lexical::engine::bm25_build(104, "m140_3_docs2", "id", "body");
        let hit: Option<i64> = Spi::get_one(
            "SELECT id FROM bm25_search(104, 'betamax', 10) ORDER BY score DESC LIMIT 1",
        )
        .unwrap();
        assert_eq!(hit, Some(2), "search after rebuild sees the new generation's docs");
    }
}
