---
type: Measurement
title: m83 — spike de storage separado: medido no access method real, não in-memory
description: Testa a única alavanca que o veredito anterior nomeou, e a testa dentro do banco — aplicando explicitamente a lição de que o ganho in-memory não sobreviveu ao caminho de página.
resource: git:f7c7b93:docs/benchmarks/m83-split-storage-spike.md
tags: [benchmark, spike, storage-separation, layout, m83]
dataset: SIFT1M
milestone: M83
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m83
    resource: git:f7c7b93:docs/benchmarks/m83-split-storage-spike.md
    title: M83 — pg_scann v5 storage-separated IVF-AQ
    last_modified: 2026-07-11
---

**Veredito: GO.**

# A alavanca sob teste

A **única** que o [ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md) nomeou: **separar os
códigos dos vetores em faixas de página distintas**, de modo que o scan leia **somente os códigos
compactos** para podar, e só faça leitura aleatória dos vetores **para os sobreviventes do rerank**.

# A lição aplicada, explicitamente

> Medido **no access method real** — não in-memory — **a lição do M82: o ganho in-memory do spike
> anterior não sobreviveu ao caminho de página.**

Este é o ponto. O [m75](/benchmarks/m75-ivf-aqah-spike.md) mediu 5–7× in-memory, e isso evaporou. Repetir
o mesmo tipo de spike teria repetido o mesmo erro — e o
[dossiê de pesquisa](/references/scann-storage-separation-2026-07.md) chama isso pelo nome: **teatro de
medição**.

**O valor está no modelo de I/O, que só existe dentro do banco.**

# O rigor de same-data

Duas tabelas com dados **idênticos**, uma por layout — separado e interleaved —, com todos os demais
parâmetros casados. Só a variável em teste difere.

# Onde essa linhagem terminou

O GO daqui autorizou a construção, e a track fechou com o
[ADR 0038](/decisions/0038-m88-billion-scale-regime-verdict.md): **vantagem de tamanho confirmada em
escala (3,52×), QPS out-of-RAM direcional mas não provado**, porque o build estourava a memória antes de
chegar ao regime que provaria a tese.
