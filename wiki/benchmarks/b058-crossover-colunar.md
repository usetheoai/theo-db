---
type: Measurement
title: b058 — o crossover do colunar não é um número, são quatro respostas — e duas delas são gaps
description: Medido de 10K a 2M linhas. Em contagem e soma o colunar ganha cedo; no agregado FILTRADO ele PERDE em toda a faixa e piora com a escala; no GROUP BY ele nem roda. E o Parquet é 40× a 142× mais lento que o heap.
tags: [colunar, crossover, parquet, gap, b-058, b-006, honest-negative]
item: B-058
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Peças: [ADR-0066](../decisions/0066-b018-ef-search-default-fica-em-64.md) e o
[runbook do droplet](../runbooks/droplet-de-medicao.md).

# A pergunta, e por que ela tinha a forma errada

O [[B-058]] pede: *"o crossover do NOSSO colunar é medido, não estimado — abaixo de quantas linhas ele
perde para o heap?"* A pergunta pressupõe **um** número, porque é assim que o avaliador do AlloyDB
publicou o dele (a inversão em algumas centenas de milhares).

**A medição diz que a pergunta tem quatro respostas, e duas delas não são um crossover — são um gap.**

Droplet `g-16vcpu-64gb` (nyc1 — nyc3 estava sem capacidade), TheoDB 1.5.0 sobre PostgreSQL 18.6,
`shared_buffers=16GB`, benchmark registrado `analytical/crossover/row-count`, 3 repetições por ponto,
oráculo próprio conferindo toda resposta.

# Contagem e soma: o colunar ganha, e ganha cedo

| linhas | `total_rows` heap | colunar | | `sum_amount` heap | colunar | |
|---|---|---|---|---|---|---|
| 10.000 | 1,0 ms | 1,0 ms | 1,04× | 1,1 ms | 1,2 ms | **0,95× heap** |
| 50.000 | 3,4 | 1,5 | 2,22× | 3,8 | 2,3 | 1,67× |
| 100.000 | 7,0 | 1,9 | 3,69× | 9,4 | 3,4 | 2,75× |
| 500.000 | 23,0 | 5,7 | **4,02×** | 29,8 | 13,2 | 2,26× |
| 1.000.000 | 36,8 | 9,4 | 3,91× | 45,8 | 22,9 | 2,00× |
| 2.000.000 | 58,1 | 18,3 | 3,17× | 75,3 | 43,4 | 1,74× |

O crossover de `total_rows` está **abaixo de 10.000**; o de `sum_amount`, **entre 10.000 e 50.000**.
Muito mais cedo que as "algumas centenas de milhares" que o concorrente publicou.

## E a vantagem tem PICO, depois declina

`total_rows` vai a **4,02× em 500K** e cai para 3,17× em 2M. `sum_amount` vai a **2,75× em 100K** e cai
para 1,74× em 2M.

Isso é o contrário do que um store colunar deveria fazer: a vantagem dele vem de ler menos bytes por
linha, e essa economia não some com a escala. Uma vantagem que **encolhe** conforme o dado cresce
aponta para custo por stripe que não amortiza, ou para o dado deixando de caber onde cabia. Não está
diagnosticado, e não vou inventar a causa.

# `filtered_sum`: não há crossover, há um gap que PIORA

| linhas | heap | colunar | |
|---|---|---|---|
| 10.000 | 1,4 ms | 3,3 ms | **0,42× — heap** |
| 50.000 | 5,2 | 9,3 | 0,55× |
| 100.000 | 11,0 | 17,8 | 0,61× |
| 500.000 | 34,5 | 84,2 | 0,41× |
| 1.000.000 | 51,2 | 167,2 | 0,31× |
| 2.000.000 | 88,2 | **348,2** | **0,25×** |

**O colunar perde em toda a faixa medida, e a distância cresce com N.** A 2 milhões de linhas ele é
**4× mais lento** que o heap numa agregação com filtro — que é a forma mais comum de consulta
analítica real. Não é um crossover a ser atingido em escala maior: a curva vai na direção errada.

# `group_by_category`: recusado em todos os seis pontos

O portão de caminho analítico — que passou a ser pedido nesta mesma entrega — **recusou a medida em
todos os N**, porque o plano não usa o agregado colunar:

```
group_by_category via columnar @ 10.000 linhas    → prova falhou
group_by_category via columnar @ 2.000.000 linhas → prova falhou
```

O `GROUP BY` cai para `Seq Scan → external-merge Sort → GroupAggregate`. O docstring do adapter já
registrava isso e media 14× mais lento que heap. **Antes deste portão, esse número teria sido publicado
como "colunar".** Agora ele é recusado com a razão, e o heap correspondente (2,6 ms a 10K, 169,5 ms a
2M) fica na tabela sem par.

# Parquet: 40× a 142× mais lento que o heap

| linhas | heap | Parquet | |
|---|---|---|---|
| 10.000 | 1,0 ms | 42,2 ms | 40× pior |
| 100.000 | 7,0 | 392,0 | 56× |
| 1.000.000 | 36,8 | 4.521,9 | 123× |
| 2.000.000 | 58,1 | **8.283,4** | **142× pior** |

E **piora com a escala**. Oito segundos para contar 2 milhões de linhas. O caminho Parquet existe e
responde certo — o oráculo confere —, mas nesta forma de consulta ele não é uma alternativa ao heap
em nenhuma escala medida.

# O que isto muda

- **A resposta do [[B-058]] bullet 2 não é um número.** É: *abaixo de ~10K para contagem, entre 10K e
  50K para soma, nunca para agregado filtrado, e não se aplica ao `GROUP BY`.*
- **Dois gaps reais ficam nomeados**: o filtro que piora com N, e o `GROUP BY` sem pushdown.
- O [[B-006]] pergunta pela suíte completa do ClickBench. Esta medição sugere que a suíte completa
  encontraria os mesmos dois gaps multiplicados — a maioria das queries do ClickBench filtra e agrupa.

# Ressalvas declaradas

- Veredito do arnês **`EXPLORATORY`**, não `release`: faltaram CPU set declarado, limite de memória
  declarado e árvore git limpa (código enviado por tarball).
- Vários pontos vêm marcados `(unstable)` — CV acima do limiar do arnês. A **direção** de cada achado
  é grande demais para ser ruído (4×, 142×), mas as razões pontuais têm menos precisão do que os dois
  algarismos sugerem.
- Corpus **sintético semeado**, quatro formas de consulta. Não é ClickBench nem TPC-H; é o eixo que o
  DoD pede — o mesmo dado nos três caminhos, no mesmo binário.
- Região **nyc1** e não nyc3 das corridas anteriores. Para esta medição não importa: a comparação é
  interna à mesma máquina.

Artefato: `benchmarks/artifacts/20260821T122336Z-analytical-crossover-row-count-theodb-eed87401/`.

# Reprodução

```bash
theodb-bench run analytical/crossover/row-count --system theodb --profile research
```
