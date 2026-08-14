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
use tantivy::schema::{
    FAST, Field, INDEXED, IndexRecordOption, STORED, Schema, TextFieldIndexing, TextOptions, Value,
};
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

/// Schema de produção: `id` i64 stored+fast (retornável), `body` TEXT com o analisador NOMEADO do TheoDB.
///
/// B-044: o nome vai serializado no schema e portanto no `meta.json` do índice — é ele que decide qual
/// cadeia cada índice usa, para sempre. Índices construídos antes disto carregam `"default"` e continuam
/// resolvendo o tokenizer padrão do Tantivy, sem migração e sem mudança de comportamento.
fn build_schema() -> (Schema, Field, Field) {
    let mut sb = Schema::builder();
    let id = sb.add_i64_field("id", STORED | FAST | INDEXED);
    let body = sb.add_text_field(
        "body",
        TextOptions::default()
            .set_stored()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer(super::analyzer::ANALYZER_NAME)
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            ),
    );
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
fn read_generation(index_id: i64) -> Option<u64> {
    Spi::connect(|c| {
        // B-048(c) — a cadeia `.ok()`/`.unwrap_or(0)` SAIU daqui, e o motivo está em
        // `rules/error-handling.md § 2`: valor mágico para sinalizar falha é proibido. Antes, um erro do SPI ao
        // consultar o catálogo produzia o MESMO `0` de um índice legitimamente não construído, e o chamador não
        // tinha como distinguir. Agora o erro do SPI propaga (o `expect` vira `ereport(ERROR)` sob pgrx) e a
        // ausência de linha é `None` — dois estados, dois valores.
        let exists: bool = c
            .select("SELECT to_regclass('theodb.lexical_index_meta') IS NOT NULL", Some(1), &[])
            .expect("lexical: consulta ao catálogo falhou")
            .into_iter()
            .next()
            .and_then(|r| r.get::<bool>(1).expect("lexical: coluna to_regclass"))
            .unwrap_or(false);
        if !exists {
            // O catálogo inteiro não existe: nenhum build jamais aconteceu neste banco. É ausência, não erro.
            return None;
        }
        // Sem `coalesce`: o que interessa agora é a PRESENÇA da linha, e o `coalesce` a apagava ao devolver 0
        // tanto para "não tem linha" quanto para "tem linha com generation 0".
        c.select(
            "SELECT generation FROM theodb.lexical_index_meta WHERE index_id = $1",
            Some(1),
            &[index_id.into()],
        )
        .expect("lexical: consulta a lexical_index_meta falhou")
        .into_iter()
        .next()
        .and_then(|r| r.get::<i64>(1).expect("lexical: coluna generation"))
        .map(|g| g.max(0) as u64)
    })
}

