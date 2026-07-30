---
type: Honest Negative
title: Superioridade de QPS vetorial sobre ScaNN/AlloyDB é NÃO-ALCANÇÁVEL por extensão PG permissiva
description: Veredito medido do M73: o gap de 25-44× a recall 0.99 é de paradigma (AH-LUT anisotrópico + não pagar o imposto MVCC/WAL), não de otimização.
resource: docs/adr/0035
tags: [vetorial, north-star, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# Superioridade de QPS vetorial sobre ScaNN/AlloyDB é **não-alcançável** por extensão PG permissiva

## O veredito medido (M73, 2026-07-10)

| Alcançado | Não alcançado |
|---|---|
| **paridade own-code de recall** classe-pgvector (M60/M69/M70) | superioridade de QPS sobre ScaNN |
| throughput multi-cliente **competitivo-a-superior** vs pgvector no regime 128d clusterizado (M72: +11% QPS a recall casado) | — |
| memória billion-scale | — |

O gap de **25-44× a recall 0.99** é de **paradigma**: AH-LUT anisotrópico + não pagar o imposto de MVCC/WAL. Não
é distância de otimização.

E o RaBitQ — o melhor quantizador permissivo disponível — dá **memória, não QPS** (M74 / ADR-0036).

## O que isso permite e proíbe dizer

- **Permitido:** "paridade de recall + memória billion-scale + AI-native / HTAP / aberto".
- **Proibido:** "mais rápido que o AlloyDB no vetor".

## A causa-raiz é um problema de PESQUISA, e três levers já foram refutados

O gap não é de engenharia de execução — é **qualidade do grafo HNSW**, que **degrada com escala**: o grafo do
theodb **satura em recall 0,974 a 500k**.

Três levers foram tentados e **refutados por medição**:

| Lever | Resultado |
|---|---|
| `ef_construction=200` | **PIORA** — recall vai a 0,832. Revertido |
| MERGE de back-links | refutado |
| `HNSW_M` | refutado |

E a hipótese de causa-raiz mais óbvia caiu ao ler o código: **o theodb já tem a poda por diversidade**. As
suspeitas restantes são finas — distribuição de níveis (`ml`), entry-point da descida greedy, ou a heurística de
seleção de vizinhos.

**Consequência de planejamento:** isso é risco **ALTO** e não se resolve com uma sprint de otimização. Qualquer
milestone que dependa de fechar esse gap precisa declarar que depende de um problema de pesquisa em aberto — não
de trabalho de implementação.

## Por que registrar como negativo honesto

Sem este registro, a pergunta volta a cada planejamento, e cada volta custa uma rodada de discover. O veredito
tem artefato (`docs/benchmarks/m73-headtohead-verdict.md`, ADR-0035) e reposicionamento formal proposto no
ADR-0033 — pendente de assinatura do owner, porque o mandato LOCKED do ADR-0002 permanece até lá.
