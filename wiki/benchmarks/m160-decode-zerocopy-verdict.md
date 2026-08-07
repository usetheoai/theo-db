---
type: Measurement
title: m160 — decode sem cópia para colunas de largura fixa
description: Elimina a tempestade de alocações por célula que um flamegraph identificou, mantendo o caminho antigo como fail-safe para os tipos que ele não cobre.
resource: git:f7c7b93:docs/benchmarks/m160-decode-zerocopy-verdict.md
tags: [benchmark, columnar, decode, arrow, fail-safe, m160]
milestone: M160
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m160
    resource: git:f7c7b93:docs/benchmarks/m160-decode-zerocopy-verdict.md
    title: M160 — zero-copy fixed-width decode
    last_modified: 2026-07-27
---

# O que muda

Colunas **não-nulas de largura fixa** passam a ser decodificadas como **um buffer contíguo**, construindo
o array Arrow com **uma alocação tipada por coluna** — em vez de uma alocação **por célula** mais uma
releitura.

Isso elimina a **tempestade de alocações por célula** que um flamegraph pós-medição identificou como o
gargalo da classe coberta — o mesmo tipo de custo que o
[profile anterior](/benchmarks/m148-flamegraph-scan.md) apontara.

# Os dois cuidados de desenho

**Fail-safe:** colunas anuláveis, de comprimento variável, de texto ou booleanas, e linhas pendentes da
mesma transação, **mantêm o caminho antigo**. O caminho rápido cobre o que ele sabe cobrir, e o resto
continua correto.

**A GUC existe para medir, não só para desligar:** o documento diz que o toggle existe **para que o ganho
possa ser medido no mesmo binário**.

Isso resolve o problema que [m46](/benchmarks/m46-highrecall-qps.md) sofreu — comparar binários
diferentes em janelas diferentes é onde a deriva da máquina invalida a atribuição. **Um toggle no mesmo
binário torna o A/B limpo por construção.**

# Nota honesta registrada

O documento anota que uma tentativa anterior de carga foi **corrompida por uma corrida entre processos
concorrentes** sobre a tabela compartilhada, e que a carga foi refeita de forma controlada.

**Registrar a corrida em vez de silenciosamente repetir o experimento** é o que permite a quem reproduzir
evitar o mesmo problema.
