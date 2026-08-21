//! Domain layer (blueprint ADR-1): the embedding call — portable business logic. Talks to the
//! configurable model endpoint over HTTP (minreq, native-tls) and returns a pgvector text literal.
//! All Postgres specifics (typed errors, GUC reads) are delegated to `crate::pg` — this module touches
//! the pgrx ABI only through those helpers, which concentrates the PG coupling at one boundary (ADR-1).
//!
//! Two entry points share one HTTP/parse path (`post_json` + `resolve_cfg` + `format_embedding`, DRY):
//!
//! * `run` — one input → one vector (per-row `theodb.embed`).
//! * `run_batch` — N inputs → N vectors in ONE round-trip (`theodb.embed_batch`); collapses the embed
//!   N+1 the system-design audit flagged CRITICAL. The endpoint natively accepts `input: string[]`
//!   (OpenAI shape); embeddings are mapped back by `data[].index`.
//!
//! Error parity with the plpython3u baseline (oracle: `benchmarks/tests/test_embed_sql.py`):
//!   * input errors (NULL content, unset endpoint, non-http(s) scheme)        -> SQLSTATE 22023
//!   * HTTP / response failures (connect/timeout/non-2xx/bad body/shape/empty) -> SQLSTATE 38000
use serde_json::Value;

use crate::http::{post_json, truncate};
use crate::pg::{err_external, err_input, guc};

/// Generate the embedding for `content` via the `theodb.embedding_endpoint` GUC and return it as a
/// pgvector text literal `"[x,y,z]"` (exactly as the plpython3u baseline did); the SQL wrapper
/// `theodb.embed` casts it to `vector`. This is the body invoked by `theodb_rs._embed_text`.
pub(crate) fn run(content: Option<&str>, model: Option<&str>) -> String {
    let content = match content {
        Some(c) => c,
        None => err_input("theodb.embed: content must not be NULL"),
    };

    let (endpoint, mdl, api_key) = resolve_cfg("theodb.embed", model);
    let payload = serde_json::json!({ "input": content, "model": mdl }).to_string();
    let body = post_json("theodb.embed", &endpoint, payload, api_key.as_deref());

    // Valid JSON but the expected data[0].embedding array is missing -> "unexpected ... shape".
    let emb = match body
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|e| e.get("embedding"))
        .and_then(|e| e.as_array())
    {
        Some(a) => a,
        None => err_external(&format!(
            "theodb.embed: unexpected embedding response shape: {}",
            truncate(&body.to_string(), 200)
        )),
    };

    format_embedding(emb, "theodb.embed")
}

/// Generate embeddings for a whole array in ONE HTTP round-trip (the N→1 fix for the audit's
/// CRITICAL embed N+1). Returns N pgvector text literals aligned to the input order; the SQL wrapper
/// `theodb.embed_batch` casts each `::vector`. N-in/N-out is enforced (count + index): a size mismatch
/// or a malformed `data[]` is a typed 38000, never a silent misalignment.
pub(crate) fn run_batch(items: &[Option<&str>], model: Option<&str>) -> Vec<String> {
    // Empty-but-valid input: no HTTP call, empty result (the SQL wrapper COALESCEs to ARRAY[]::vector[]).
    if items.is_empty() {
        return Vec::new();
    }
    // NULL element breaks the N-in/N-out alignment (mirror ai.generate_batch) -> fail-fast 22023,
    // BEFORE any GUC read or HTTP call.
    let inputs = validate_inputs(items);
    // GUC read (SPI) — this is why the standalone path needs an open txn; the vectorizer's phase B does
    // NOT call this (it uses `run_batch_resolved` with cfg resolved earlier, so the embed pins no snapshot).
    let (endpoint, mdl, api_key) = resolve_cfg("theodb.embed_batch", model);
    embed_resolved(&inputs, &endpoint, &mdl, api_key.as_deref())
}

