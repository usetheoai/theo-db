---
type: Measurement
title: m188 — as 20 falhas da suíte, classificadas: 10 eram classificação de teste, 10 seguem abertas
description: Metade das falhas da primeira execução não era defeito do banco mas teste mal registrado ou com premissa falsa; as outras dez estão classificadas por causa capturada, e três delas compartilham a mesma assinatura.
resource: wiki/runbooks/rodar-a-suite-de-testes.md
tags: [benchmark, m188, suite, classificacao, b-012, honest-negative]
milestone: M188
generated: { by: claude-code/opus-5, at: 2026-08-10T16:00:00Z }
---

A suíte destravou pelo B-001 e revelou **20 falhas**. Este artefato as classifica por **causa capturada do
log do servidor** — não por suposição a partir do nome do teste.

# Resultado: 419/20 → 429/10

**As 10 que caíram não tiveram uma linha de mudança no comportamento do banco.** Todas eram defeito de
*registro ou classificação de teste*:

| falhas | causa | correção |
|---|---|---|
| 6 (`lexical::engine`) | módulo sem `#[pgrx::pg_schema]` → o pgrx nunca gerou as funções SQL, e o harness chamava `tests.<fn>()` inexistente | atributo adicionado |
| 2 (`m168_affinity`) | `#[pg_test]` num módulo `pg_schema` de nome próprio → funções no schema errado. E não tocam `pg_sys`: são `std::thread::current().id()` | reclassificados para `#[test]` |
| 2 (`vector_join`) | premissa falsa — `ef_search` limita o beam, não o resultado ([m187](/benchmarks/m187-vector-join-recall-defeito.md)) | beam elevado, alvo intacto |

**Isto é o achado mais importante do lote e ele é sobre o processo, não sobre o produto:** metade das falhas
de uma suíte que passou meses sem rodar era a própria suíte apodrecendo. Um teste que não executa não protege
nada **e acumula defeitos silenciosos próprios**.

# As 10 abertas, por causa capturada

| # | teste | causa (verbatim do servidor) | classificação |
|---|---|---|---|
| 1 | `autotune::explain_scan_shows_index_and_candidates` | `explain_scan shows real pages_read (got 0)` | **produto ou instrumentação** — não determinado |
| 2 | `autotune::scan_stats_records_real_pages_read` | idem, mesma família | idem |
| 3 | `columnar_project::chunk_skip_prunes_and_ab_identical` | `the table must span >= 2 chunk groups to prove pruning (scanned 0)` | **produto ou instrumentação** |
| 4 | `columnar_project::enable_chunk_skip_guc_off_disables_skip` | `with the GUC on the selective predicate must prune (got 0)` | idem |
| 5 | `columnar_project::predicate_pushdown_best_effort` | `the pushable a = 25000 conjunct must prune (got 0)` | idem |
| 6 | `graph::csr_build_guards_u32_boundary` | `graph_build: node ids must fit in u32` | teste de borda que **espera** o erro; provavelmente forma de captura |
| 7 | `http::m104_breaker_success_closes` | `open after K failures` | circuit breaker |
| 8 | `embed_unreachable_endpoint_fails_typed` | `refusing to call 127.0.0.1 — blocked internal address` | **guarda SSRF do produto funcionando**; o teste espera outro erro tipado |
| 9 | `rerank_unreachable_endpoint_fails_typed` | idem | idem |
| 10 | `vectorizer::process_delete_failure_does_not_mark_done` | não capturada | — |

## Os cinco `got 0` são a pista mais forte

Testes 1–5 falham todos com um **contador em zero** — `pages_read`, `chunks_scanned`, `chunks_skipped`. Cinco
testes de três módulos diferentes com a mesma assinatura sugere **uma causa comum**: ou os contadores não são
incrementados sob o harness, ou o caminho de varredura que eles medem não é tomado nesse ambiente.

**Não determinei qual.** A distinção importa exatamente como importou no [m187](/benchmarks/m187-vector-join-recall-defeito.md):
se for instrumentação, conserta-se o teste; se for produto, o chunk-skip não está podando e isso é um defeito
de performance real e silencioso.

## Os dois de egress são o caso mais interessante

`pg_embed_unreachable_endpoint_fails_typed` quer provar que um endpoint inalcançável produz **erro tipado**.
Recebe um erro tipado — mas **outro**: a guarda SSRF do produto recusa `127.0.0.1` antes de tentar conectar.

**O produto está certo e o teste está desatualizado**: a guarda é mais nova que ele. O conserto é o teste
passar a usar um endereço externo inalcançável, não afrouxar a guarda — que seria abrir um SSRF para fazer um
teste passar.

# O que este artefato NÃO faz

Não conserta as 10. **Classifica-as com a causa que o servidor emitiu**, que é o que o B-012 pediu e o que
transforma "20 vermelhos" numa lista de trabalho com hipótese própria cada. Duas delas (8 e 9) já têm o
conserto nomeado; cinco (1–5) precisam da distinção instrumentação-vs-produto antes de qualquer código.
