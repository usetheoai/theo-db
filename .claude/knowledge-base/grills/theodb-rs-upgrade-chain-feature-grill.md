---
slug: theodb-rs-upgrade-chain
milestone_id: M137
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M137 Cadeia de upgrade do `theodb_rs` (`ALTER EXTENSION UPDATE`)

> **Nota de método (honestidade):** as 4 perguntas do grill não foram feitas numa entrevista separada — as
> respostas vêm da análise de fundação conduzida com o owner em 2026-07-21 (peers neon/paradedb/pg_durable, inventário
> medido do repo, e as decisões que ele tomou ao longo dela). Registro a diferença em vez de simular uma
> entrevista que não houve.

**Q1 — O que é e por que AGORA?** Medido em 2026-07-21: `theodb_rs` expõe **94 funções `pg_extern`** e tem
**zero** scripts de upgrade, travado em `default_version = '1.0.0'` através de 120 releases (v0.120.0). Quem
instalou **não consegue** `ALTER EXTENSION theodb_rs UPDATE` — teria de dropar e recriar, perdendo objetos
dependentes. (A extensão umbrella `theodb` tem cadeia 1.0→1.4 consistente; o problema é só na extensão Rust.)
**Por que agora:** é a classe de defeito **irrecuperável para quem já instalou** — quanto mais releases sem
cadeia, maior o buraco. Um banco que não se atualiza não é um banco.

**Q2 — Dependências.** M135 `[x]` (a migração PG18 acabou de mudar a superfície e é a base sobre a qual a
cadeia começa).

**Q3 — Decisões do owner.** Priorizado como desqualificador junto com o CI (2026-07-21).

**Q4 — Riscos NOVOS.** (a) Não existe registro de qual era a superfície SQL em cada release passado — a
cadeia terá de partir de um baseline declarado (provável `1.0.0` = superfície atual), aceitando que instalações
antigas não têm caminho retroativo; isso precisa ser dito em voz alta, não escondido. (b) O pgrx regenera o SQL
de instalação inteiro a cada build, então é fácil o script de upgrade divergir do install — exige o gate que
compara os dois (padrão SchemaBot do paradedb).
