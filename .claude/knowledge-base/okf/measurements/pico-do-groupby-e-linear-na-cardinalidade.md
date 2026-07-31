---
type: Measurement
title: O pico da pool no GROUP BY é LINEAR na cardinalidade — 2M grupos usam 95,4% de uma pool de 192 MiB
description: Medido 2026-07-31 sobre 2M linhas com work_mem 64 MB. 10³ grupos → 0,2 MiB; 10⁶ → 91,7 MiB; 2×10⁶ → 183 MiB. É o termo que o streaming NÃO reduz, e o que faz a q32 do ClickBench estourar a 100M.
resource: benchmarks/m169_groupby_peak.sh
tags: [groupby, memoria, cardinalidade, streaming, clickbench, limite]
timestamp: 2026-07-31T00:00:00Z
---

# O pico do `GROUP BY` é **linear na cardinalidade**

## Os números

2M linhas, `work_mem = 64MB` → pool de **192 MiB** (`work_mem*2 + 64 MiB`), binário `5ba1e09e`, box ociosa:

| grupos distintos | `peak_reserved` | % do teto |
|---|---|---|
| 10³ | 220.864 B (0,21 MiB) | 0,1 % |
| 10⁵ | 8.458.304 B (8,1 MiB) | 4,2 % |
| 10⁶ | 96.108.608 B (91,7 MiB) | 47,7 % |
| 2×10⁶, **uma** chave | 192.086.080 B (183,2 MiB) | **95,4 %** |
| 2×10⁶, **duas** chaves | 167.028.800 B (159,3 MiB) | 83,0 % |

Entre 10⁵ e 10⁶ o pico cresce ~11,4×, para um aumento de 10× na cardinalidade. Entre 10⁶ e 2×10⁶, ~2,0× para
2×. **Linear**, com constante de ~92 B por grupo.

## O que isto decide

O M169 fez o agregado consumir **um chunk-group por vez**, removendo o termo O(N) do *decode*. Este número
mostra o termo que **permanece**: a tabela de hash é O(grupos distintos) e não depende de quantas linhas chegam
por vez.

Consequência direta para o ClickBench a 100M: a **q32** (`GROUP BY WatchID, ClientIP`, chave quase-única)
teria ~10⁸ grupos. A ~92 B/grupo isso é ordem de **9 GB** de estado — contra uma pool de 192 MiB no default, ou
~576 MiB mesmo com `work_mem = 256MB`.

Foi por isso que a q32 apareceu no baseline com `agg_routed = true` **e** `timeout`: ela roteia pelo caminho
colunar e morre no estado, não nos offsets — ao contrário de q20/q33/q34, que morriam no
[teto de offsets i32](teto-offsets-i32.md) e são o que o T2.1 endereça.

## ⚠ A previsão que eu derivei daqui foi FALSIFICADA (2026-07-31, T4.1)

A versão anterior deste arquivo concluía, deste mesmo parágrafo: *"O estouro não é hipótese; é aritmética a
partir da reta medida."* **A q32 completou em 290,5 s no T4.1.** A reta está certa; a previsão tirada dela,
errada.

O defeito do raciocínio: extrapolei a reta assumindo que o estado **precisa caber na pool**. Sob streaming ele
não precisa — o DataFusion passa a poder **derramar para disco**, o que o caminho eager, entregando um único
batch gigante, não conseguia fazer no meio da execução. Mudar o regime de entrega **mudou o mecanismo**, e a
aritmética só valia no mecanismo antigo.

O mesmo spill tem dois sinais opostos na mesma corrida, o que é a evidência mais forte de que é ele o
mecanismo: **salva** a q32 (estado passa a caber em disco) e **quebra** q08/q09 (`COUNT(DISTINCT)`, que
esgotam o soft limit de 1024 descritores criando arquivos de partição). Ver
[failure-mode/extrapolar-reta-para-regime-de-outro-mecanismo](../failure-modes/extrapolar-reta-para-regime-de-outro-mecanismo.md).

O que continua válido, e é o que este conceito deve ser citado para sustentar: **a reta em si** (~92 B/grupo,
linear) e o fato de que o streaming **não reduz** o termo de estado. O que não se deve citar daqui é qualquer
previsão de falha derivada por aritmética sem checar se o mecanismo do regime-alvo é o mesmo.

## Duas honestidades sobre a medição

**Uma linha de trace a mais que o esperado.** O driver previa 5 e capturou 6 — o `EXPLAIN` de verificação também
roteia e emite trace. Não invalida os números (cada linha traz seu próprio pico), mas o contador esperado estava
errado, e arredondar isso para "5 de 5" seria maquiar.

**Duas chaves deram pico MENOR que uma (159 vs 183 MiB), com a mesma cardinalidade.** Contraintuitivo. A causa
provável é o layout do hash — duas colunas `bigint` versus uma podem cair em caminhos de agrupamento diferentes
no DataFusion. **Não medi o suficiente para afirmar**, e a diferença não muda a conclusão (ambas na mesma ordem
de grandeza, ambas lineares). Fica registrado como aberto em vez de explicado por conveniência.

## Relacionados

- [measurement/teto-offsets-i32](teto-offsets-i32.md) — o OUTRO modo de falha a 100M, que o T2.1 remove
- [measurement/limite-de-escala-100m-nao-conclusao](limite-de-escala-100m-nao-conclusao.md)
- [invariant/chunk-group-e-a-unidade-de-tudo](../invariants/chunk-group-e-a-unidade-de-tudo.md)
- [invariant/maintenance-work-mem-nao-capa-rss-de-rust](../invariants/maintenance-work-mem-nao-capa-rss-de-rust.md)