/// M122 — the phase-B embed entry for the vectorizer worker: the endpoint/model/api_key were already resolved
/// in phase A (inside a txn), so this does **NO GUC read and NO SPI** — pure HTTP+parse. Called with NO open
/// `BackgroundWorker::transaction`, so `backend_xmin` is not pinned for the HTTP round-trip. Same N-in/N-out
/// contract + error messages as [`run_batch`]; the two share [`embed_resolved`] (one HTTP+parse path, DRY).
///
/// LOAD-BEARING INVARIANT (do NOT break): everything reachable from here — `validate_inputs`, `embed_resolved`,
/// `post_json`, `format_embedding`, `err_*` — MUST stay free of `Spi::`/`guc()`/`current_setting`/`palloc`-heavy
/// PG work. The worker catches a longjmp out of this call with `PgTryBuilder` while holding **no open
/// transaction**; that catch is safe ONLY because no PG resource (SPI conn, snapshot, buffer pin, subtxn) is
/// held here. Adding SPI/txn work to this path without wrapping it in a transaction would make the off-txn catch
/// unsafe (skipped cleanup) AND re-pin `backend_xmin` — defeating M122 (see ADR-0049, council-rust-pgrx LOW-2).
pub(crate) fn run_batch_resolved(
    items: &[Option<&str>],
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }
    let inputs = validate_inputs(items);
    embed_resolved(&inputs, endpoint, model, api_key)
}

