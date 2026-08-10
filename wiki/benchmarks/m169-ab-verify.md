---
type: Measurement
title: m169 — prova de identidade byte a byte a 100M
description: Prova de CORREÇÃO, separada da de conclusão; roda sem LIMIT de propósito, porque empates escolheriam linhas arbitrárias entre alternativas igualmente válidas.
resource: git:f7c7b93:docs/benchmarks/m169-ab-verify.md
tags: [benchmark, correcao, byte-identico, empates, desenho-de-experimento, m169]
milestone: M169
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m169ab
    resource: git:f7c7b93:docs/benchmarks/m169-ab-verify.md
    title: M169 — byte-identidade colunar vs heap (100M)
---

**Prova de CORREÇÃO, complementar à corrida de conclusão** — e a separação é deliberada.

**Resultado: 4 de 4 idênticas, zero divergentes, zero não verificadas.**

# As duas escolhas de desenho

**Cada lado roda UMA vez, sem medir tempo.** Porque a pergunta é correção, e repetir para reduzir
variância só faz sentido quando se mede tempo. Uma execução basta para comparar resultados.

**Sem `LIMIT`, de propósito** — e a justificativa é precisa:

> empates escolheriam linhas **arbitrárias entre válidas**.

Com `LIMIT` e chave não única, dois planos **corretos** podem devolver conjuntos diferentes na fronteira,
porque ambos satisfazem a ordenação. A comparação byte a byte acusaria uma divergência **que não é
defeito**.

**Remover o `LIMIT` elimina a ambiguidade na origem** — a mesma estratégia que
[m158](/benchmarks/m158-late-mat-verdict.md) usou ao escolher chave única, e o oposto de tratar o
problema depois com tolerância.

# Por que separar correção de conclusão

Porque são perguntas diferentes com desenhos diferentes: **conclusão** exige o teto de memória real e
mede se a query termina ([m169 t41](/benchmarks/m169-t41.md)); **correção** exige comparação sem
ambiguidade e não mede tempo.

**Um experimento que tenta responder as duas de uma vez responde mal as duas.**
