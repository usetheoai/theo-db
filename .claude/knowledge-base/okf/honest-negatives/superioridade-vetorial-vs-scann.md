---
type: Honest Negative
title: Superioridade de QPS vetorial sobre ScaNN/AlloyDB é NÃO-ALCANÇÁVEL por extensão PG permissiva
description: Veredito medido do M73: o gap de 25-44× a recall 0.99 é de paradigma (AH-LUT anisotrópico + não pagar o imposto MVCC/WAL), não de otimização.
resource: docs/adr/0035-m73-northstar-vector-verdict.md
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

## A causa-raiz, e os SETE levers já refutados

> **CORRIGIDO 2026-07-30 após review.** Esta seção dizia "três levers" e apresentava a saturação em 0,974 como
> **estado corrente** — errado nos dois pontos. São **sete**, e a saturação foi **superada pelo ADR-0034**. Pior:
> a saturação como estado corrente **contradizia a tabela deste próprio conceito**, que credita M60 como
> alcançado.

**O que a saturação era, e o que ela é hoje.** O grafo do theodb **platôava em recall 0,974 a 500k** — até o
**ADR-0034** (`extendCandidates`), que o levou a **0,990** (pgvector 0,994). O gap de recall a 500k está
**fechado**; o que permanece é **eficiência recall-por-`ef`**: o theodb precisa de **~5× o `ef`** do pgvector para
igualar recall a 500k, o que sai **~1,8× mais lento** (`gap1-extend-candidates.md:39`).

> **CORRIGIDO 2026-07-30.** Este conceito dizia "~1,8× o `ef`", citando `ADR-0035:21`. **O ADR está errado** — ele
> comprimiu *"~5× o `ef` → ~1,8× mais lento"* do artefato em *"~1,8× o `ef`"*, fundindo o multiplicador de `ef`
> com o de latência, e **cita o próprio artefato que o contradiz**. Eu fui fiel ao ADR e herdei o erro.
>
> **Corroboração independente:** o `ADR-0031:14` — outro ADR do mesmo pilar — registra o número **certo**:
> a iso-recall 0,996 a 100k, pgvector **2,13 ms (ef=100)** vs theodb **3,16 ms (ef=200)**, ou seja *"precisa ~2× o
> `ef`; ~5× a 500k"*. Dois ADRs do mesmo pilar, um correto e um comprimido — o que confirma que o defeito é do elo
> ADR-0035, não do artefato nem da medição. A classe
> está registrada em [numero-comprimido-na-cadeia-de-citacao](../failure-modes/numero-comprimido-na-cadeia-de-citacao.md).

**E o fix teve custo, que o conceito omitia:** o `extendCandidates` deixou o **build ~2-3× mais lento** (pool de
candidatos maior por insert), erodindo a vantagem de build-speed que o theodb tinha
(`gap1-extend-candidates.md:41-42`). O trade foi deliberado — recall era o eixo do North Star.

E a degradação por escala nunca foi uniforme: a **100k×768d o theodb dá recall@10 = 0,998**, em paridade ou acima
do pgvector. A fonte marca isso explicitamente como **notícia de produto** — para ≤100k vetores o pilar vetorial
está em paridade/superioridade.

**Os sete levers refutados por medição:**

| Lever | Resultado |
|---|---|
| `ef_construction` 64→200 | **PIORA** — recall 0,832 |
| MERGE de back-links | 0,846 — refutado |
| `HNSW_M` 16→32 | 0,952 — refutado |
| bissecção build sequencial vs paralelo | seq ≈ paralelo — **não é contenção, é o algoritmo base** |
| descida-beam `ef=1` | no-op |
| multi-entry `ep←W` | no-op de recall, **mas +29% QPS** — o único com efeito positivo medido |
| overwrite paralelo (7º) | seq 0,974 ≈ parallel 0,972 — a degradação é inerente ao build **nos dois modos** |

Os quatro que a versão anterior omitia são exatamente os que um planejador tenderia a propor como "ainda não
tentados". Dois deles já vêm com conclusão **estrutural** ("não é contenção paralela — é o algoritmo base"), que
é a informação de planejamento que este conceito existe para preservar.

E a hipótese de causa-raiz mais óbvia caiu ao ler o código: **o theodb já tem a poda por diversidade**. As
suspeitas restantes são finas — distribuição de níveis (`ml`), entry-point da descida greedy, ou a heurística de
seleção de vizinhos.

**Consequência de planejamento:** risco **ALTO**, e não se resolve com uma sprint de otimização. Um milestone que
dependa de fechar o gap de **QPS** depende de problema de pesquisa em aberto — não de trabalho de implementação.

## Por que registrar como negativo honesto

Sem este registro, a pergunta volta a cada planejamento, e cada volta custa uma rodada de discover. O veredito
tem artefato (`docs/benchmarks/m73-headtohead-verdict.md`, ADR-0035) e reposicionamento formal proposto no
ADR-0033 — pendente de assinatura do owner, porque o mandato LOCKED do ADR-0002 permanece até lá.
