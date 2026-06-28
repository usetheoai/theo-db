# ADR 0003 — Permissive BM25 lexical ranking: pg_textsearch (identification)

**Status:** Accepted · **Data:** 2026-06-28 · **Owner:** paulohenriquevn
**Relacionado:** ADR `0002-north-star-equal-or-superior-to-alloydb` (measurement-first / D3), ADR `0001-no-engine-fork`, PRD §11/§15 (D1 — AGPL barrada), `ROADMAP.md` M7-S2, blueprint `m7-bm25-permissive`

> Esta ADR registra a **identificação** da peça BM25 permissiva (o DoD do M7-S2). A **adoção na distribuição**
> é uma decisão futura, *gated* pelo benchmark de recall produzido nesta fatia (measurement-first, ADR 0002).

## Contexto

M7 (IA avançada) precisa de um leg lexical BM25 para a busca híbrida. A SOTA dessa capacidade — ParadeDB
`pg_search` — é **AGPL-3.0**, barrada pela D1 (`.claude/knowledge-base/references/paradedb/LICENSE:1`). A
ROADMAP M7 transformou isso num risco explícito (#1) e num DoD: "alternativa permissiva a BM25 full-text
identificada". A discovery `m7-bm25-permissive` (blueprint SHIPPABLE_WITH_CAVEATS 89) investigou o campo com
due-diligence de licença sourceada dos repos canônicos.

## Decisão

**A peça BM25 permissiva do TheoDB é `timescale/pg_textsearch`** — **PostgreSQL License** (permissiva,
D1-clean, verbatim do repo canônico), GA `v1.3.1` (2026-06-23), **Okapi BM25 verdadeiro** (`k1=1.2`/`b=0.75`,
Block-Max WAND, `CREATE INDEX … USING bm25(content)` + operador `content <@> 'query'`). Verificada **ao vivo**
sobre `theo-db:dev`: build PGXS limpo + query BM25 corretamente rankeada (`k1=1.20, b=0.75, avg_length=3.80`).

**A adoção de `pg_textsearch` na imagem de distribuição NÃO é feita agora** — fica *gated* pelo benchmark de
recall reproduzível vs. o `ts_rank_cd` já entregue (M7-S1), produzido nesta fatia (measurement-first, ADR
0002 / PRD D3). O leg lexical default permanece `ts_rank_cd`+RRF até a medição justificar a troca.

## Alternativas consideradas

- **VectorChord-bm25 (`vchord_bm25`/TensorChord):** **rejeitada** — dual **AGPLv3 / Elastic License v2**
  (verbatim do repo canônico) → barrada pela D1 (nenhuma das duas é permissiva).
- **BM25 próprio em SQL/plpgsql sobre `ts_stat`:** **rejeitada** — Regra 9 (reinvenção de uma extensão
  permissiva já existente e madura). PostgreSQL expõe os inputs (`ts_stat.ndoc`, `length(tsvector)`), mas
  manter uma implementação própria é custo sem ganho frente ao pg_textsearch.
- **Manter só `ts_rank_cd` (cover-density, NÃO é BM25):** mantida como **default interino** (já é paridade
  lexical com o AlloyDB), mas não fecha o gap "BM25 verdadeiro" — por isso a identificação do pg_textsearch.
- **`psql_bm25s` (Apache-2.0):** permissiva, registrada como **fallback** caso pg_textsearch regrida.

## Consequências

- **Habilita:** fecha o DoD do M7-S2 com evidência (identificação + prova funcional + medição); dá ao time o
  gate measurement-first para decidir a adoção na distribuição.
- **Restringe:** pg_textsearch exige `shared_preload_libraries=pg_textsearch` (constraint operacional a pesar
  na decisão de adoção) e adiciona uma dependência de build (`postgresql-server-dev`) **se** adotada — por
  isso fica numa imagem throwaway (`packaging/Dockerfile.bm25`) até a medição justificar.
- **Licença:** verdito reproduzível em `packaging/license-sweep.sh` (re-fetch do repo canônico) — drift é
  pego a cada run.

## § D4 — BM25F (fielded BM25) está FORA de escopo

BM25F (BM25 multi-campo com pesos por campo, combinação **pré-saturação** — Robertson, Zaragoza & Taylor
2004) **não entra no M7-S2**. Razões (parsimony-ladder rung 1 + measurement-first):

1. **Necessidade:** o schema de busca do TheoDB é **single-field** (`content` + `embedding`); ninguém pediu
   pesagem multi-campo (YAGNI).
2. **A peça não entrega de graça:** o índice do pg_textsearch é single-column (`USING bm25(content)`) — BM25
   puro, não BM25F.
3. **Anti-pattern:** aproximar BM25F por soma ponderada de scores BM25 por-campo é exatamente o erro que o
   BM25F foi criado para corrigir (satura cada campo separadamente). Não fazemos isso.
4. **Measurement-first:** ainda não medimos o BM25 puro vs. `ts_rank_cd` (o gate desta fatia) — BM25F seria
   otimização prematura sobre ganho não comprovado.

**Seed futuro (gated):** reabrir BM25F apenas quando houver (a) caso de uso multi-campo concreto (ex.: docs
com title/abstract/body) E (b) ganho medido sobre o leg single-field.

## Quando esta ADR muda

A **identificação** é estável. A **decisão de adoção na distribuição** é tomada num ADR futuro quando o
benchmark (`docs/benchmarks/m7-bm25-vs-tsrank.md`) mostrar (ou não) ganho que justifique o custo de build +
`shared_preload_libraries`. Trocar a peça identificada exige nova due-diligence de licença (sweep) + nota aqui.