/// NULL-check the batch (fail-fast 22023 BEFORE any GUC/HTTP) → the non-null `&str` inputs.
fn validate_inputs<'a>(items: &'a [Option<&'a str>]) -> Vec<&'a str> {
    let mut inputs: Vec<&str> = Vec::with_capacity(items.len());
    for it in items {
        match it {
            Some(s) => inputs.push(s),
            None => err_input("theodb.embed_batch: array elements must not be NULL"),
        }
    }
    inputs
}

/// The pure HTTP+parse tail shared by [`run_batch`] and [`run_batch_resolved`]: build the payload, POST, and
/// map each embedding back by its `index`. NO GUC/SPI — safe to call with no open transaction (M122 phase B).
fn embed_resolved(
    inputs: &[&str],
    endpoint: &str,
    mdl: &str,
    api_key: Option<&str>,
) -> Vec<String> {
    let n = inputs.len();
    let payload = serde_json::json!({ "input": inputs, "model": mdl }).to_string();
    let body = post_json("theodb.embed_batch", endpoint, payload, api_key);
    parse_embedding_data(&body, n)
}

/// B-009 — o mapeamento resposta→saída, extraído como função PURA para poder ser testado sem rede.
///
/// Os seis caminhos de erro tipado abaixo viviam depois do `post_json`, o que os tornava inalcançáveis
/// por teste unitário: exercitá-los exigiria um provedor HTTP que devolvesse cada forma malformada. Medido
/// em 2026-08-21: `embed.rs` tinha **1** teste em 236 linhas — o extremo inferior do crate — e é a
/// superfície que fala com provedor externo, ou seja, a que mais tem modo de falha que só aparece em
/// produção.
///
/// O molde é o `parse_rerank_results` do irmão `rerank.rs`, que já resolveu o mesmo problema no mesmo
/// crate e por isso tem 6 testes de erro tipado. Extração pura: nenhuma mudança de comportamento.
fn parse_embedding_data(body: &Value, n: usize) -> Vec<String> {
    let data = match body.get("data").and_then(|d| d.as_array()) {
        Some(a) => a,
        None => err_external(&format!(
            "theodb.embed_batch: unexpected embedding response shape: {}",
            truncate(&body.to_string(), 200)
        )),
    };
    // N-in/N-out count invariant.
    if data.len() != n {
        err_external(&format!(
            "theodb.embed_batch: batch size mismatch: requested {} embeddings, endpoint returned {}",
            n,
            data.len()
        ));
    }

    // Map each embedding back by its `index` (OpenAI guarantees it), NOT array position — so an
    // out-of-order response is still placed correctly. A missing/out-of-range/duplicate index is a
    // malformed response (38000), not a silent misalignment.
    let mut out: Vec<Option<String>> = vec![None; n];
    for (pos, item) in data.iter().enumerate() {
        let idx = item.get("index").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(pos);
        if idx >= n {
            err_external(
                "theodb.embed_batch: unexpected embedding response shape: index out of range",
            );
        }
        if out[idx].is_some() {
            err_external(
                "theodb.embed_batch: unexpected embedding response shape: duplicate index",
            );
        }
        let emb = match item.get("embedding").and_then(|e| e.as_array()) {
            Some(a) => a,
            None => err_external(
                "theodb.embed_batch: unexpected embedding response shape: missing embedding array",
            ),
        };
        out[idx] = Some(format_embedding(emb, "theodb.embed_batch"));
    }

    // Bijection guaranteed (N data items, N slots, in-range, no duplicate) — every slot is filled.
    out.into_iter()
        .map(|o| {
            o.unwrap_or_else(|| err_external("theodb.embed_batch: missing embedding for an index"))
        })
        .collect()
}

/// M122 — resolve the batch embed cfg (endpoint/model/api_key) from the session GUCs. Public so the vectorizer's
/// phase A can resolve it INSIDE the read txn (this reads GUCs via SPI, so it needs an open txn); phase B then
/// calls `run_batch_resolved` with the returned owned values and pins no snapshot during the HTTP.
pub(crate) fn resolve_batch_cfg(model: Option<&str>) -> (String, String, Option<String>) {
    resolve_cfg("theodb.embed_batch", model)
}

/// Resolve the endpoint (with SSRF http(s) guard), model, and optional api key from session GUCs.
/// `fn_name` prefixes the typed-error messages so `embed` and `embed_batch` report under their own name
/// (parity with the per-row messages the oracle pins).
fn resolve_cfg(fn_name: &str, model: Option<&str>) -> (String, String, Option<String>) {
    let endpoint = match guc("theodb.embedding_endpoint") {
        Some(e) => e,
        None => err_input(&format!(
            "{fn_name}: theodb.embedding_endpoint is not set — \
             SET theodb.embedding_endpoint = 'http://host:port/v1/embeddings'"
        )),
    };

    // SSRF hardening: only http(s); refuse file://, ftp://, gopher://, etc. (the GUC is session-settable).
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        err_input(&format!("{fn_name}: endpoint must be http(s)://"));
    }

    let mdl: String = model
        .map(|s| s.to_string())
        .or_else(|| guc("theodb.embedding_model"))
        .unwrap_or_else(|| "default".to_string());
    let api_key = guc("theodb.embedding_api_key");

    (endpoint, mdl, api_key)
}

/// Format a JSON embedding array as a pgvector text literal `"[x,y,z]"` (the SQL wrapper casts `::vector`).
/// An empty array or a non-numeric element is a malformed response (38000), not a silent zero/empty.
fn format_embedding(emb: &[Value], fn_name: &str) -> String {
    if emb.is_empty() {
        err_external(&format!("{fn_name}: endpoint returned an empty embedding"));
    }
    let mut out = String::with_capacity(emb.len() * 8 + 2);
    out.push('[');
    for (i, v) in emb.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // Non-numeric entries are a malformed response (38000), not a silent 0.
        match v.as_f64() {
            Some(f) => out.push_str(&f.to_string()),
            None => err_external(&format!(
                "{fn_name}: unexpected embedding response shape: non-numeric vector element"
            )),
        }
    }
    out.push(']');
    out
}

