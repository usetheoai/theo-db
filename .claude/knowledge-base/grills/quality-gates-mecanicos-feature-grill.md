---
slug: quality-gates-mecanicos
milestone_id: M136
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M136 Gates mecânicos de qualidade + Postgres cassert no CI

> **Nota de método (honestidade):** as 4 perguntas do grill não foram feitas numa entrevista separada — as
> respostas vêm da análise de fundação conduzida com o owner em 2026-07-21 (peers neon/paradedb/pg_durable, inventário
> medido do repo, e as decisões que ele tomou ao longo dela). Registro a diferença em vez de simular uma
> entrevista que não houve.

**Q1 — O que é e por que AGORA?** Inventário medido em 2026-07-21: 28 arquivos de regra, 40 skills, 8 hooks
(gates de PROCESSO) contra **zero** gates mecânicos de Rust — sem `clippy.toml`, `rustfmt.toml`, `deny.toml`,
sem `-D warnings`, sem menção a clippy/fmt no CI, e **840–967 warnings** no build. A regra **D1** (nenhuma
dependência AGPL na distribuição) — a mais inegociável do projeto — é hoje aplicada por vigilância humana.
Os três peers pesquisados (neon, paradedb, pg_durable) rodam `-D warnings`; o neon tem `deny.toml` com
allowlist de licenças. **Por que agora:** gate de processo depende de alguém invocar; gate mecânico escala
quando o projeto cresce, que é exatamente a premissa desta rodada.

**Q2 — Dependências.** M133 `[ ]` — sem CI vivo, todo gate aqui é documentação que ninguém executa.

**Q3 — Decisões do owner.** "Vamos pensar em um banco de dados de verdade, não importa o esforço" (2026-07-21).

**Q4 — Riscos NOVOS.** (a) Ligar `-D warnings` com 840–967 warnings existentes trava o build no dia 1 — exige
decisão entre baseline-allow e mutirão de limpeza. (b) `deny.toml` pode barrar uma dependência transitiva já
embarcada, revelando um problema D1 latente — o que é o ponto, mas pode virar trabalho não previsto.
