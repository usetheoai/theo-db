---
type: Failure Mode
title: Destruir antes de provar a precondição deixa um estado PIOR que o inicial — e o vazio se disfarça de dado
description: O rebuild dropou hits_heap, recriou vazia, e só então descobriu que não conseguia ler o TSV. Onde não havia tabela passou a haver uma de 0 linhas — e 0 linhas lê como divergência de contagem, não como ausência.
resource: benchmarks/m169_rebuild_heap.sh
tags: [destrutivo, ordem, precondicao, benchmark, estado]
timestamp: 2026-07-31T00:00:00Z
---

# Destruir **antes** de provar a precondição deixa um estado pior que o inicial

## O caso medido (2026-07-31)

O script de reconstrução do gêmeo heap tinha duas guardas boas — box ociosa, colunar íntegro — e executou nesta
ordem:

1. `DROP TABLE IF EXISTS hits_heap`
2. `CREATE UNLOGGED TABLE hits_heap` (a partir do mesmo `create.sql`)
3. `\copy` do TSV de 70 GB → **`Permission denied`** → aborta

O estado ANTES: `hits_heap` **ausente**. O estado DEPOIS: `hits_heap` com **0 linhas**.

Isso é estritamente pior, e não por pouco. O gate de atestação da própria box distingue os dois casos:

| estado | identificador emitido | leitura do operador |
|---|---|---|
| ausente | `hits_heap_absent` | "falta carregar" — tolerável sob `ALLOW_MISSING_HEAP=1` |
| 0 linhas | `hits_heap_rowcount_mismatch` | "a carga **perdeu** linhas" — persegue-se um bug de COPY que não existe |

A falha converteu "não fiz" em "fiz errado". O tempo gasto depois é gasto no lugar errado.

## A classe

Todo passo destrutivo é uma aposta na precondição do passo seguinte. Se a precondição só é testada **depois** da
destruição, o custo do erro não é "a operação falhou" — é "a operação falhou **e** levou junto o que existia".

O agravante é que o resíduo raramente é neutro. Uma tabela vazia, um diretório meio-copiado, um índice truncado:
todos **existem**, então qualquer checagem de existência os aceita, e só uma checagem de conteúdo os rejeita.
O vazio se disfarça de dado.

## A regra

1. **Prove a precondição inteira antes do primeiro passo destrutivo.** Não a versão barata dela — a real: ler 1
   byte do arquivo **como o usuário que vai ler**, não `test -f`.
2. Quando a prova completa é cara, torne a destruição **reversível**: renomeie em vez de dropar
   (`ALTER TABLE … RENAME TO …_old`), e só apague depois do sucesso.
3. A mensagem de aborto diz **o que NÃO foi tocado**. `"ABORTA: … — e NADA foi dropado"` poupa a próxima pessoa
   de inspecionar o estado para descobrir o estrago.
4. Se o resíduo for inevitável, **remova-o no aborto**. Um `hits_heap` vazio deixado para trás é uma armadilha
   que dispara na próxima medição, não nesta.

## Distinção de um vizinho

Não confundir com [guard antes de materializar o pendente](guard-antes-de-materializar-o-pendente.md): lá o
guard roda cedo demais e **julga** estado parcial; aqui o passo destrutivo roda cedo demais e **cria** estado
parcial. São espelhos — e a correção é oposta: naquele, mova o guard para depois; neste, mova a prova para antes.

## Relacionados

- [invariant/ler-arquivo-exige-x-em-todo-o-caminho](../invariants/ler-arquivo-exige-x-em-todo-o-caminho.md) — a precondição concreta deste caso
- [failure-mode/guard-antes-de-materializar-o-pendente](guard-antes-de-materializar-o-pendente.md)
- [failure-mode/falso-verde-de-script](falso-verde-de-script.md)
