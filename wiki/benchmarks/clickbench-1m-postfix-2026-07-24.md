---
type: Measurement
title: ClickBench a 1M depois da correção — o gate destravou
description: Re-executa o gate bloqueado, agora com amostragem sistemática que corrige o viés de pegar as primeiras linhas do arquivo.
resource: git:f7c7b93:docs/benchmarks/clickbench-1m-postfix-2026-07-24.md
tags: [benchmark, gate, amostragem, vies, correcao]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cb1m
    resource: git:f7c7b93:docs/benchmarks/clickbench-1m-postfix-2026-07-24.md
    title: ClickBench 1M pós-fix — o gate DESTRAVOU
    last_modified: 2026-07-24
---

Re-execução do [gate que bloqueara](/benchmarks/clickbench-scale-gate-2026-07-24.md), agora com o defeito
corrigido. **O gate destravou.**

# A segunda correção: o viés de amostragem

Além do defeito de código, esta execução corrige um problema **do próprio método**: a amostragem passa a
ser **sistemática — uma linha a cada 99, varrendo o arquivo inteiro** — em vez de pegar **as primeiras
linhas**.

**Pegar o início de um arquivo não é amostrar.** Dados reais têm ordem: por tempo, por origem, por como
foram coletados. As primeiras linhas de um dataset de eventos podem ser de um período específico, com
cardinalidade e distribuição atípicas — e um benchmark sobre elas mede um recorte, não o dataset.

O efeito é sutil e direcional: com um recorte enviesado, a cardinalidade de agrupamentos e a
seletividade de filtros mudam, o que **muda quais otimizações aparecem funcionando**.

# Por que isso se encaixa no padrão do repositório

É a mesma família de armadilha do [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md), em que a
geração de dados produzia vetores idênticos, e do
[m162](/benchmarks/m162-100m-gap-verdict.md), em que uma carga aparentemente completa não continha o
dataset inteiro.

**Três defeitos diferentes, todos no dado e não no código, todos capazes de invalidar a medição
silenciosamente.** É por isso que os artefatos posteriores passaram a **verificar a contagem de linhas**
em vez de confiar na conclusão do harness.
