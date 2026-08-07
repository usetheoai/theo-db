---
type: Measurement
title: m90 — filtro inline: recall 1,00 contra 0,52 do post-filter
description: A ~1% de seletividade, empurrar o filtro para dentro da travessia recupera o recall inteiro e ainda multiplica o QPS por 20 — o regime onde o post-filter passa fome.
resource: git:f7c7b93:docs/benchmarks/m90-inline-filter.md
tags: [benchmark, filtered-ann, inline, seletividade, m90]
milestone: M90
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m90
    resource: git:f7c7b93:docs/benchmarks/m90-inline-filter.md
    title: M90 — inline label filter
    last_modified: 2026-07-12
---

**Veredito: GO, inline supera post — decisivamente.**

# O gate, formulado com precisão

A pergunta não é "o inline é melhor?", e sim: **o inline bate o post-filter na seletividade em que o
post-filter degrada?**

**Escolher o regime de teste onde a alternativa falha** é o que torna a comparação informativa. Testar
onde ambos funcionam não decidiria nada.

# Resultado

A 500k, com ~1% de seletividade de label, 32 probes, k=10, 100 queries:

| Estratégia | recall@10 | QPS |
|---|---|---|
| **inline** | **1,0000** | **208,8** |
| post-filter | 0,5180 | 10,5 |

**+0,48 de recall e ~20× de QPS.**

# O mecanismo

A ~1% de seletividade, **quase todos os candidatos do pool de rerank falham o filtro** — o post-filter
passa fome. O inline **pula os não-correspondentes antes de custarem um slot**, então o pool enche de
candidatos que casam, o recall fica completo, e o re-search iterativo caro deixa de ser necessário.

**O ganho de QPS é consequência do ganho de recall**, não um eixo independente: não precisar refazer a
busca é o que economiza o tempo.

# Fronteira declarada

Cobre **apenas** a coluna de label declarada com o operador de sobreposição. `WHERE` arbitrário sobre
coluna comum **ainda post-filtra**. Exige bump de formato e REINDEX para usar labels.

**Não é claim de QPS superior** ao adversário externo — o teto de paradigma permanece. É claim de
**recall estável sob filtro seletivo**.

Dados sintéticos com clusters bem separados; a comparação inline contra post é sobre os **mesmos dados**,
o que a torna válida independentemente; run único.

# Contexto

A decisão é o [ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md), e a investigação **corrigiu
a arquitetura** antes do código: o escopo original previa um mecanismo muito mais pesado, e a leitura da
implementação de referência mostrou um caminho de risco bem menor.
