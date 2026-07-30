---
type: Failure Mode
title: Congelar uma crença intermediária como se fosse conclusão
description: Minerar transcripts (deliberação em andamento) sem cruzar com memória consolidada e artefato produz conceitos que parecem verificados e registram o que se acreditava no meio do caminho.
tags: [conhecimento, fonte, honestidade, okf]
timestamp: 2026-07-30T00:00:00Z
---

# Congelar uma crença intermediária como se fosse conclusão

## A assinatura

Um registro que **parece verificado** — tem número, tem fonte citada, tem tom de veredito — e afirma o que se
acreditava **no meio** de uma investigação, não o que ela concluiu.

## O caso que gerou este conceito

O commit `5c38eee` minerou 562 MB de **transcripts** de um diretório irmão para escrever 7 conceitos deste
bundle. O review adversarial (5 agentes, 2026-07-30) encontrou **4 BLOCKER e 4 HIGH**, e a concentração não foi
acidental:

| Conceito | A crença congelada | O que a medição posterior disse |
|---|---|---|
| SBQ | "só falta medir sob pressão de RAM" | a pressão **foi** medida — 0,73× / 0,77×, e o mecanismo explica por quê |
| pg_duckdb | faixa `0,52-0,78×` | **nenhum artefato** tem esses números; o medido é 0,63-0,89× |
| levers do HNSW | "3 refutados" | **7** refutados, e dois com conclusão estrutural |
| saturação de recall | "satura em 0,974 a 500k" | **superado** pelo ADR-0034 → 0,990 |

## Por que a fonte importa mais do que parece

| Fonte | O que ela é | O que ela registra |
|---|---|---|
| **transcript** | deliberação **em andamento** | hipóteses, tentativas, o que eu achava às 14h |
| **memória consolidada** | destilação **depois** do ciclo | o que sobreviveu |
| **artefato** (`docs/benchmarks/`, ADR) | evidência **datada e versionada** | o que foi medido |

Um transcript contém, em ordem cronológica, **a crença errada e a correção dela**. Minerar por palavra-chave sem
ler até o fim colhe a primeira e perde a segunda — e o resultado herda o tom de convicção da primeira.

## Como evitar

**Toda afirmação extraída de transcript é hipótese até ser cruzada com artefato ou memória consolidada.** Na
prática: ao minerar, para cada achado, uma busca no corpus consolidado pela mesma entidade antes de escrever. Se
o consolidado disser outra coisa, ele vence — ele é posterior por construção.

E quando um conceito citar um ADR ou benchmark, **abrir o arquivo e confirmar que ele diz aquilo**. Dois dos
quatro BLOCKER eram conceitos que citavam corretamente uma fonte cuja conclusão era o **oposto** do que o
conceito afirmava.

## Relacionados

- [failure-mode/diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md) — a classe-mãe
- [technique/nenhuma-alegacao-sem-medicao](../techniques/nenhuma-alegacao-sem-medicao.md)
- [honest-negative/sbq-nao-ganha-qps-em-regime-algum](../honest-negatives/sbq-nao-ganha-qps-em-regime-algum.md)
