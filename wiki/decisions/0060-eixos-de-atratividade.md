---
type: Decision
title: ADR 0060 — os cinco eixos de atratividade, e o que cada um exige como prova
description: Substitui "ser melhor em todo benchmark" — que o M73 mediu como não-alcançável no pilar mais visível — por cinco eixos onde a vitória é possível, cada um com a medição que o sustenta e o número que hoje o sustenta ou não.
adr_id: "0060"
adr_status: Proposed
decision_date: 2026-08-09
tags: [adr, posicionamento, north-star, atratividade, m186, proposto]
generated: { by: claude-code/opus-5, at: 2026-08-09T21:00:00Z }
---

**Status: PROPOSTO.** A decisão de posicionamento é do owner; este documento reúne o que está medido para
que a assinatura seja informada, não para substituí-la. O [ADR 0033](/decisions/0033-north-star-reposition-proposal.md)
está proposto e sem assinatura desde 2026-07-10 — este o sucede e reduz o escopo ao que é decidível hoje.

# Contexto

O mandato LOCKED ([ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)) diz "igualar ou
superar o AlloyDB". O [M73](/benchmarks/m73-headtohead-verdict.md) mediu que **superar o ScaNN/AlloyDB no
vetorial é não-alcançável** por extensão PG permissiva: o gap de 25–44× a recall 0.99 é de paradigma —
AH-LUT anisotrópico mais não pagar o imposto MVCC/WAL —, não de esforço. O [ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md)
fechou a última alternativa permissiva.

Em 2026-08-09 o owner reformulou o alvo: **não precisamos ser melhor em todos os benchmarks, precisamos ser
atrativos.** Isso é compatível com a medição e incompatível com o texto do ADR 0002.

Sem eixos declarados, "atrativo" não é acionável: cada milestone escolhe o seu por intuição, e é assim que se
otimiza o eixo errado com rigor exemplar.

# Decisão proposta — cinco eixos, cada um com sua prova

| # | Eixo | O que promete | Prova exigida | Estado medido |
|---|---|---|---|---|
| **A1** | **Um banco só** | vetorial + lexical + colunar + grafo + lakehouse na mesma transação, sem ETL | superfície no catálogo do binário default, não em documentação | **medido** — [m184](/benchmarks/m184-pilares-superficie-medida-verdict.md) + lexical no default (M186) |
| **A2** | **Melhor que o que já se tem** | cada pilar bate o equivalente nativo do Postgres | ganho com significância **pareada** sobre o baseline nativo | **medido no lexical**: 2,08× o nDCG do `ts_rank_cd` ([m186](/benchmarks/m186-lexical-ndcg-scifact-verdict.md)). Não medido nos demais |
| **A3** | **Paridade onde não dá para vencer** | vetorial classe-pgvector, sem alegar classe-ScaNN | recall×QPS pareado vs pgvector | **medido** — paridade (M45/M60/M69/M70), +11% QPS multi-cliente (M72) |
| **A4** | **Own-code permissivo** | Apache 2.0 puro, sem dependência AGPL, sem C++ | due-diligence de licença como gate de release | **medido** — D1, e o `pg_duckdb` removido no M143 |
| **A5** | **Roda onde você está** | qualquer Postgres 18, self-host, sem control plane proprietário | instalação reproduzível verificada por terceiro | **NÃO medido** — zero uso real, âncora de dogfood em `planned` |

# O que esta proposta explicitamente NÃO promete

- **Superioridade de QPS vetorial sobre ScaNN/AlloyDB.** Medida como não-alcançável. Jamais entra em copy.
- **Vencer o ClickBench.** O colunar tem correção provada (md5 idêntico ao heap em 43 queries, [m128](/benchmarks/m128-clickbench-columnar.md)); competitividade em toda a suíte não foi medida.
- **Ganho da fusão híbrida.** O [m123](/benchmarks/m123-hybrid-significance.md) mediu não-significativo. Enquanto for esse o número, o híbrido não é eixo de atratividade — é dívida.

# Consequência para o backlog

Se assinado, este ADR reordena `B-003..B-010`: **A5 é o único eixo com estado `NÃO medido` e é o que B-010
ataca** — o que o torna prioritário sobre os eixos onde já vencemos. Os itens que não servem a nenhum eixo
devem ser mortos, incluindo os que eu mesmo registrei.

# Alternativas consideradas

- **Manter o ADR 0002 como está.** Rejeitada: o texto pede algo que a própria medição do projeto declara impossível, e manter isso vivo produz milestones que perseguem um teto conhecido.
- **Declarar "SOTA em todos os pilares".** Rejeitada pelo mesmo motivo, agravado: estenderia a promessa impossível do vetorial para pilares onde ela nem foi medida.
- **Não declarar eixo nenhum e decidir caso a caso.** Rejeitada: é o estado atual, e ele produziu 109 artefatos de benchmark e zero usuários.
