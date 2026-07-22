# ADR 0054 — BM25 own-code supersede a exceção permissiva do `pg_textsearch` (ADR-0013)

- **Status:** Aceito
- **Data:** 2026-07-22
- **Milestone:** M140.3 (engine BM25 de produção own-code)
- **Supersede (parcialmente):** ADR-0013 (BM25 via `pg_textsearch` como exceção v1-legacy) — só a parte BM25; o columnar do ADR-0013 é intocado.
- **Relacionado:** ADR-0006 (mandato v2: own-code, deps mínimas), ADR-0051 (spike M139 GO), ADR-0052 (heap, não AM), ADR-0053 (núcleo pgrx-free), `docs/benchmarks/m138-bm25-fusion.md` (#146), `docs/benchmarks/m140-1-lexical-measurement.md`.

## Contexto

O ADR-0013 (M30) manteve o `pg_textsearch` como **exceção permissiva** para BM25 — uma medição
throwaway (M7), NÃO embarcada, enquanto não houvesse own-code viável. A superfície shipada de texto
continuava sendo FTS nativo (`ts_rank_cd` + GIN). Desde então:

- **M139 (ADR-0051)** provou own-code BM25 viável **dentro do PG**: Tantivy 0.26 (MIT, D1-limpo) sobre
  um `Directory` nosso, buffer-then-flush ao heap → MVCC/WAL/crash herdados, sem rmgr/página custom.
- **M140.1 (`docs/benchmarks/m140-1-lexical-measurement.md`)** mediu que a BM25 own-engine bate o
  baseline shipado `ts_rank_cd` em retrieval lexical puro (BEIR + logs HDFS reais), com índice ~3,5×
  menor, e fica em **paridade** com o `pg_textsearch` na qualidade de ranking (~4% de diferença de
  impl BM25, dentro do ruído).
- **M140.2 (ADR-0053)** extraiu o núcleo pgrx-free testável; **M140.3** entrega a superfície de
  produção (`bm25_build`/`bm25_search`) com cache MVCC-correto.
- O leg in-DB do `pg_textsearch` foi medido **quebrado** (issue **#146**, M138) — nunca exercido ponta-a-ponta.

## Decisão

**A superfície BM25 de produção do TheoDB é own-code (Tantivy MIT via `bm25_build`/`bm25_search`); a
exceção permissiva do ADR-0013 para BM25 (`pg_textsearch`) fica superseded.** O `pg_textsearch` passa
a ser **referência de benchmark** (o número do M138), **não** um componente de produto.

## Rationale

- **Mandato v2 (ADR-0006):** own-code, deps mínimas permissivas. O Tantivy MIT é own-composição (nosso
  `Directory`/cache/superfície); o `pg_textsearch` é dep externa que exigiria `shared_preload_libraries`
  + reinício e está com o leg in-DB quebrado (#146) — o oposto de "bateria inclusa own-code".
- **Paridade medida + vantagens estruturais:** M140.1 mediu paridade de ranking com `pg_textsearch` e
  vitória sobre `ts_rank`; own-code adiciona índice ~3,5× menor + tokenização de logs/IDs + o caminho
  para features (phrase/fuzzy/facet) que o FTS nativo não tem — sem custo de dep externa.
- **Honestidade (Regra 7):** o ganho de qualidade sobre `pg_textsearch` **não** é o argumento (é
  paridade); o argumento é **own-code permissivo + cache + storage + o moat de consolidação in-PG**.

## Alternativas consideradas

- **Manter `pg_textsearch` como a superfície BM25.** Rejeitado: dep externa + `shared_preload` +
  reinício + leg quebrado (#146) + fora do mandato own-code. Zero benefício medido de qualidade.
- **Manter só o `ts_rank_cd` nativo (sem BM25 own-code).** Rejeitado: M140.1 mediu que `ts_rank_cd`
  perde para BM25 no retrieval lexical puro (o caso do theo-lens); o consumidor real se beneficia.

## Consequências

- **Habilita:** o roadmap lexical passa a own-code; o M140.4 prova MVCC/VACUUM/crash da superfície de
  produção e liga o primeiro consumidor (theo-lens).
- **Plano de saída do `pg_textsearch`:** quem tiver adotado a imagem com `pg_textsearch` (throwaway,
  nunca shipada por padrão) migra para `bm25_build`/`bm25_search` (own-code, sem preload). Como o
  `pg_textsearch` nunca foi default (ADR-0013: "medição throwaway, NÃO embarcada"), não há usuário de
  produção a migrar — a saída é remover a imagem de benchmark, não uma migração de dados de produção.
- **Restringe:** a superfície BM25 é a função own-code decidida no ADR-0052 (não um index AM; não o
  `pg_textsearch`). Mudança futura exige novo ADR.

## Referências

- ADR-0013 (a exceção superseded — só a parte BM25), ADR-0006 (mandato v2), ADR-0051/0052/0053 (a cadeia M139→M140.2).
- `docs/benchmarks/m140-1-lexical-measurement.md` (paridade vs pg_textsearch + vitória vs ts_rank), `docs/benchmarks/m140-3-bm25-engine.md` (o report deste milestone: latência cache vs reload + nDCG in-PG).
- issue #146 (leg in-DB do pg_textsearch quebrado).
