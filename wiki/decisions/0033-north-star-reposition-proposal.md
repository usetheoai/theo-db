---
type: Decision
title: ADR 0033 — Reposicionar o north star vetorial: paridade + memória, não superioridade de QPS
description: Assinado pelo owner, emenda o ADR 0002 LOCKED — a superioridade de QPS vetorial foi medida como inalcançável por extensão PG permissiva, e a meta passa a ser paridade classe-pgvector mais eficiência de memória.
resource: git:f7c7b93:docs/adr/0033-north-star-reposition-proposal.md
tags: [adr, north-star, estrategia, reposicionamento, rabitq, honestidade]
adr_id: "0033"
adr_status: Accepted (assinado pelo owner)
decision_date: 2026-07-16
owner: human:paulohenriquevn
amends: ["0002"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0033
    resource: git:f7c7b93:docs/adr/0033-north-star-reposition-proposal.md
    title: ADR-0033 — Reposicionar o North Star vetorial
    last_modified: 2026-07-16
---

O desfecho da linha inteira de investigação vetorial: uma **emenda ao ADR LOCKED**, assinada pelo
owner, retirando a cláusula que a medição refutou.

# Contexto medido

O [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) mandava igualar **ou
superar** o [AlloyDB](/technologies/alloydb.md), buscando superioridade de QPS vetorial comprovada
por benchmark. Depois de perseguir isso por todos os caminhos honestos e medir cada um:

- **Gap 1 — TheoDB até o pgvector:** sete alavancas de navegabilidade refutadas por medição. O gap é
  fechável para **paridade**, não para superioridade.
- **Gap 2 — pgvector até o ScaNN:** o melhor quantizador permissivo do SOTA
  ([RaBitQ](/technologies/rabitq.md)) foi vendorizado e medido a 1M: **competitivo com precisão
  plena** (8,2 ms a 98,4% de recall), **não 25× mais rápido**. O ganho do RaBitQ é **memória** — 5,3
  MB residentes a 98,4% na variante em disco —, não QPS. Os 25× do [ScaNN](/technologies/scann.md)
  vêm do algoritmo dele (AH-LUT anisotrópico em 128d) somados ao fato de ele **não pagar o imposto
  do Postgres** que qualquer extensão paga.

# Decisão

A meta do pilar vetorial deixa de ser

> "superar o AlloyDB no vetor (superioridade de QPS por benchmark)"

e passa a ser

> **"Paridade vetorial classe-pgvector (recall e latência) + eficiência de memória RaBitQ para
> billion-scale em hardware barato + diferenciação por AI-native, HTAP, abertura e portabilidade."**

Consequências concretas:

1. O Gap 1 vira um milestone de **paridade**, com veredito honesto de paridade.
2. O RaBitQ vira **feature de memória e escala** — billion-scale em SSD barato, 32× de compressão,
   latência competitiva — posicionada como "escala e custo", **nunca** como "mais rápido que o
   AlloyDB".
3. O **claim público** passa a ser "capacidades classe-AlloyDB, abertas, portáveis e eficientes em
   memória", e **não** "vetorialmente superior ao AlloyDB".
4. Os head-to-heads são reenquadrados como **medições de posicionamento** — documentar onde estamos —
   e não como gates de superioridade.

# Alternativas rejeitadas

**Manter a meta de superioridade de QPS** — empiricamente inalcançável como extensão PG permissiva, e
mantê-la viraria claim desonesto. **Ir para engine standalone fora do Postgres** para fugir do
imposto — reabre decisões travadas, já que a wire-compatibility é gate de produto; é outra categoria
de produto. **A aposta ScaNN-AH do zero** — possível patente mais anos de tuning, contra um RaBitQ
permissivo já medido.

# O que NÃO muda

Measurement-first, as restrições de licença, a regra de não reinventar, o engine PostgreSQL mantido
e a honestidade. **Este reposicionamento é aplicação dessas regras à evidência, não exceção a
elas.** E a Opção α permanece — remove-se apenas a parte "**superar** no QPS vetorial", que a
medição refutou.[^adr0033]

A evidência consolidada está em
[veredito do pilar vetorial](/benchmarks/vector-pillar-verdict-2026-07.md), com os vereditos
formais nos ADRs [0035](/decisions/0035-m73-northstar-vector-verdict.md) e
[0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md). A reconciliação de governança está
no [ADR 0045](/decisions/0045-northstar-governance-reconciliation.md).

[^adr0033]: ADR-0033 — Reposicionar o North Star vetorial
