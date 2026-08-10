---
type: Measurement
title: m56 fase 2 — reuso de slot sob churn: a regressão de recall e seu conserto
description: O reuso consumia os tombstones antes do limiar, então o fold que repara o grafo nunca disparava e o recall caía de 0,95 para 0,57 — a história é contada em três medições.
resource: git:f7c7b93:docs/benchmarks/m56-slot-reuse-churn.md
tags: [benchmark, churn, slot-reuse, recall, regressao, m56]
milestone: M56
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m56slot
    resource: git:f7c7b93:docs/benchmarks/m56-slot-reuse-churn.md
    title: M56 fase 2 — slot-reuse churn benchmark
---

Um artefato estruturado como **história em três medições** — o que o torna muito mais útil que um número
final.

# 1. A regressão original

Com o reuso de slot **ligado**, o recall media **0,57**; **desligado**, 0,95.

**A causa é uma interação não óbvia entre dois mecanismos:** o reuso de slot **consumia os tombstones
antes de o limiar ser atingido**, então **o fold — que é quem REPARA o grafo — nunca disparava**. O
recall despencava.

Nenhum dos dois mecanismos está errado isoladamente. **O defeito está no acoplamento**, e só aparece sob
churn sustentado.

# 2. O conserto, em duas partes

- **Reusar apenas slots de nível zero que não sejam o ponto de entrada** — o que dá religação limpa, sem
  herdar links obsoletos e sem corromper a entrada do grafo.
- **Disparar a compactação por *churn*** — contando tombstones **mais** slots reusados — e não apenas por
  tombstones. Assim o fold repara **mesmo sob reuso**.

A segunda parte é a correção conceitual: **o gatilho media a coisa errada**. Ele contava o sintoma que o
reuso removia, em vez do trabalho acumulado.

# 3. O resultado

Recall corrigido, com **benefício líquido marginal** — e o documento diz "marginal", não "ganho".

Ou seja: o mecanismo passa a ser **seguro**, e o ganho que ele oferece é pequeno. Ambas as informações
importam para decidir se ele fica ligado.

# Relacionado

O desenho de manutenção completo é o
[ADR 0017](/decisions/0017-m55-index-maintenance-at-scale.md), e o custo do caminho de DELETE está em
[m56 in-place](/benchmarks/m56-inplace-maintenance.md).
