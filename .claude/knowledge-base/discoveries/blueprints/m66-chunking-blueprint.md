# Blueprint M66 — Estratégias de chunking declarativas no vectorizer

**Milestone:** M66 · **Data:** 2026-07-09 · **Método:** R0 (WebSearch/WebFetch — papers/OSS/blogs, ≥2 fontes por claim) + council-ai-in-db (mapa file:line do vectorizer M54).

## Coverage Corner 1 — Integration Tests

- Vectorizer M54 testado via `#[pg_test]` em `vectorizer.rs:667-906` (fila fatiada, roda sem worker/OpenAI). Já há testes de chunk: `chunk_text_windows_with_overlap` (`:842`), `chunk_text_empty_returns_empty` (`:854`), `chunk_text_rejects_overlap_ge_size` (`:859`). E2E: `scripts/vectorizer-e2e.sh`.
- **M66:** chunking é **lógica de string pura → unit-testável offline** (o ponto forte). Testar cada estratégia + edge/negative sem rede.

## Coverage Corner 2 — Dependencies

- **Own-code vs dep:** chunking é lógica de string (Rule 9 aplicada com cuidado). `fixed`+`recursive`+`overlap` = own-code trivial (str slicing em fronteira de char UTF-8 — Rust garante char-boundary). `sentence` = fronteira Unicode (UAX#29) sutil → NÃO reinventar; usar `unicode-segmentation` (MIT/Apache) ou o crate `benbrandt/text-splitter` (MIT — faz fixed/recursive/sentence + Unicode + tokens). **v1 character-based** (sem tokenizer BPE — tiktoken seria complexidade acidental; pgai também começou char-based).
- Reusa `embed::run_batch` (`embed.rs:55` — N chunks → N vetores num round-trip) e `http.rs`.

## Coverage Corner 3 — Tools

- Benchmark reusa o harness BEIR do M53/M65 (`theodb_bench` + `metrics.ndcg_at_k`/`recall_at_n`) + OpenAI embed cacheado.
- Metodologia **k-adaptativo** (Vecta): igualar o budget de contexto entre estratégias (`k = target_tokens / avg_chunk_size`) — comparar top-k fixo é INJUSTO (semantic com chunks de 43 tokens "ganha" page-recall mas entrega lixo).

## Coverage Corner 4 — Techniques

**Achado central (a evidência, ≥2 fontes):**
1. **Recursive é o default robusto.** Chroma (5 corpora, 472 queries): recursive@200 = 88.1% recall; semantic só +0-4pp ao custo de embeddar cada sentença ([Chroma](https://www.trychroma.com/research/evaluating-chunking)). Vecta (50 papers): recursive-512 = melhor overall (69% accuracy) ([Vecta](https://www.runvecta.com/blog/we-benchmarked-7-chunking-strategies-most-advice-was-wrong)).
2. **Recall alto ≠ resposta correta.** Semantic teve page-F1 0.91 mas colapsou a 54% accuracy (chunks de ~43 tokens, bons isolados, pobres em contexto) ([Vecta](https://www.runvecta.com/blog/we-benchmarked-7-chunking-strategies-most-advice-was-wrong)).
3. **Semantic geralmente não paga o custo.** *Is Semantic Chunking Worth the Computational Cost?* (Qu 2024, [arXiv:2410.13070](https://arxiv.org/abs/2410.13070)): o ganho só aparece em datasets "stitched" artificiais; some em docs reais (HotpotQA: fixed 90.6% vs semantic 87.4%). "not justified by consistent performance gains." Corroborado por [arXiv:2606.00881](https://arxiv.org/pdf/2606.00881).
4. **Overlap** melhora recall nas bordas (10-20% típico), mas evidência **mista** acima de 20% (AstroRAG 30% ótimo; CRUD-RAG até 70%; outros 0%) → `overlap` tunável, default ~15% é ponto de partida não verdade ([arXiv:2401.17043](https://arxiv.org/pdf/2401.17043)).
- **API declarativa (pgai SOTA):** `ai.chunking_recursive_character_text_splitter(chunk_size, chunk_overlap, separators)` ([pgai](https://github.com/timescale/pgai/blob/released/docs/utils/chunking.md)). LangChain `RecursiveCharacterTextSplitter` (hierarquia `\n\n`→`\n`→`. `→` `) ([LangChain](https://docs.langchain.com/oss/python/integrations/splitters/recursive_text_splitter)). LlamaIndex `SentenceSplitter` (default, evita "hanging sentences").

## O estado atual (council-ai-in-db, file:line) — o que muda

- Vectorizer M54 em `theodb_rs/src/vectorizer.rs`. `theodb.create_vectorizer(...)` (`:86-94`) grava config + trigger; worker async (`:516-665`) drena a fila.
- **1→1 hoje:** `content → embed::run → 1 vetor` escrito **in-place** (`UPDATE target SET col = $1::vector WHERE pk`, `:361-367`). `theodb.chunk_text` (`:120-144`) EXISTE mas é **código morto** (só nos testes, nunca no fluxo).
- Catálogo `theodb.vectorizer` (`:25-35`) sem colunas de chunking.
- **O recall-mover real é o schema 1-doc→N-chunks** — o modelo in-place (1 vetor/linha) não comporta N chunks; precisa de uma **chunk table separada** `(source_pk, chunk_index, chunk_text, embedding)` com FK (como o pgai).

## ADR-1 — Escopo: chunker own-code + modo chunk-table OPT-IN (não-breaking)

Implementar `chunk.rs` (Rust puro): `fixed`, `sentence`, `recursive` + `overlap`, Unicode-safe. Wire no vectorizer via um **modo opt-in** `WITH (chunk_strategy=…, chunk_size=…, overlap=…)` que escreve numa **chunk table separada** `{target}_chunks (source_pk, chunk_index, chunk_text, embedding)` — 1 doc → N chunks → N vetores (via `embed::run_batch`). O modo 1→1 in-place atual é **preservado** (default sem `chunk_strategy` → não-breaking; o query contract existente não muda). Quem opta pelo chunk-table faz retrieval com join/agregação sobre a chunk table.

## ADR-2 — Deferir `semantic` (honest-negative por evidência)

NÃO implementar semantic chunking no M66. Ganho de recall 0-4pp e frequentemente **negativo** end-to-end, 14× mais caro (embeddar cada sentença), hiperparâmetro frágil (threshold arbitrário por modelo). O ganho só existe em corpora sintéticos com tópicos entrelaçados. Registrar como decisão baseada em evidência (arXiv:2410.13070), não gap.

## O gate REAL — benchmark de recall por estratégia

O `chunk_text` atual ser **código morto não medido** é o sintoma: código plausível, ganho não comprovado. O DoD exige "recall de RAG por estratégia". Metodologia honesta: corpus fixo (BEIR SciFact/NFCorpus) + query set com qrels; variar (fixed vs recursive vs sentence) × (size 256/512) × (overlap 0/64); **k-adaptativo** (igualar budget de contexto); medir recall@k + nDCG@10 por corpus (não média que esconde a dependência de corpus). Reportar honestamente "config X vence no corpus Y", não "X é melhor". Se uma estratégia não move o recall, honest-negative.

## Edge/negative cases (o DoD exige)

| Caso | Tipo | Tratamento |
|---|---|---|
| Doc vazio / só whitespace | edge | 0 chunks (não 1 vazio); não embeddar |
| Doc gigante | edge | chunka normal (iterativo) |
| Doc < chunk_size / 1 token | edge | 1 chunk (o doc inteiro, sem padding) |
| Palavra gigante sem separador | edge | recursive cascateia até char-cut forçado; NUNCA loop infinito nem chunk > size |
| Unicode/multibyte (emoji/CJK) | edge sutil | cortar em fronteira de char Unicode, **NUNCA byte** (Rust String UTF-8 garante) |
| `overlap >= chunk_size` | negative | typed error no boundary (fail-fast) |
| `chunk_size <= 0` | negative | typed error |
| `chunk_strategy` desconhecido | negative | typed error listando os válidos |

## Débito honesto

- Chunk-table muda o query contract (retrieval passa a join/agregar sobre chunks) — por isso é OPT-IN no M66 (não-breaking).
- Token-based chunking (tiktoken) fica para futuro (v1 é char-based).
- `overlap > 20%` é evidência mista — default ~15%, tunável.
