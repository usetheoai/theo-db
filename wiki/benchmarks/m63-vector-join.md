---
type: Measurement
title: m63 — vector JOIN por LATERAL: o gate é estrutural, não de latência
description: O critério é provado por EXPLAIN — o ramo interno usa o índice, não é nested-loop quadrático — e a latência é reportada sem ser gate.
resource: git:f7c7b93:docs/benchmarks/m63-vector-join.md
tags: [benchmark, vector-join, lateral, explain, gate-estrutural, m63]
milestone: M63
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m63
    resource: git:f7c7b93:docs/benchmarks/m63-vector-join.md
    title: M63 — Vector JOIN via LATERAL-index-scan
    last_modified: 2026-07-09
---

**Veredito estrutural: cumprido e PROVADO por `EXPLAIN`** dentro de um teste — o similarity join usa o
**índice ANN no ramo interno**, e **não** é o nested-loop O(n·m).

# Por que o gate é estrutural

O critério deste milestone é **"usa o índice, não é quadrático"** — uma propriedade **de plano**, que se
prova lendo o plano.

**A paridade de latência NÃO é o gate**, e o documento diz isso explicitamente. Os números de latência
são reportados, mas não decidem.

Escolher um gate que o instrumento prova de forma **binária e determinística** — o plano contém um Index
Scan ordenado, ou não contém — é mais forte que um gate de latência numa máquina ruidosa. **Um `EXPLAIN`
não tem variância.**

# O que também é medido

Recall preservado contra ground truth exato — computado por força bruta quadrática por linha, que é o
oráculo caro mas correto —, com verificação por mínimo e por média entre linhas. E três braços,
incluindo o caso de deduplicação ponta a ponta.

# A consequência

**Zero código de produção novo.** O idioma já existente **é** o join, e construir um nó de execução
customizado seria complexidade acidental — a rejeição está no
[ADR 0022](/decisions/0022-m63-vector-join-lateral-not-node.md), junto com a rejeição de um helper que
**poderia ser mais lento que a coisa que embrulha**.

# Débito registrado

O padrão faz N buscas independentes, **sem compartilhar trabalho entre linhas externas próximas** — o gap
de throughput conhecido. Fica como semente, só com evidência.
