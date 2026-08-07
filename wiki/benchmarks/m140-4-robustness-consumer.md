---
type: Measurement
title: m140.4 — robustez provada no binário embarcado, e a disciplina travada por construção
description: Crash, VACUUM e MVCC verificados contra o binário que ships; e a regra de thread-safety deixa de depender de revisão porque o compilador passa a impô-la.
resource: git:f7c7b93:docs/benchmarks/m140-4-robustness-consumer.md
tags: [benchmark, robustez, crash, mvcc, thread-safety, binario-embarcado, m140]
milestone: M140.4
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m1404
    resource: git:f7c7b93:docs/benchmarks/m140-4-robustness-consumer.md
    title: M140.4 — robustez + consumidor
    last_modified: 2026-07-22
---

**Manchete:** a engine é **robusta** — crash, VACUUM e MVCC provados **no binário embarcado** —, tem a
disciplina de thread-safety **travada por construção**, e é **consumida** por uma aplicação real, com
prova ponta a ponta e wiring testada.

# Por que contra o binário embarcado

A validação roda contra a extensão **instalada**, e não pela suíte de teste da linguagem — que não linka
neste ambiente.

É o padrão que o repositório repete: **suíte verde não é suficiente; a robustez vale provada no binário
que efetivamente ships.** Um teste que roda num ambiente que o usuário nunca terá prova menos do que
parece.

# A disciplina travada por construção

Este é o ponto mais forte do artefato. O storage que vive no caminho das threads foi colocado num crate
**que não linka o ferramental do banco** — de modo que é **impossível, por construção**, tocar o banco de
uma worker thread.

**Uma regressão teria de mover código para fora do núcleo**, o que é pego por um gate objetivo de
dependências.

**A convenção deixou de depender de alguém lembrar dela numa revisão.** É a diferença entre uma regra
documentada e uma regra que o compilador impõe — e a razão de ela existir está em
[ADR 0051](/decisions/0051-m139-tantivy-pg-page-directory-design.md): foi **medido** que o motor chama o
storage de quatro threads distintas.

# O boundary do consumidor

Prova ponta a ponta e wiring testada, **com o cutover de produção declarado fora deste escopo**.

**Não se reivindica "consumidor em produção"** antes do uso sustentado — a disciplina de dogfood que o
projeto aplica a qualquer alegação de produção. Os detalhes estão no
[ADR 0055](/decisions/0055-m140-4-lexical-robustness-consumer.md).
