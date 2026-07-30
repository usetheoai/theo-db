---
type: Failure Mode
title: drop_caches uma vez por sweep mede a PRIMEIRA query fria e 99 quentes — o resultado é um limite inferior
description: O +21% de cold-QPS do M88 é consistente com a tese e não é uma medição limpa de crossover; o artefato diz isso, e quem cita só o número perde a ressalva.
resource: docs/adr/0038-m88-billion-scale-regime-verdict.md
tags: [benchmark, cache, metodologia, vies]
timestamp: 2026-07-30T00:00:00Z
---

# `drop_caches` uma vez por sweep mede a **primeira** query fria e 99 quentes

## O que aconteceu (M88, ADR-0038)

A tese: um índice 3,52× menor pagina menos, logo ganha QPS quando o working set excede o cache. O número medido:
**+21% de cold-QPS a probes=32** (10,2 vs 8,4).

Mas o próprio artefato qualifica:

> um **limite inferior** — a medição cold dá `drop_caches` **uma vez por sweep** → só a 1ª query é fria; as 2–100
> aquecem. Consistente com a tese, **mas não é uma medição limpa de crossover**.

O veredito registrado foi `SIZE_CONFIRMED / **OUT_OF_RAM_QPS_INCONCLUSIVE**` — e a honestidade está no segundo
termo, não no +21%.

## Por que é uma armadilha e não um detalhe

Um sweep de 100 queries com um único `drop_caches` inicial mede, na prática, **regime quente com uma amostra
fria**. O efeito que se queria medir — I/O por query no estado estacionário frio — aparece diluído por 99×. O
número resultante:

- **subestima** o efeito quando ele é real (é limite inferior);
- **não distingue** "o efeito existe e é pequeno" de "o efeito é grande e foi diluído";
- e, citado sem a ressalva, vira "medimos +21% no regime out-of-RAM" — que é uma afirmação que o experimento não
  sustenta.

## Como medir de verdade

| | |
|---|---|
| **Por query** | `drop_caches` antes de **cada** query, ou reiniciar a instância entre pontos |
| **Ou por regime** | garanta que o working set **excede** a RAM por construção (dataset ≫ RAM), tornando toda query fria por definição |
| **Ou declare** | `[NEEDS-COLD-ISOLATION]` no artefato — e no conceito — em vez de publicar o número liso |

Vale para qualquer benchmark com cache entre o teste e o disco: page cache do SO, shared_buffers, cache de
resultado, CDN. **Uma medição "fria" que aquece durante a própria corrida é uma medição morna.**

## Relacionados

- [failure-mode/instrumento-cego-a-arquitetura](instrumento-cego-a-arquitetura.md) — o outro jeito de a medição não observar o que quer
- [failure-mode/estatistica-que-nao-sustenta-a-alegacao](estatistica-que-nao-sustenta-a-alegacao.md)
- [invariant/build-pica-4x-o-dataset-base](../invariants/build-pica-4x-o-dataset-base.md) — por que o regime frio não era construível
