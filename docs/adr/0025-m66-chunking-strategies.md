# ADR 0025 — M66 chunking declarativo: chunker own-code char-based + chunk-table opt-in; semantic deferido

**Status:** Accepted · **Data:** 2026-07-09 · **Milestone:** M66 · **Owner:** Eng
**Relacionado:** blueprint `.claude/knowledge-base/discoveries/blueprints/m66-chunking-blueprint.md`,
plan `.claude/knowledge-base/plans/m66-chunking-plan.md`, ADR `0016` (vectorizer M54),
`.claude/rules/parsimony-ladder.md`, `.claude/rules/testing.md §4.1` (edge vs negative), Unbreakable Rule 9.

## Contexto

O vectorizer (M54) auto-embeda colunas de texto, mas o **chunking domina a qualidade do RAG** e o vectorizer
era 1-doc→1-vetor in-place (`vectorizer.rs:361`). A discovery (blueprint, R0 web-citado, ≥2 fontes por claim)
concluiu: (a) implementar fixed/sentence/recursive + overlap; (b) o **recall-mover real é o schema
1-doc→N-chunks** (o `theodb.chunk_text` plpgsql era **código morto** — existia mas nunca era chamado no fluxo);
(c) chunking é lógica de string (own-code char-based v1); (d) **semantic não paga o custo** (arXiv:2410.13070).

## Decisão D1 — Chunker own-code char-based (fixed/sentence/recursive + overlap); NÃO adotar tokenizer no v1

`theodb.chunk(content text, strategy text DEFAULT 'recursive', chunk_size int DEFAULT 512, overlap int DEFAULT 64)
RETURNS text[]` em `chunk.rs` (Rust puro): `fixed` (janelas deslizantes), `sentence` (agrupa sentenças `.!?`),
`recursive` (hierarquia `\n\n`→`\n`→`. `→` ` à la LangChain) + `overlap` ortogonal. **Char-based, Unicode-safe**
(conta/corta por char UTF-8, nunca por byte → grapheme multibyte nunca é partido). Substitui o `chunk_text`
plpgsql morto (KISS — um só chunker).

**Rationale:** char-based resolve o caso comum (pgai também começou char-based); token-based (tiktoken/BPE) é
complexidade acidental para o v1 (débito rastreado). Own-code justifica-se: a API SQL do TheoDB precisa de
controle que um crate não dá 1:1; o diff é lógica de string. A lógica pura é unit-testável offline (`cargo test`)
— o antídoto ao chunk_text morto não-medido.

**Alternativas rejeitadas:**
- **(A) Adotar `benbrandt/text-splitter` (MIT) inteiro** — resolve sentence/recursive/Unicode/tokens de uma vez,
  mas traz dep + a API não casa 1:1 com os reloptions; para o v1 own-code char-based é menor diff. Reavaliar em v2.
- **(B) Token-based (tiktoken-rs)** — mais correto (budget do gerador em tokens) mas exige o BPE; deferido para v2.

## Decisão D2 — Modo chunk-table OPT-IN (não-breaking); semantic DEFERIDO por evidência

O chunking é opt-in via `create_vectorizer(..., chunk_strategy, chunk_size, chunk_overlap)`: quando
`chunk_strategy` é não-NULL, o worker cria/usa a chunk-table `{target_table}_chunks (source_pk, chunk_index,
chunk_text, embedding)` — 1 doc → N chunks → N vetores (via `embed::run_batch`, um round-trip por doc; DELETE
os chunks antigos do PK antes do INSERT — sem órfãos no re-embed). O modo 1→1 in-place é **preservado** quando
`chunk_strategy` é NULL (default). `semantic` NÃO é implementado.

**Rationale:** mudar in-place → chunk-table é breaking no query contract (retrieval passa a join/agregar sobre
chunks); opt-in preserva os vectorizers/queries existentes. Semantic deferido **por evidência**
(arXiv:2410.13070 — ganho 0-4pp, frequentemente negativo end-to-end, 14× custo; o ganho só existe em corpora
sintéticos com tópicos entrelaçados), não por falta de tempo.

**Alternativas rejeitadas:**
- **(A) Trocar in-place por chunk-table (breaking)** — quebraria vectorizers/queries existentes. Rejeitada.
- **(B) Implementar semantic** — evidência contra (custo alto, ganho não-universal). Rejeitada.

## Edge/negative (o DoD exige; testing.md §4.1)

- **Edge (válido):** vazio/whitespace→0 chunks; doc<size→1 chunk; palavra gigante sem separador→char-cut forçado
  (nunca loop infinito nem chunk>size); multibyte (emoji/CJK)→fronteira de char (nunca byte). **Bug pego em teste
  de mesa antes do commit:** o carry de overlap do `pack_with_overlap` acumulava chunks > size quando overlap=0.
- **Negative (inválido):** `overlap>=size`/`size<=0`/strategy desconhecida → typed error 22023 (fail-fast).

## Nota de upgrade (honesta)

As colunas de chunking vivem no schema do `theodb_rs` (extension_sql em `vectorizer.rs`) — um **fresh install**
(`CREATE EXTENSION theodb_rs`) já as traz. `theodb_rs` é uma extensão pgrx sem migrações incrementais (version
1.0.0 regenerada a cada build), então um deployment `theodb_rs` PRÉ-existente precisa reinstalar a extensão OU um
`ALTER TABLE theodb.vectorizer ADD COLUMN chunk_strategy text, ADD COLUMN chunk_size int DEFAULT 512, ADD COLUMN
chunk_overlap int DEFAULT 64` manual. Declarado honestamente (não há um `theodb--1.4--1.5.sql` porque o schema é
do theodb_rs, não do umbrella theodb).

## Evidência (medida)

- **pg_test GREEN (stack real):** chunk 16/16 (3 estratégias + edge/negative + multibyte + error-paths) +
  vectorizer 13/13 (chunk-table criada + config; default NULL preservado; delete remove N chunks). 9 unit-tests puros.
- **Benchmark BEIR/NFCorpus (50 queries) — STRATEGY_MATTERS (rigor declarado):** `sentence`/`recursive`
  (nDCG@10 0.397/0.391) > `fixed` (0.372), spread total **0.025** — o degrau robusto é **sentence > fixed (Δ0.025)**;
  o degrau fino **sentence vs recursive (Δ0.0055) é empate estatístico** (dentro do ruído de 50 queries; NÃO
  afirmado). k-adaptativo iguala o budget (comparação justa). Dependente de corpus (não-universal). **Débito honesto
  (council-benchmark):** n=1 run, `noise_tol` assumido; separar sentence/recursive exige std pareado + ≥3 runs
  (`analysis-golden-rule §3`) — o harness agora reporta ndcg10_std. `docs/benchmarks/m66-chunking.{md,json}`.

## Consequências

- **Chunking declarativo own-code** fecha o gap (o chunk_text morto vira `theodb.chunk` medido).
- **Recall-mover 1-doc→N-chunks** disponível opt-in (não-breaking).
- **Char-based v1** (token-based é v2 rastreado); **semantic deferido** por evidência.

## Caveats honestos

Char-based pode divergir do budget de tokens do gerador (declarado, v2). O benchmark de recall por estratégia é
dependente de corpus (a literatura mostra que "config X vence no corpus Y" não generaliza) — reportado por-corpus.
