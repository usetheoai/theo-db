---
type: Failure Mode
title: Corrigir a instância que mostraram, e deixar os irmãos vivos
description: Um revisor aponta UM caso; o fix fecha aquele caso e não varre os outros do mesmo formato — que continuam lá, agora com a falsa sensação de terem sido revisados.
resource: benchmarks/m169_box_attest.py
tags: [remediacao, review, disciplina, honestidade]
timestamp: 2026-07-30T00:00:00Z
---

# Corrigir a **instância** que mostraram, e deixar os irmãos vivos

## A assinatura

Um revisor (ou um teste, ou uma medição) exibe **um** caso de um defeito. O fix conserta aquele caso, o gate fica
verde, e o commit diz "corrigido". Os outros casos do **mesmo formato** continuam no código — e agora carregam a
falsa sensação de já terem passado por revisão.

O que torna a classe cara é que ela é **invisível pelo mesmo motivo que o original**: se o revisor tivesse visto
os irmãos, teria apontado os irmãos.

## Cinco instâncias medidas numa única sessão (M169, 2026-07-30)

| # | O que mostraram | O que corrigi | O irmão que ficou |
|---|---|---|---|
| 1 | `timeout=60` no `wc -l` de um corpus de 69,7 GB | o `wc` | **`_psql_int`** ficou com os mesmos 60 s — e um `count(*)` a 100M leva ~2100 s, então a checagem de dataset **nunca podia** funcionar na escala para a qual foi escrita |
| 2 | `_sh` tratando não-zero como "não consegui rodar" | nada, achei correto | rotulou mal **três** comandos: o `count` que estourou o timeout, o `systemctl is-enabled` de uma unidade **corretamente mascarada**, e qualquer comando cujo código de saída **codifique um estado** |
| 3 | `_psql(sql, timeout)` — parâmetro acrescentado ao chamador | a assinatura de `_psql` | o **corpo** continuou repassando sem o argumento; só quebrou na box, depois do rsync |

E duas de formato vizinho, que não são instância-vs-classe mas o mesmo "saber não impede":

| | |
|---|---|
| nome de conceito escrito de memória | **três vezes** na mesma sessão, três vezes pego pelo gate C2 |
| comando longo em foreground remoto | documentei o invariante de manhã e repeti o erro **uma hora depois**, deixando um backend órfão que contaminou a medição |

## A regra

1. **Ao receber um achado, pergunte qual é a CLASSE dele** antes de escrever o fix. "Timeout curto demais para a
   escala" é a classe; `wc -l` é a instância.
2. **Varra por essa classe.** `grep` pelo padrão — todos os `timeout=`, todos os pontos que leem `returncode`,
   todos os chamadores da função cuja assinatura mudou. Custa minutos.
3. **O commit diz quantos irmãos foram encontrados**, inclusive zero. "Varri N ocorrências, uma precisava do
   mesmo fix" é informação; "corrigido" não é.
4. Quando o fix é numa **assinatura**, os chamadores são a varredura — e o compilador só ajuda em linguagem
   tipada. Em Python, o teste que exercita a **cadeia real** é o único que pega
   ([teste-que-passa-pela-razao-errada](teste-que-passa-pela-razao-errada.md) descreve o inverso: o teste que
   falsifica a camada onde o defeito mora).

> O sistema pegou as cinco. **O que funcionou foi o gate e o revisor, não a minha atenção** — e é por isso que a
> regra é varrer, não é "prestar mais atenção".

## Relacionados

- [failure-mode/allowlist-por-regex-sobre-linguagem](allowlist-por-regex-sobre-linguagem.md) — a mesma disciplina num caso específico: cada correção pontual só fecha a variante que alguém lembrou
- [failure-mode/teste-que-passa-pela-razao-errada](teste-que-passa-pela-razao-errada.md)
- [failure-mode/diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md)
- [technique/controle-positivo](../techniques/controle-positivo.md)
