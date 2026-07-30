---
type: Honest Negative
title: SBQ não ganha QPS em regime algum — nem in-RAM, nem sob pressão de memória
description: A tese de ≥2× foi falsificada em TODOS os regimes medidos (0,35-0,77×), e o mecanismo é conhecido — o HNSW tem localidade de acesso, então o índice f32 não thrasha sob pressão.
resource: docs/adr/0018-m57-sbq-inline-not-superior.md
tags: [vetorial, quantizacao, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# SBQ não ganha QPS em regime algum — nem in-RAM, nem sob pressão de memória

> **CORRIGIDO 2026-07-30 após review.** A versão anterior deste conceito afirmava que "a vantagem do SBQ só
> aparece sob pressão de RAM" e se chamava `sbq-nao-ganha-qps-em-regime-algum` — o slug codificava um recorte que a
> medição não sustenta. **A pressão FOI medida, e o SBQ perdeu lá também.** O conceito estava reabrindo a aposta
> que ele deveria fechar. Registro do erro em [failure-mode/crenca-intermediaria-congelada](../failure-modes/crenca-intermediaria-congelada.md).

## O veredito (M57, v0.49.0, ADR-0018)

A hipótese era **≥2×** de QPS sobre f32. Medido a 500k×768d, cosine:

| Regime | SBQ | f32 | razão |
|---|---|---|---|
| in-RAM (16 GB) | 90 | 256 | **0,35×** |
| pressão 1,8 GB | 194 | 266 | **0,73×** |
| pressão tight 1,3 GB | 218 | 284 | **0,77×** |
| in-RAM, `ef_search=1000` (recall casado 0,974) | 47,7 | 152 | **0,31×** |

O ADR-0018 conclui: *"consistentemente mais lento (0.35–0.77×) em **TODOS os regimes medidos** — in-RAM **e sob
pressão de RAM** até 1.3 GB"*. O 0,31× vem do ponto de recall casado (`m57-sbq-superiority.md:44`) — fonte
diferente do ADR, e por isso citada à parte.

## O mecanismo — e é ele que fecha a porta

A hipótese natural é *"sob pressão o índice f32 não cabe, então ele paga I/O e o SBQ ganha"*. A fonte a refuta
diretamente:

> **O HNSW tem localidade de acesso → o índice f32 não thrasha sob pressão mesmo excedendo a RAM.** A premissa
> "índice não cabe → I/O por query" NÃO vale.

Ou seja: não é que o regime certo não foi testado. É que o regime certo **não existe** para esta técnica sobre
este grafo — o gargalo que o SBQ atacaria não é exposto pelo HNSW.

## O que o SBQ de fato entrega

**Footprint de índice.** Isso é real e continua valendo para decisões de memória. O que não se converte é
footprint em **QPS**. É o mesmo padrão que o RaBitQ mostrou depois: **o quantizador permissivo dá memória, não
velocidade.**

## Como usar este registro

Antes de propor "quantizar para acelerar", a pergunta não é *"em qual regime o ganho apareceria?"* — é
**"qual gargalo essa técnica ataca, e o nosso caminho expõe esse gargalo?"**. Aqui a resposta é não, e ela foi
medida em quatro pontos.

## Relacionados

- [honest-negative/superioridade-vetorial-vs-scann](superioridade-vetorial-vs-scann.md)
- [failure-mode/dados-sinteticos-degenerados](../failure-modes/dados-sinteticos-degenerados.md)
