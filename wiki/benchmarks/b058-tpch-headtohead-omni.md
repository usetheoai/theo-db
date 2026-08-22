---
type: Measurement
title: b058 — TPC-H contra o AlloyDB Omni: empatamos no heap e perdemos feio no colunar
description: "Primeira corrida do TheoDB contra o concorrente real na mesma máquina. No heap somos competitivos e a q18 é 1,4× mais rápida que a dele. No colunar o resultado é o oposto do que o pilar promete: o nosso é 5× a 16× mais lento que o NOSSO PRÓPRIO heap, enquanto o engine do Omni ganha do heap dele em duas das três queries. Colunar contra colunar, a q6 é 159× a favor dele. Uma execução por ponto, sem variância."
tags: [colunar, tpch, alloydb, omni, headtohead, honest-negative, b-058, b-006]
item: B-058
procedencia: arnes
generated: { by: claude-code/opus-5, at: 2026-08-22T17:00:00Z }
---

Artefatos: `benchmarks/artifacts/b058/tpch/` (10 JSON + `PROVENIENCIA.md`).
Peças: [b058 — o crossover do colunar](b058-crossover-colunar.md),
[b061 — o crossover colunar](b061-columnar-crossover.md),
[o instrumento reporta o pedido](../guides/instrumento-reporta-o-pedido.md).

# O que foi medido

Uma máquina (`g-16vcpu-64gb`), um dado, dois motores, cinco configurações. TPC-H q1/q6/q18 — as três
que a avaliação independente do AlloyDB publicou. **Toda resposta foi conferida contra o oráculo do
arnês e bateu, nas dez corridas.** Uma query rápida e errada não é uma query rápida.

## SF=0,1 (≈15 mil clientes, ≈600 mil `lineitem`), em ms

| configuração | q1 | q6 | q18 |
|---|---|---|---|
| **TheoDB heap** | 68,6 | 31,0 | **271,8** |
| **TheoDB colunar** (`theodb_columnar` + pushdown ligado) | 628,2 | 493,8 | 1412,5 |
| Omni, engine desligado (heap) | 66,3 | 24,8 | 380,3 |
| Omni, engine ligado, tabela heap | 75,1 | 26,3 | 332,4 |
| **Omni colunar** (`google_columnar_engine`) | **35,1** | **3,1** | 646,0 |

## SF=0,01 (≈60 mil `lineitem`), em ms

| configuração | q1 | q6 | q18 |
|---|---|---|---|
| TheoDB heap | 18,1 | 6,9 | 39,0 |
| TheoDB colunar | 64,8 | 51,2 | 452,5 |
| Omni, engine desligado | 20,0 | 7,4 | 42,7 |
| Omni, engine ligado, heap | 22,4 | 8,2 | 51,1 |
| Omni colunar | 22,4 | 1,8 | 48,2 |

# O que os números dizem

**No heap somos competitivos, e isso é uma boa notícia que ninguém tinha medido.** A SF=0,1 o nosso
heap empata na q1 (68,6 contra 66,3), fica 1,25× atrás na q6 — e é **1,4× mais rápido na q18**
(271,8 contra 380,3), que é a query que junta três tabelas. O motor PostgreSQL que carregamos não é
o problema.

**No colunar o resultado é o oposto do que o pilar existe para entregar.** O nosso colunar, com o
pushdown **ligado e verificado**, é mais lento que o **nosso próprio heap** em todas as três queries e
nas duas escalas:

| | q1 | q6 | q18 |
|---|---|---|---|
| nosso colunar ÷ nosso heap @ SF=0,1 | **9,2× pior** | **15,9× pior** | **5,2× pior** |

**E o engine do Omni faz o que um motor colunar deve fazer.** Contra o heap dele: **1,9× melhor** na
q1, **8,0× melhor** na q6 — e **1,7× PIOR na q18**, o que é honesto registrar, porque mostra que nem o
concorrente ganha em tudo. A q18 é uma junção de três tabelas com subconsulta, e não é o terreno de um
scan colunar.

**Colunar contra colunar, a SF=0,1:**

| | q1 | q6 | q18 |
|---|---|---|---|
| nós ÷ Omni | 17,9× mais lento | **159× mais lento** | 2,2× mais lento |

# Por que isto não é surpresa, e por que ainda assim precisava ser medido

O [[b058-crossover-colunar]] já havia medido que **o nosso colunar perde no agregado filtrado em toda a
faixa e não faz pushdown no `GROUP BY`**. As três queries do TPC-H são exatamente essas formas: a q1
agrupa, a q6 filtra e soma, a q18 agrupa e junta. A nota do [[B-006]] previu isto em 2026-08-21, com
estas palavras: *"a maioria das queries do ClickBench filtra e agrupa — rodar a suíte antes de tratar
esses dois gaps produziria uma tabela cujo resultado já é previsível, e cara"*.

**A previsão estava certa, e mesmo assim a medição mudou o que sabemos.** Antes dela, o gap era um
número contra o nosso próprio heap. Agora ele é um número contra o concorrente **rodando na mesma
máquina, no mesmo dado, no mesmo minuto** — que é a única forma que o [[ADR-0061]] aceita, e que o
North Star exige. E o eixo onde vamos bem — o heap, com a q18 à nossa frente — não estava medido
contra ninguém.

# Ressalvas, e elas são grandes

- **Uma execução por ponto.** O comando `tpch` roda cada query uma vez. **Não há variância, não há
  intervalo de confiança, não há teste pareado** — nada que `papers/rigorous-perf-eval-georges-2007`
  exigiria. Diferenças de poucos por cento nesta página não significam nada; as de 8× a 159×
  dificilmente são ruído, mas isso é um julgamento, não uma medição.
- **A escala é pequena.** SF=0,1 são ~600 mil `lineitem`. A avaliação independente do AlloyDB usou
  SF10 e SF100, onde ela mediu o engine dele **311× mais rápido** que o heap na q6. Nada aqui fala
  sobre como qualquer um dos dois se comporta duas ordens de grandeza acima.
- **Versões diferentes do PostgreSQL** — 18.6 do nosso lado, 17.9 do dele. É um confundidor real e
  inevitável: o Omni não existe em 18. Ele afeta sobretudo a comparação **heap × heap**.
- **Os artefatos são JSON cru, não bundles validados.** O `tpch` não emite bundle. Lacuna registrada
  no [[B-069]], não descuido.
- **Não medimos o que a q18 do Omni revela.** O engine dele piorar 1,7× numa query de junção é um
  achado sobre ele que não investigamos.

# O que isto muda

1. **A frase "nosso colunar" precisa de qualificação em qualquer texto público.** Ele é mais lento que
   o nosso próprio heap nas formas que o TPC-H exercita. Não é uma alternativa ao heap para carga
   analítica dessa forma, nesta escala.
2. **O [[B-006]] fica ainda mais claramente na ordem errada.** Rodar o ClickBench completo agora
   produziria 43+ linhas confirmando o que estas três já dizem. Os dois gaps — agregado filtrado e
   pushdown de `GROUP BY` — vêm primeiro.
3. **Há um resultado positivo que não estava medido e que vale dizer:** no heap somos par do AlloyDB
   Omni, e à frente dele na query mais complexa das três.
