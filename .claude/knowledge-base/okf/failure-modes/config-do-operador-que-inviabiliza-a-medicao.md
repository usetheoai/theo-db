---
type: Failure Mode
title: Uma configuração escolhida pelo operador torna o sistema inmedível
description: Um knob mexido com boa intenção (acelerar a carga) muda o regime a ponto de o experimento não poder rodar — e o sintoma parece bug do produto.
resource: https://github.com/usetheodev/theo-db/issues/221
tags: [medicao, configuracao, postgres]
timestamp: 2026-07-30T00:00:00Z
---

# Uma configuração escolhida pelo operador torna o sistema inmedível

## Assinatura

O sistema morre num caminho que a versão anterior percorria bem, e a diferença está no ambiente, não no código.

## Caso pago — M169

Pus `maintenance_work_mem = 2GB` para acelerar a carga de 100M. O backend foi OOM-killed com **23,4 GB de
`anon-rss`**. O M162 carregara os **mesmos** 100M numa box de 31 GB com o default de 64 MB.

A causa é real e virou o issue #221 — `flush_pending` consome ≈ `mwm × 7` — mas **o OOM foi escolha minha**.
Duas verdades que não se cancelam:

- eu causei aquele OOM específico;
- ele expôs um defeito real (o knob promete um orçamento e o consumo é ~7× ele).

E uma hora antes eu havia afirmado que "o flush incremental do M104 limita a memória de escrita, isso não é bug
do produto". A medição mostrou que o **gatilho** está limitado (`columnar.rs:1866`) e o **flush** não (`:1958`).

## Custo

~2 h de `COPY` de 75 GB perdidas — duas vezes, porque a tabela de origem era `UNLOGGED` e o recovery a truncou.

## Como evitar

- Ao mexer num knob para uma medição, **registre-o como parte da evidência** e compare com o baseline conhecido.
  Se o baseline anterior usava outro valor, a diferença é sua, não do código.
- Antes de acusar o produto, rode o caso na configuração em que ele **já** funcionou.

## Relacionados

- [measurement/amplificacao-maintenance-work-mem](../measurements/amplificacao-maintenance-work-mem.md)
- [invariant/unlogged-truncado-por-recovery](../invariants/unlogged-truncado-por-recovery.md)
