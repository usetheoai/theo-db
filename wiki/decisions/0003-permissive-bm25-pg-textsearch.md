---
type: Decision
title: ADR 0003 — BM25 permissivo: identificação do pg_textsearch
description: A peça BM25 permissiva do TheoDB é timescale/pg_textsearch (PostgreSQL License); a adoção na distribuição fica gated por benchmark de recall.
resource: git:f7c7b93:docs/adr/0003-permissive-bm25-pg-textsearch.md
tags: [adr, bm25, lexical, licenca, busca-hibrida]
adr_id: "0003"
adr_status: Accepted
decision_date: 2026-06-28
owner: human:paulohenriquevn
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0003
    resource: git:f7c7b93:docs/adr/0003-permissive-bm25-pg-textsearch.md
    title: ADR 0003 — Permissive BM25 lexical ranking
    author: human:paulohenriquevn
    last_modified: 2026-06-28
---

Este ADR registra a **identificação** de uma peça, não a sua adoção. A distinção é deliberada e
é a doutrina measurement-first do [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)
aplicada a uma dependência.

# Contexto

A [busca híbrida](/features/06-busca-hibrida.md) precisa de uma perna lexical
[BM25](/technologies/bm25.md). A SOTA dessa capacidade — o `pg_search` do ParadeDB — é
**AGPL-3.0**, barrada pela política de licença permissiva do projeto. O roadmap transformou isso
num risco explícito e num critério de pronto: "alternativa permissiva a BM25 full-text
identificada".

# Decisão

A peça BM25 permissiva do TheoDB é **`timescale/pg_textsearch`**:

- **PostgreSQL License** — permissiva, verificada verbatim no repositório canônico.
- GA `v1.3.1` (2026-06-23).
- **Okapi BM25 verdadeiro** com $k_1 = 1{,}2$ e $b = 0{,}75$, Block-Max WAND,
  `CREATE INDEX … USING bm25(content)` e o operador `content <@> 'query'`.
- Verificada **ao vivo** sobre a imagem de dev: build PGXS limpo e query BM25 corretamente
  rankeada (`k1=1.20, b=0.75, avg_length=3.80`).

**A adoção na imagem de distribuição NÃO acontece aqui.** Fica gated pelo benchmark de recall
reproduzível contra o `ts_rank_cd` já entregue — a medição registrada em
[m7 — BM25 vs ts_rank](/benchmarks/m7-bm25-vs-tsrank.md). O leg lexical default permanece
`ts_rank_cd` + [RRF](/technologies/rrf.md) até a medição justificar a troca.

# Alternativas consideradas

| Alternativa | Veredito | Motivo |
|---|---|---|
| VectorChord-bm25 (`vchord_bm25`) | Rejeitada | dual AGPLv3 / Elastic License v2 — nenhuma das duas é permissiva |
| BM25 próprio em SQL/plpgsql sobre `ts_stat` | Rejeitada | reinvenção de extensão permissiva madura; o PG expõe os inputs, mas manter implementação própria é custo sem ganho |
| Manter só `ts_rank_cd` (cover-density, **não** é BM25) | Default interino | já é paridade lexical com o AlloyDB, mas não fecha o gap "BM25 verdadeiro" |
| `psql_bm25s` (Apache-2.0) | Fallback registrado | caso o pg_textsearch regrida |

# BM25F está fora de escopo

BM25F — BM25 multi-campo com pesos por campo e combinação **pré-saturação** (Robertson,
Zaragoza & Taylor, 2004) — foi explicitamente excluído, por quatro razões encadeadas:

1. **Necessidade.** O schema de busca é single-field (`content` + `embedding`); ninguém pediu
   pesagem multi-campo.
2. **A peça não entrega de graça.** O índice do pg_textsearch é single-column — BM25 puro.
3. **Anti-pattern.** Aproximar BM25F por soma ponderada de scores BM25 por-campo é exatamente
   o erro que o BM25F foi criado para corrigir: satura cada campo separadamente.
4. **Measurement-first.** O BM25 puro contra `ts_rank_cd` ainda não fora medido; BM25F seria
   otimização prematura sobre ganho não comprovado.

Reabre apenas com (a) caso de uso multi-campo concreto e (b) ganho medido sobre a perna
single-field.

# Consequências

Fecha o critério de pronto com evidência — identificação, prova funcional e medição — e dá ao
time o gate para decidir adoção. Em contrapartida, o pg_textsearch exige
`shared_preload_libraries=pg_textsearch` (uma restrição operacional que pesa na decisão) e uma
dependência de build adicional *se* adotado; por isso vive numa imagem descartável até a
medição justificar.[^adr0003]

A trajetória posterior desta linha está no
[ADR 0054](/decisions/0054-m140-3-bm25-supersede-textsearch.md), onde o BM25 próprio supersede
o `ts_rank_cd`.

[^adr0003]: ADR 0003 — Permissive BM25 lexical ranking: pg_textsearch
