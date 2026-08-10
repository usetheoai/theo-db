---
type: Measurement
title: gap1 — extendCandidates fecha a degradação de recall por escala
description: Sete tentativas black-box falharam sem apontar a causa; a virada para análise estrutural do grafo localizou o problema e o fix subiu o recall em ~5 pontos.
resource: git:f7c7b93:docs/benchmarks/gap1-extend-candidates.md
tags: [benchmark, recall, hnsw, white-box, metodo, navegabilidade]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: gap1
    resource: git:f7c7b93:docs/benchmarks/gap1-extend-candidates.md
    title: Gap 1 — extendCandidates no build
    last_modified: 2026-07-10
---

**Recall de 0,974 para 0,990** — fecha a degradação por escala — **e é honesto sobre a fronteira**.

# O método que funcionou, depois de sete que não

> **Sete alavancas black-box — reconstruir e medir — foram refutadas sem apontar a causa.** Trocamos para
> **white-box**: um analisador da anatomia do grafo.

Esta é a lição de método mais valiosa da linhagem vetorial. Sete iterações de "mudar um parâmetro e medir
o recall" produziram sete negativos **e nenhuma explicação** — porque medir a saída não revela **onde** o
processo falha.

O analisador estrutural respondeu de imediato: **conectividade perfeita, mas 100% das misses são de
ROTEAMENTO**, com a distância em saltos crescendo com a escala.

**Um grafo bem conectado e mal navegável** — diagnóstico que nenhuma varredura de parâmetro daria, e que
aponta direto para o mecanismo do paper.

# A honestidade sobre o que o fix NÃO faz

O ganho é de **teto de recall**, e **não de fronteira**: a referência ainda tem recall maior **no mesmo
`ef`**, o que significa que, a recall casado, o índice próprio continua mais lento.

**Subir o teto e igualar a eficiência são coisas diferentes**, e confundi-las transformaria um ganho real
em claim falso. O
[ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md) registra as duas metades.

# O custo

Build 2 a 3× mais lento — mitigado por opt-out, com o default ligado porque qualidade de recall importa
mais que velocidade de build na maioria dos workloads vetoriais.

# Contexto

Resolve o que [m60](/benchmarks/m60-hnsw-recall.md) diagnosticara e o
[bloqueador-raiz](/benchmarks/p0-vector-superiority-root-blocker.md) consolidara.
