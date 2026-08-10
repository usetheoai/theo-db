---
type: Decision
title: ADR 0040 — Filtro de label inline: +0,48 de recall e ~20× QPS sobre o post-filter
description: Empurrar o filtro para dentro da travessia do IVF resolve a fome do post-filter em seletividade de ~1%; a investigação corrigiu a arquitetura de Custom Scan para scan-key.
resource: git:f7c7b93:docs/adr/0040-m90-inline-label-filter-verdict.md
tags: [adr, filtered-ann, inline-filter, scan-key, recall, m90]
adr_id: "0040"
adr_status: Accepted
decision_date: 2026-07-12
milestone: M90
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0040
    resource: git:f7c7b93:docs/adr/0040-m90-inline-label-filter-verdict.md
    title: ADR-0040 — M90 inline label filter
    last_modified: 2026-07-12
---

Um dos poucos vereditos francamente positivos da linhagem vetorial — e outro caso em que a
investigação **corrigiu a arquitetura** antes de qualquer código.

# Veredito medido: GO

A 500k, com ~1% de seletividade, 32 probes, k=10 e 100 queries
([m90](/benchmarks/m90-inline-filter.md)):

| Abordagem | recall@10 | QPS |
|---|---|---|
| **inline** | **1,0000** | **208,8** |
| post-filter | 0,5180 | 10,5 |

Um delta de **+0,4820 de recall e ~20× de QPS**.

O mecanismo explica o tamanho do ganho: a ~1% de seletividade — o regime em que o post-filter passa
fome, porque quase todos os candidatos do pool de rerank falham o filtro —, o inline **pula os
não-correspondentes antes de custarem um slot**. O pool de rerank enche de candidatos que **casam**,
o recall fica completo, e o re-search iterativo caro deixa de ser necessário.

# Como — código próprio

1. **Opclass:** o AM passa a declarar suporte multi-coluna, com uma opclass default sobre
   `smallint[]` e um operador de sobreposição próprio, de modo que o planner empurra o filtro como
   Index Cond.
2. **Formato:** o blob de códigos por lista passa a ser `[ids][labels_fixed][codes]`, com 8 slots de
   label e uma contagem por vetor, **co-localizados** — assim o primeiro estágio lê o label sem
   random-read extra. O writer reusa o flush por lista do
   [ADR 0039](/decisions/0039-m89-ambuild-streaming-verdict.md), e a contabilidade de páginas é
   idêntica à anterior.
3. **Scan:** o `amrescan` interpreta a chave de scan, extrai o conjunto de labels da query, e o
   primeiro estágio pula os que não sobrepõem antes do rerank.

# O desvio de escopo — a investigação corrigiu o milestone

O texto original do milestone dizia **Custom Scan Provider**. A investigação, lendo o código real do
[pgvectorscale](/technologies/pgvectorscale.md), mostrou que ele usa **scan-key** — risco muito menor
e suficiente para o critério, que pedia um filtro de label seletivo. O Custom Scan, necessário para
`WHERE` arbitrário, foi movido para o milestone seguinte.

# A fronteira honesta — o que este trabalho NÃO faz

Cobre **apenas** a coluna de label declarada com o operador de sobreposição sobre `smallint[]`. Um
`WHERE price < 100` numa coluna heap comum **ainda post-filtra** — o inline para `WHERE` arbitrário
(o Custom Scan Provider, que é a abordagem do AlloyDB) é o milestone seguinte da linhagem, medido em
[m91 — filtro adaptativo](/benchmarks/m91-adaptive-filter.md).

Usar labels exige bump de formato e REINDEX; índices sem label ficam inalterados.

**Não é claim de QPS superior** ao ScaNN ou ao AlloyDB — o teto de paradigma permanece. É claim de
**recall estável sob filtro de label seletivo**, com um bônus grande de QPS, medido.

Dados sintéticos com clusters bem separados, e a comparação inline contra post é sobre os **mesmos
dados**, o que a torna válida independentemente; run único.[^adr0040]

# Alternativas consideradas

**Custom Scan Provider agora** — YAGNI para o critério, com máquina pesada de planner e executor.
**Manter só o post-filter** — medido em recall 0,52 no regime seletivo. **Labels de comprimento
variável** — adiado: 8 slots fixos cobrem a maioria dos filtros de tag e categoria.

[^adr0040]: ADR-0040 — M90: inline label filter, veredito MEDIDO GO
