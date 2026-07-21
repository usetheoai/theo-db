---
slug: engine-lexical-propria
milestone_id: M140
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M140 Engine lexical própria sobre Tantivy + crate núcleo sem pgrx *(gated M139)*

> **Nota de método (honestidade):** as 4 perguntas do grill não foram feitas numa entrevista separada — as
> respostas vêm da análise de fundação conduzida com o owner em 2026-07-21 (peers neon/paradedb/pg_durable, inventário
> medido do repo, e as decisões que ele tomou ao longo dela). Registro a diferença em vez de simular uma
> entrevista que não houve.

**Q1 — O que é e por que AGORA?** Assumir a perna lexical em vez de alugá-la. O North Star foi reposicionado
(ADR-0033/0035) porque superioridade de QPS vetorial sobre o ScaNN foi **medida como não-alcançável** por
extensão permissiva; o lugar onde podemos ser genuinamente superiores é a **superfície AI-native híbrida**, e
isso exige as duas pernas nossas. Inclui a extração do **crate núcleo sem pgrx** — não por estética: uma
integração Tantivy tem superfície pura grande (parser de query, scoring, tokenizers), e o ParadeDB provou o
padrão (o único crate deles sem pgrx é `tokenizers`). Hoje **98%** do nosso código está atrás do pgrx e **54
testes** não rodam porque `cargo pgrx test` não linka na droplet.

**Q2 — Dependências.** M139 `[ ]` (o gate — se o spike falhar, este milestone não abre) e M138 `[ ]` (a linha de
base que precisamos bater).

**Q3 — Decisões do owner.** "Vamos usar o Tantivy assim como o ParadeDB utiliza" (2026-07-21).

**Q4 — Riscos NOVOS.** (a) **Supersede uma decisão registrada**: o roadmap mantinha `pg_textsearch` como exceção
permissiva *"por não haver peça own-code permissiva que resolva"* — o Tantivy (MIT) muda essa premissa, mas a
reversão precisa de ADR explícito, não pode ser silenciosa. (b) A extração do crate núcleo colide com o
**ADR-0009** (`theodb-rs-api-surface-single-module`), que escolheu módulo único deliberadamente — exige ADR que
reconcilie, ou lê como reversão não registrada.
