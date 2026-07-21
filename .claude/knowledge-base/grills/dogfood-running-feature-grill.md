---
slug: dogfood-running
milestone_id: M141
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M141 Dogfood `running`: theo-data em produção sobre TheoDB self-hosted *(continuação do M124)*

> **Nota de método (honestidade):** as 4 perguntas do grill não foram feitas numa entrevista separada — as
> respostas vêm da análise de fundação conduzida com o owner em 2026-07-21 (peers neon/paradedb/pg_durable, inventário
> medido do repo, e as decisões que ele tomou ao longo dela). Registro a diferença em vez de simular uma
> entrevista que não houve.

**Q1 — O que é e por que AGORA?** O anchor de dogfood está em **`wired`**, não `running`. Pela nossa própria
`dogfood-golden-rule.md`, `running` é o único valor que satisfaz o hard cap — ou seja, **hoje não podemos
reivindicar production-ready**, por mais benchmark que acumulemos (120 artefatos). Um banco de dados de verdade
é aquele que seus criadores usam em produção. **Por que agora:** o M132 destravou o worker do vectorizer no
self-host e o M138 corrige a perna lexical — as duas razões técnicas que impediam depender dele de verdade.

**Q2 — Dependências.** M124 `[x]` (entregou o `wired`; milestone concluído é imutável, então esta é a
continuação por novo milestone, conforme o contrato do `/roadmap-feature`) e M138 `[ ]` (não faz sentido
depender de uma busca que mede 0,07).

**Q3 — Decisões do owner.** "Vamos pensar em um banco de dados de verdade" (2026-07-21).

**Q4 — Riscos NOVOS.** (a) Dogfood real expõe classes de bug que benchmark não pega (operação, upgrade, backup,
observabilidade) — é o ponto, mas vai gerar trabalho não planejado. (b) O hard cap exige evidência **fresca**
(≤ 30 dias) e ≥ 2 operadores; um dogfood de uma pessoa só reproduz a síndrome do "único que sabe rodar".