// M25 — unit test for the pure embedding formatter (parity with the vec/nl/sbq test discipline).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;
    use serde_json::json;

    #[pg_test]
    fn format_embedding_renders_numeric_array() {
        let emb = vec![json!(1.0), json!(-2.5), json!(0.0)];
        assert_eq!(format_embedding(&emb, "test"), "[1,-2.5,0]");
    }

    // ---- B-009: os seis caminhos de erro TIPADO do mapeamento resposta→saída ----
    //
    // Cada um assere a MENSAGEM inteira, não apenas que lança. `#[pg_test(error = ...)]` compara o texto
    // completo, então um erro que mude de forma quebra o teste — que é o ponto: o contrato com quem
    // depura em produção é a mensagem, não o fato de haver erro (`rules/error-handling.md` § 2).
    //
    // Estes caminhos eram inalcançáveis até o `parse_embedding_data` ser extraído: exercitá-los exigiria
    // um provedor HTTP devolvendo cada forma malformada.
    //
    // O prefixo `b009_embed_` não é decoração: `#[pg_test]` gera um símbolo ACHATADO por nome de teste, então
    // nomes colidem entre MÓDULOS. Três destes já existiam no irmão `rerank.rs` e o link falhava com
    // `symbol ... is already defined` — descoberto compilando, não lendo.

    #[pg_test]
    fn b009_embed_parse_maps_embeddings_by_index_not_by_position() {
        // Resposta FORA DE ORDEM. O código mapeia por `index`, e este teste é o que prova — com
        // mapeamento por posição, a saída sairia trocada em silêncio, que é o pior modo de falha aqui.
        let body = json!({"data": [
            {"index": 1, "embedding": [3.0, 4.0]},
            {"index": 0, "embedding": [1.0, 2.0]},
        ]});
        assert_eq!(parse_embedding_data(&body, 2), vec!["[1,2]", "[3,4]"]);
    }

    #[pg_test(error = "theodb.embed_batch: unexpected embedding response shape: {}")]
    fn b009_embed_parse_missing_data_key_fails_typed() {
        let _ = parse_embedding_data(&json!({}), 1);
    }

    #[pg_test(
        error = "theodb.embed_batch: batch size mismatch: requested 3 embeddings, endpoint returned 1"
    )]
    fn b009_embed_parse_size_mismatch_fails_typed() {
        // A invariante N-entra/N-sai. Sem ela, um provedor que descarta uma entrada devolveria menos
        // vetores do que linhas, e o desalinhamento seguiria silencioso para dentro da tabela.
        let body = json!({"data": [{"index": 0, "embedding": [1.0]}]});
        let _ = parse_embedding_data(&body, 3);
    }

    #[pg_test(
        error = "theodb.embed_batch: unexpected embedding response shape: index out of range"
    )]
    fn b009_embed_parse_index_out_of_range_fails_typed() {
        let body = json!({"data": [{"index": 5, "embedding": [1.0]}]});
        let _ = parse_embedding_data(&body, 1);
    }

    #[pg_test(error = "theodb.embed_batch: unexpected embedding response shape: duplicate index")]
    fn b009_embed_parse_duplicate_index_fails_typed() {
        let body = json!({"data": [
            {"index": 0, "embedding": [1.0]},
            {"index": 0, "embedding": [2.0]},
        ]});
        let _ = parse_embedding_data(&body, 2);
    }

    #[pg_test(
        error = "theodb.embed_batch: unexpected embedding response shape: missing embedding array"
    )]
    fn b009_embed_parse_missing_embedding_array_fails_typed() {
        let body = json!({"data": [{"index": 0}]});
        let _ = parse_embedding_data(&body, 1);
    }

    #[pg_test(
        error = "theodb.embed_batch: unexpected embedding response shape: non-numeric vector element"
    )]
    fn b009_embed_parse_non_numeric_element_fails_typed() {
        let body = json!({"data": [{"index": 0, "embedding": ["nao e numero"]}]});
        let _ = parse_embedding_data(&body, 1);
    }

    #[pg_test(error = "theodb.embed_batch: array elements must not be NULL")]
    fn b009_embed_validate_inputs_rejects_null_element_typed() {
        // O caminho de entrada, não o de resposta. Um NULL no array recusa ANTES do egress — o que
        // também significa que não gastamos uma chamada ao provedor para descobrir isso.
        let _ = validate_inputs(&[Some("a"), None]);
    }
}
