---
type: Measurement
title: m88 — regime out-of-RAM: tamanho confirmado, QPS inconclusivo
description: O token do veredito carrega as duas metades separadas, e o milestone falha o alvo de escala por um motivo medido — dois OOM-kills — registrado como dívida.
resource: git:f7c7b93:docs/benchmarks/m88-billion-scale-verdict.md
tags: [benchmark, escala, out-of-ram, oom, veredito-parcial, m88]
milestone: M88
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m88
    resource: git:f7c7b93:docs/benchmarks/m88-billion-scale-verdict.md
    title: M88 — storage-separated out-of-RAM regime
    last_modified: 2026-07-12
---

**Veredito: `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`.**

**O próprio token do veredito carrega as duas metades** — o que impede que ele seja citado como se fosse
uma coisa só.

# A tese sob teste

Num regime em que os dados de refine **não cabem em RAM**, o layout comprimido converteria vantagem de
**memória** em vantagem de **QPS**, por ler proporcionalmente menos páginas do disco.

# O que foi medido

**Tamanho: confirmado.** O índice comprimido é **3,52× menor** a 16M, confirmando **a 16× a escala** o
achado anterior de 3,5× a 1M.

**QPS: direcional, não definitivo.** Há um sinal de +21% a frio, mas é **limite inferior**, porque a
medição limpa o cache uma vez por varredura — **só a primeira query é realmente fria**.

**Recall: não reestabelecido aqui.** Ambos medem no mesmo ponto **degenerado**, com clusters sintéticos
saturados de empates — **artefato, não prova de qualidade**.

# Por que o alvo de escala não foi atingido

**Dois OOM-kills observados**, com o pico de memória do build excedendo a máquina. **16M foi o maior que
coube.** Um índice genuinamente out-of-RAM **não foi construível**.

Registrado como **dívida técnica honesta, não como falha silenciosa** — e note a ironia útil: **a tese
não pôde ser testada porque o build, e não a query, era o gargalo**.

Isso redirecionou o trabalho para o [build em streaming](/benchmarks/m89-ambuild-streaming.md), que é a
alavanca correta — atacar a causa em vez de comprar RAM para mascarar a ineficiência.

O veredito formal é o [ADR 0038](/decisions/0038-m88-billion-scale-regime-verdict.md).
