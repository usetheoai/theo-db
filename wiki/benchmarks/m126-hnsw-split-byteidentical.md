---
type: Measurement
title: m126 — divisão de um god-file provada byte-idêntica, por duas vias
description: Identidade estática do texto dos módulos mais um A/B sobre o MESMO índice físico — o refactor é provado, não argumentado.
resource: git:f7c7b93:docs/benchmarks/m126-hnsw-split-byteidentical.md
tags: [benchmark, refactor, byte-identico, prova, code-quality, m126]
milestone: M126
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m126
    resource: git:f7c7b93:docs/benchmarks/m126-hnsw-split-byteidentical.md
    title: M126 — Split of hnsw_page.rs proven byte-identical
    last_modified: 2026-07-20
---

**Veredito:** um arquivo de 3.456 linhas foi dividido em módulos **com zero mudança de comportamento,
formato ou API**.

# As duas provas, e por que uma só não bastaria

**1. Identidade estática do texto.** O diff de produção inteiro contra o original é de **6 linhas** — uma
diretiva por módulo. Portanto as funções de codificação, empacotamento, escrita e travessia são
**textualmente idênticas**.

Isso cobre os caminhos de **escrita, build, VACUUM e formato** — que são difíceis de exercitar em runtime
e onde um erro seria caro.

**2. A/B sobre o MESMO índice físico.** Os binários pré e pós refactor leem **o mesmo índice em disco** e
retornam rankings **byte-idênticos**.

Isso corrobora em runtime o caminho quente de **leitura**.

**Nenhuma das duas sozinha é suficiente.** A identidade textual não prova que o wiring dos módulos está
certo; o A/B de leitura não toca o caminho de escrita. **Juntas, elas cobrem o que cada uma deixa de
fora** — e ambas foram confirmadas por revisão adversarial independente.

# Por que isso importa como padrão

Refactors "que não mudam nada" são onde defeitos silenciosos entram. **Exigir prova, e escolher provas
que se complementam em cobertura**, é o que permite dividir arquivos grandes sem transformar higiene de
código em risco.

É o mesmo padrão do [m25](/benchmarks/m25-craft-hardening.md), que provou paridade de um refactor por
rebuild mais suíte verde mais revisor de paridade — e do
[ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md), que registrou a divergência quando o
resultado não bateu o critério literal.
