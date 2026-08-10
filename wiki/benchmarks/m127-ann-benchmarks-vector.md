---
type: Measurement
title: m127 — piloto do benchmark oficial no pilar vetorial
description: Prova o padrão adotar-e-envolver com um adaptador ao protocolo oficial mais a camada própria de significância e regressão, que a ferramenta oficial não tem.
resource: git:f7c7b93:docs/benchmarks/m127-ann-benchmarks-vector.md
tags: [benchmark, oficial, ann-benchmarks, adopt-and-wrap, piloto, m127]
dataset: GloVe
milestone: M127
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m127
    resource: git:f7c7b93:docs/benchmarks/m127-ann-benchmarks-vector.md
    title: M127 — Official benchmark VECTOR pilot
    last_modified: 2026-07-20
---

A primeira aplicação do padrão decidido no
[ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md) — **a fatia vertical que estabelece o
padrão** antes de replicá-lo aos outros pilares.

**Veredito:** o adaptador ao protocolo oficial dirige uma fronteira real de recall × QPS através do
índice próprio, e a **camada própria retida — regressão byte-idêntica mais significância pareada — que a
ferramenta oficial NÃO tem — funciona ponta a ponta.**

# A ressalva que impede o número de circular errado

> **Não é o hardware canônico do leaderboard**, então **o QPS não é comparável ao leaderboard**.

Isso é declarado no topo. É a diferença entre "rodamos o benchmark oficial" e "temos um resultado
oficial" — e confundir as duas coisas é precisamente o risco de adotar uma ferramenta de leaderboard.

O dataset escolhido é o de licença compatível, conforme as guardas registradas no ADR.

# O que o padrão entrega

**Comparabilidade externa** vem do driver, do dataset e do protocolo oficiais. **Rigor** vem da camada
própria: significância pareada e regressão de resultado — capacidades que **nenhuma** das ferramentas de
mercado oferece, conforme o levantamento que fundamentou a decisão.

**Adotar sem envolver perderia rigor; envolver sem adotar perderia comparabilidade.** O piloto prova que
dá para ter os dois.

# Replicações

O padrão foi aplicado depois aos pilares [colunar](/benchmarks/m128-clickbench-columnar.md),
[OLTP](/benchmarks/m129-oltp.md) e [HTAP](/benchmarks/m130-htap.md).