/// Abre um `Index` do estado heap VISÍVEL ao snapshot (reusa `load`, que é MVCC — M139 gate 2).
fn open_from_heap(index_id: i64) -> Index {
    let store = Arc::new(load(index_id));
    let index = Index::open(PgDirectory::with_store(store)).unwrap_or_else(|e| {
        error!("bm25: índice heap ilegível/corrompido para index_id={index_id}: {e}")
    });
    // B-044: registrar também na LEITURA. O `QueryParser` resolve o tokenizer pelo nome gravado no schema
    // do campo e o procura aqui; sem isto, um índice novo daria `UnknownTokenizer` na consulta.
    super::analyzer::register(&index);
    index
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
    // B-044: registrar na ESCRITA. Indexar com uma cadeia e consultar com outra degrada recall sem erro.
    super::analyzer::register(&index);

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
                // B-048(a) — o `unwrap_or_default()` que estava aqui transformava `body` NULL num documento
                // VAZIO: ele entrava no índice, contava no retorno e não casava consulta nenhuma. Medido: 3
                // linhas com um NULL devolviam `3`, e só dois ids apareciam em qualquer busca — quem conferia o
                // retorno acreditava que os três estavam buscáveis.
                //
                // NULL não é erro: um corpus com `body` NULL é dado legítimo do usuário, e o `bm25_build` não
                // decide o esquema de ninguém. O que ele deve é RELATAR honestamente quantos indexou.
                let Some(body) = r.get::<String>(2).expect("body col") else {
                    continue;
                };
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

    // B-041 — o índice que nunca foi construído passa a RECUSAR, em vez de devolver zero linhas.
    //
    // Zero é resposta legítima num pilar de busca ("nada casou"), e é isso que torna o silêncio caro: a
    // aplicação que esqueceu o `bm25_build` — ou o perdeu num restore, ou o construiu com outro `index_id` —
    // concluía que o corpus não tinha o documento. O cliente do VectorDBBench precisou consultar
    // `lexical_index_meta` por conta própria para conseguir falhar alto, o que é a evidência de que a
    // informação estava no lugar certo e a função no lugar errado.
    //
    // O guard consulta PRESENÇA no catálogo, nunca contagem de documentos: medido, um `bm25_build` sobre corpus
    // vazio registra `generation 1`. Errar por "zero documentos" trocaria um falso-silêncio por um falso-alarme,
    // e `search_on_built_but_empty_index_returns_zero_rows_without_error` existe para provar que não trocamos.
    let Some(generation) = read_generation(index_id) else {
        error!(
            "bm25_search: index_id {index_id} nunca foi construído (sem linha em theodb.lexical_index_meta) — \
uma busca sobre índice inexistente devolveria zero linhas, indistinguível de 'nada casou'"
        );
    };

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
    // B-044 (D4): erro de parse NÃO vira lista vazia. Vazio é indistinguível de "nada casou", e um
    // `UnknownTokenizer` — a falha exata que um registro malfeito produz — passaria como resultado
    // legítimo, fazendo uma corrida de benchmark publicar NDCG 0 como medição. `sanitize_query` já
    // reduziu a consulta a alfanuméricos separados por espaço, então este caminho só dispara em defeito
    // de configuração, nunca em consulta de usuário.
    let parsed = match qp.parse_query(&clean) {
        Ok(q) => q,
        Err(e) => error!("bm25: consulta inválida no index_id={index_id} ({clean:?}): {e}"),
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

    // ---------------------------------------------------------------- B-044: stemming

    fn setup_inflection_docs() {
        Spi::run("DROP TABLE IF EXISTS b044_docs").unwrap();
        Spi::run("CREATE TABLE b044_docs (id bigint, body text)").unwrap();
        Spi::run(
            "INSERT INTO b044_docs VALUES \
             (1, 'the quick brown fox jumps over the lazy dog'), \
             (2, 'a database engine indexing documents')",
        )
        .unwrap();
    }

    /// O sintoma que abriu o item: a consulta flexionada não casava o documento.
    #[pg_test]
    fn test_bm25_matches_across_inflection() {
        setup_inflection_docs();
        crate::lexical::engine::bm25_build(710, "b044_docs", "id", "body");
        let hit: Option<i64> =
            Spi::get_one("SELECT id FROM bm25_search(710, 'jumping', 5)").unwrap();
        assert_eq!(hit, Some(1), "consulta flexionada 'jumping' deve casar o doc com 'jumps'");
    }

    /// Indexação e consulta usam a MESMA cadeia: `indexing` (documento) casa `index` (consulta).
    #[pg_test]
    fn test_bm25_stems_both_sides_of_the_pipeline() {
        setup_inflection_docs();
        crate::lexical::engine::bm25_build(711, "b044_docs", "id", "body");
        let hit: Option<i64> = Spi::get_one("SELECT id FROM bm25_search(711, 'index', 5)").unwrap();
        assert_eq!(hit, Some(2), "'index' deve casar o doc que contém 'indexing'");
    }

    /// Stopwords somem do índice e da consulta — zero linhas é resultado legítimo, não erro.
    #[pg_test]
    fn test_stopword_only_query_returns_no_rows_without_error() {
        setup_inflection_docs();
        crate::lexical::engine::bm25_build(712, "b044_docs", "id", "body");
        let n: Option<i64> =
            Spi::get_one("SELECT count(*) FROM bm25_search(712, 'the', 5)").unwrap();
        assert_eq!(n, Some(0), "consulta só de stopword devolve vazio, sem erro");
    }

    /// Consulta de usuário com pontuação e operadores NUNCA chega ao caminho de erro do parser:
    /// `sanitize_query` a reduz a alfanuméricos antes. Prova a mitigação do risco R6 do plano.
    #[pg_test]
    fn test_sanitized_user_queries_never_reach_the_error_path() {
        setup_inflection_docs();
        crate::lexical::engine::bm25_build(713, "b044_docs", "id", "body");
        for q in ["LAZY, Dog!", "a+b", "x AND -y", "acentuação", "(((", "\"aspas\""] {
            let sql = format!("SELECT count(*) FROM bm25_search(713, '{q}', 5)");
            let n: Option<i64> = Spi::get_one(&sql).unwrap_or_else(|e| {
                panic!("consulta de usuário {q:?} levantou erro: {e}");
            });
            assert!(n.is_some(), "consulta {q:?} deve devolver contagem, não erro");
        }
    }

    /// A prova mais importante da D1: um índice com o schema ANTIGO (tokenizer `default`) continua
    /// respondendo sob a semântica antiga quando lido pelo binário novo — sem migração e sem surpresa.
    #[pg_test]
    fn test_legacy_default_schema_index_keeps_its_own_semantics() {
        use std::sync::Arc;
        use tantivy::schema::{STORED, Schema, TEXT};
        use tantivy::{Index, IndexSettings};

        setup_inflection_docs();
        crate::lexical::pg_backing::ensure_table();
        Spi::run(
            "CREATE TABLE IF NOT EXISTS theodb.lexical_index_meta (\
             index_id bigint PRIMARY KEY, generation bigint NOT NULL DEFAULT 0)",
        )
        .unwrap();

        // schema legado: `TEXT` puro => tokenizer "default", como antes do B-044
        let mut sb = Schema::builder();
        let id_f = sb.add_i64_field("id", tantivy::schema::FAST | tantivy::schema::INDEXED | STORED);
        let body_f = sb.add_text_field("body", TEXT | STORED);
        let store = Arc::new(crate::lexical::MemStore::default());
        let index = Index::create(
            crate::lexical::PgDirectory::with_store(store.clone()),
            sb.build(),
            IndexSettings::default(),
        )
        .unwrap();
        {
            let mut w = index.writer_with_num_threads(1, 15_000_000).unwrap();
            w.add_document(
                tantivy::doc!(id_f => 1i64, body_f => "the quick brown fox jumps over the lazy dog"),
            )
            .unwrap();
            w.commit().unwrap();
        }
        crate::lexical::pg_backing::flush(714, &store);
        Spi::run("INSERT INTO theodb.lexical_index_meta(index_id, generation) VALUES (714, 1)")
            .unwrap();

        // sob "default" não há stemming: a flexionada NÃO casa — e isso é o comportamento CORRETO
        let inflected: Option<i64> =
            Spi::get_one("SELECT count(*) FROM bm25_search(714, 'jumping', 5)").unwrap();
        assert_eq!(inflected, Some(0), "índice legado não deve stemizar");

        // e o que casava antes continua casando
        let exact: Option<i64> =
            Spi::get_one("SELECT id FROM bm25_search(714, 'jumps', 5)").unwrap();
        assert_eq!(exact, Some(1), "índice legado deve continuar casando o termo exato");
    }

    /// O caminho de atualização é reconstruir — e ele funciona sobre o mesmo `index_id`.
    #[pg_test]
    fn test_rebuilding_a_legacy_index_upgrades_it_to_the_named_analyzer() {
        setup_inflection_docs();
        crate::lexical::engine::bm25_build(715, "b044_docs", "id", "body");
        let before: Option<i64> =
            Spi::get_one("SELECT count(*) FROM bm25_search(715, 'jumping', 5)").unwrap();
        assert_eq!(before, Some(1), "após rebuild com a cadeia nova, a flexionada casa");
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

    // B-041 — `test_bm25_search_empty_index_returns_no_rows` foi REMOVIDO daqui, e a razão é que ele
    // codificava o defeito como contrato: `assert_eq!(n, Some(0), "index with no build -> 0 rows, not an
    // error")` sobre o `index_id` 999, que nunca passou por `bm25_build`.
    //
    // Dois erros somados, e o segundo escondia o primeiro: o NOME dizia "empty index" e a MONTAGEM usava um
    // índice nunca construído. São estados diferentes — um é resposta legítima, o outro é a aplicação não saber
    // que esqueceu o build —, e o nome fazia o primeiro cobrir o segundo.
    //
    // Nenhuma cobertura se perdeu; os dois estados ganharam teste próprio, cada um afirmando o que o seu nome
    // diz: `search_on_never_built_index_raises_typed_error` e
    // `search_on_built_but_empty_index_returns_zero_rows_without_error`.


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

    // ---- B-041 / B-048: a superfície para de responder onde deveria recusar ----
    //
    // A classe que estes três testes fecham já foi consertada TRÊS vezes neste projeto (explain_scan/scan_stats
    // com zeros silenciosos, o contador do chunk-skip do colunar, o gerador de script de upgrade) e reapareceu
    // outras três. O que a torna cara não é cada instância — é que zero, num pilar de busca, é uma resposta
    // legítima, então o silêncio é indistinguível do resultado.

    /// B-041 — buscar num `index_id` que nunca passou por `bm25_build` levanta erro tipado, em vez de devolver
    /// zero linhas. Medido antes do conserto contra `theodb:b036`: `bm25_search(999,'lazy dog',5)` devolvia
    /// **0 linhas, sem erro nem aviso**.
    #[pg_test(error = "bm25_search: index_id 999 nunca foi construído (sem linha em theodb.lexical_index_meta) — \
uma busca sobre índice inexistente devolveria zero linhas, indistinguível de \'nada casou\'")]
    fn search_on_never_built_index_raises_typed_error() {
        let _ = pgrx::Spi::get_one::<i64>("SELECT count(*) FROM bm25_search(999, 'lazy dog', 5)");
    }

    /// B-041, a outra metade — e é ELA que impede o conserto de virar regressão.
    ///
    /// Um índice construído sobre corpus vazio é estado VÁLIDO: devolve zero linhas e **não** deve erguer. Foi
    /// medido que esse build registra `generation 1` no catálogo, e é por isso que o guard consulta PRESENÇA e
    /// nunca contagem de documentos — a implementação ingênua (errar quando não há documento) reprovaria aqui.
    #[pg_test]
    fn search_on_built_but_empty_index_returns_zero_rows_without_error() {
        pgrx::Spi::run("CREATE TABLE b041_vazio(id bigint PRIMARY KEY, body text)").unwrap();
        let built = pgrx::Spi::get_one::<i64>(
            "SELECT bm25_build(4242::bigint, 'b041_vazio', 'id', 'body')",
        )
        .unwrap();
        assert_eq!(built, Some(0), "corpus vazio indexa zero documentos");
        let registrado = pgrx::Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb.lexical_index_meta WHERE index_id = 4242",
        )
        .unwrap();
        assert_eq!(registrado, Some(1), "o build vazio TEM de registrar — é o que distingue dos nunca-construídos");
        let n = pgrx::Spi::get_one::<i64>("SELECT count(*) FROM bm25_search(4242, 'qualquer', 5)").unwrap();
        assert_eq!(n, Some(0), "corpus vazio é resultado legítimo, não erro");
    }

    /// B-048(a) — `bm25_build` conta o que é ACHÁVEL. Medido antes do conserto: 3 linhas com um `body` NULL
    /// devolviam **3**, e só os ids 1 e 3 apareciam em qualquer busca — o `unwrap_or_default()` fazia do NULL
    /// um documento vazio que contava e nunca casava.
    ///
    /// A segunda asserção é a que impede contar certo por outro caminho errado: o retorno tem de BATER com o
    /// que a busca de fato encontra.
    #[pg_test]
    fn build_counts_only_findable_documents() {
        pgrx::Spi::run("CREATE TABLE b048_d(id bigint PRIMARY KEY, body text)").unwrap();
        pgrx::Spi::run("INSERT INTO b048_d VALUES (1,'alpha'),(2,NULL),(3,'beta')").unwrap();
        let n = pgrx::Spi::get_one::<i64>(
            "SELECT bm25_build(4243::bigint, 'b048_d', 'id', 'body')",
        )
        .unwrap();
        assert_eq!(n, Some(2), "o NULL não é achável e não deve ser contado");
        let achaveis =
            pgrx::Spi::get_one::<i64>("SELECT count(*) FROM bm25_search(4243, 'alpha beta', 10)").unwrap();
        assert_eq!(achaveis, Some(2), "o retorno do build tem de bater com o que a busca encontra");
    }

    /// B-048(a), o caso extremo: TODOS os `body` NULL. Zero documentos acháveis continua sendo build válido —
    /// registra no catálogo, e a busca subsequente devolve zero SEM erro.
    #[pg_test]
    fn build_over_all_null_bodies_is_a_valid_empty_build() {
        pgrx::Spi::run("CREATE TABLE b048_nulos(id bigint PRIMARY KEY, body text)").unwrap();
        pgrx::Spi::run("INSERT INTO b048_nulos VALUES (1,NULL),(2,NULL)").unwrap();
        let n = pgrx::Spi::get_one::<i64>(
            "SELECT bm25_build(4244::bigint, 'b048_nulos', 'id', 'body')",
        )
        .unwrap();
        assert_eq!(n, Some(0), "nenhum documento achável");
        let registrado = pgrx::Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb.lexical_index_meta WHERE index_id = 4244",
        )
        .unwrap();
        assert_eq!(registrado, Some(1), "build válido registra, mesmo sem documento achável");
        let n2 = pgrx::Spi::get_one::<i64>("SELECT count(*) FROM bm25_search(4244, 'x', 5)").unwrap();
        assert_eq!(n2, Some(0), "zero linhas SEM erro — o índice existe, o corpus é que não tem termo");
    }
}
