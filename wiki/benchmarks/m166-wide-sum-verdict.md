---
type: Measurement
title: m166 — soma larga com expressão: roteada apenas na classe provadamente livre de overflow
description: Noventa agregados sobre expressões passam a rotear, mas só onde é demonstrável que não há overflow — a garantia que preserva a identidade byte a byte.
resource: git:f7c7b93:docs/benchmarks/m166-wide-sum-verdict.md
tags: [benchmark, columnar, overflow, prova, cobertura, m166]
milestone: M166
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m166ws
    resource: git:f7c7b93:docs/benchmarks/m166-wide-sum-verdict.md
    title: M166 — wide-SUM verdict
---

# O caso

Uma query com **90 agregados**, cada um somando uma **expressão** em vez de uma coluna nua. Ela era
recusada porque o classificador de argumentos aceitava **apenas colunas nuas**.

# A garantia que torna o roteamento seguro

A forma é admitida **apenas numa classe provadamente livre de overflow** — coluna base estreita com
resultado de operador mais largo.

**Isso é o ponto.** Somar uma expressão num motor diferente do PostgreSQL levanta uma questão que somar
uma coluna não levanta: **se os tipos intermediários diferirem, o overflow acontece em pontos
diferentes**, e o resultado diverge — legitimamente, dos dois lados.

**A solução não é comparar e torcer: é restringir a admissão à classe onde o overflow é impossível.** Aí
a identidade byte a byte deixa de depender dos dados e passa a ser propriedade dos tipos.

É o mesmo raciocínio de [m154](/benchmarks/m154-count-distinct.md), que recusou aproximação porque ela
quebraria a garantia, e de [m153](/benchmarks/m153-groupby-text.md), que exigiu collation determinística
antes de aceitar agrupamento por texto.

**O padrão que emerge da série toda: ampliar cobertura sempre custa uma prova, nunca uma tolerância.**
