---
type: Invariant
title: Quando a granularidade do relógio é maior que a distância entre dois eventos, o teste fica flaky
description: O smoke de PITR capturava o alvo no MESMO segundo do stop do backup; pgbackrest --type=time compara com estritamente-menor, então o restore falhava de forma intermitente.
resource: .claude/rules/testing.md
tags: [teste, flaky, tempo, recovery]
timestamp: 2026-07-30T00:00:00Z
---

# Quando a granularidade do relógio é **maior** que a distância entre dois eventos, o teste fica flaky

## O caso (BLOCKER — `pitr-smoke`)

O teste de PITR capturava o **timestamp alvo** de recuperação no **mesmo segundo de wall-clock** em que o backup
terminava. O `pgbackrest --type=time` compara com **estritamente menor**. Quando os dois caíam no mesmo segundo,
o alvo não era alcançável e o restore falhava — **às vezes**, dependendo de onde a corrida caía dentro do segundo.

Um teste que passa ~90% das vezes é pior que um que falha sempre: ele treina o time a re-rodar até ficar verde.

## O invariante

Sempre que um teste compara **dois instantes** produzidos pelo sistema, verifique:

| | |
|---|---|
| Qual a **resolução** do carimbo? | segundos, ms, µs — `--type=time` do pgbackrest é **segundo** |
| A comparação é `<` ou `<=`? | estritamente-menor exige distância **> 0** na resolução do relógio |
| Os dois eventos podem cair no **mesmo tick**? | se sim, o teste é flaky por construção |

**Um relógio de segundo não distingue dois eventos separados por 3 ms.** Nenhuma quantidade de retry conserta
isso — o retry só muda a probabilidade.

## Como fechar

1. **Separe os eventos explicitamente** — durma até o próximo tick, ou avance o alvo em 1 unidade da resolução.
2. **Prefira um marcador causal a um temporal** — LSN, número de transação, sequência. O PITR aceita `--type=lsn`
   e `--type=xid`, e ambos são exatos onde o tempo é aproximado.
3. Se o tempo é obrigatório, **assert a pré-condição**: falhe com mensagem clara se `alvo <= stop`, em vez de
   deixar o restore falhar de forma opaca.

Um teste flaky é um **bug**, não ruído (`rules/testing.md` § 3): corrija ou remova.

## Relacionados

- [failure-mode/estatistica-que-nao-sustenta-a-alegacao](../failure-modes/estatistica-que-nao-sustenta-a-alegacao.md)
- [technique/braco-de-controle-inalterado](../techniques/braco-de-controle-inalterado.md)
