---
type: Measurement
title: m89 — build com memória limitada: de 4,21× para 1,28× da base
description: Fecha o muro de memória do build sem mudar o formato on-disk e sem FFI — e o desvio do plano é justificado por medição, não por preferência.
resource: git:f7c7b93:docs/benchmarks/m89-ambuild-streaming.md
tags: [benchmark, build, memoria, streaming, parsimony, m89]
milestone: M89
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m89
    resource: git:f7c7b93:docs/benchmarks/m89-ambuild-streaming.md
    title: M89 — ambuild streaming
    last_modified: 2026-07-12
---

**Veredito: critério atingido.** Fecha o muro descoberto em [m88](/benchmarks/m88-billion-scale-verdict.md).

# O resultado

A 30M vetores numa máquina de 62 GB usáveis:

| Build | pico | razão sobre a base |
|---|---|---|
| anterior | 64,7 GB | 4,21× → **OOM** |
| **precisão plena, novo** | 19,7 GB | **1,28×** |
| **comprimido, novo** | 23,1 GB | **1,50×** |

E duas propriedades que tornam a mudança segura de adotar: **zero mudança de formato on-disk** — sem bump
de versão e **sem REINDEX** — e **zero regressão**, com toda a suíte verde.

# Os dois incrementos

**Eliminar clone**, movendo o corpus para dentro do índice em vez de copiá-lo; e **escrever páginas em
streaming**, recebendo os dados por referência e liberando cada lista após escrevê-la.

# O desvio do plano, justificado por medição

O plano escolhera uma rota via FFI para uma estrutura interna do PostgreSQL. **A implementação não a
usou** — e a justificativa é medida, não preferida:

O primeiro incremento, **medido isolado, ainda estourava a 4,21×** — as cópias dominantes estavam nos
buffers dos writers, não no clone do build. O segundo incremento **atinge o critério com risco muito
menor: zero FFI**.

**Medir o primeiro incremento isoladamente** é o que revelou onde a memória realmente estava, e o que
transformou o desvio numa decisão informada em vez de um atalho.

# Limite honesto

**Não é memória constante.** O pico ainda carrega uma cópia do corpus, então **100M ainda não cabe em
RAM commodity** — a rota via FFI segue sendo o follow-up honesto para essa escala. **Este milestone
entrega 30M, não escala de bilhão.**
