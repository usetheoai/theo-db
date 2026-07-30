---
type: Failure Mode
title: Um guard colocado ANTES do passo que completa o estado transforma dado presente em zero linhas, em silêncio
description: scan_ivf_structured retornava cedo em centroides vazios antes de dobrar a região pendente — um índice vazio com INSERTs depois devolvia zero linhas sem erro.
resource: .claude/knowledge-base/reviews
tags: [correcao, indice, silencio, estado]
timestamp: 2026-07-30T00:00:00Z
---

# Um guard colocado **antes** do passo que completa o estado devolve zero linhas, em silêncio

## O caso (BLOCKER)

`scan_ivf_structured` começava com um guard razoável: *"sem centroides → nada a varrer, retorne"*. O problema é
**onde** ele estava — **antes** do fold da região pendente.

A sequência que quebra:

1. índice criado (ou `VACUUM`ado) **vazio** → zero centroides gravados;
2. `INSERT`s chegam → as linhas vão para a **região pendente**, ainda não dobradas;
3. `SELECT` → o guard vê zero centroides, **retorna cedo**, e a região pendente nunca é consultada.

Resultado: **zero linhas para dados que existem**, sem erro, sem aviso. O pior desfecho possível — não é uma falha,
é uma resposta **errada com cara de certa**.

## A classe

Um guard de curto-circuito é uma afirmação sobre o estado. Se ele roda **antes** da etapa que torna o estado
completo — fold, flush, materialização, hidratação de cache, aplicação de WAL pendente — ele julga um estado
**parcial** e conclui sobre o total.

O sintoma é sempre o mesmo e é sempre silencioso: o caminho rápido devolve o resultado vazio/default em vez de o
correto, e nenhum teste de happy-path pega, porque no happy-path o estado já estava completo.

## A regra

1. **Complete o estado, depois decida.** Fold/flush primeiro; o guard depois.
2. Se o guard **tem** de vir antes por custo, ele precisa consultar o estado pendente também — o predicado passa
   de "há centroides?" para "há centroides **ou** há pendentes?".
3. O teste que pega isto é o de **transição**: crie vazio → insira → consulte **sem** o passo de manutenção. Um
   teste que sempre popula antes de consultar nunca visita esse estado.

## Relacionados

- [failure-mode/fail-open-por-omissao](fail-open-por-omissao.md) — o primo em segurança: o caminho vazio vira permissivo
- [failure-mode/cobertura-alegada-sem-execucao](cobertura-alegada-sem-execucao.md)
