---
type: Measurement
title: b058 — o crossover do colunar não é um número, são quatro respostas — e duas delas são gaps
description: Medido de 10K a 2M linhas. Em contagem e soma o colunar ganha cedo; no agregado FILTRADO ele PERDE em toda a faixa e piora com a escala; no GROUP BY ele nem roda. E o Parquet é 40× a 142× mais lento que o heap. Re-medido após o B-097: o plano do GROUP BY mudou de GroupAggregate para Sort+HashAggregate, o QPS NÃO mudou, e a afirmação de que isso fechava o B-095 foi RETRATADA — o agregado vetorizado continua ausente.
tags: [colunar, crossover, parquet, gap, b-058, b-006, b-097, b-095, honest-negative, retratacao]
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

## A desvantagem do Parquet não é uma propriedade da consulta — vale para as quatro

*Acrescentado em 2026-08-22, ao fechar o [[B-008]]. Nada acima foi reescrito.* A tabela anterior mostra
**uma** forma de consulta, e uma tabela de uma consulta deixa em aberto a leitura mais generosa: *"o
Parquet perde nesta forma; talvez em outra ele ganhe"*. O mesmo bundle já continha as outras três, e
elas fecham essa porta.

Razão Parquet ÷ heap, p50 mediano, mesmo bundle:

| linhas | `total_rows` | `sum_amount` | `filtered_sum` | `group_by_category` |
|---|---|---|---|---|
| 10.000 | 38,9× | 52,5× | 24,6× | 14,9× |
| 100.000 | 51,1× | 42,5× | 35,9× | 17,4× |
| 1.000.000 | 134,2× | 105,8× | 86,2× | 54,4× |
| 2.000.000 | **154,0×** | **119,8×** | **100,3×** | **56,7×** |

**Nenhuma das quatro escapa, e as quatro pioram com a escala.** A melhor razão medida em qualquer
ponto — 14,9× no `GROUP BY` a 10 mil linhas — ainda é uma ordem de grandeza. O `group_by_category` é o
caso mais favorável ao Parquet **porque é onde o heap também sofre**, não porque o Parquet melhore: em
números absolutos ele sai de 43,2 ms para 9.684 ms na mesma faixa.

**Parte da constante já tem causa medida** e está em [[b096-parquet-jsonb-dois-roundtrips]]: são dois
round-trips de texto por leitura, e apenas um deles é nosso. Remover o nosso deu 1,085× — o que
significa que **esta desvantagem não é um problema de parsing**, e que atacá-la por ali seria otimizar
o eixo errado.

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

# Re-medido em 2026-08-21 depois do [[B-097]] — e uma afirmação minha caiu

O [[B-097]] corrigiu `columnar_relation_estimate_size`, que devolvia `*tuples = 0.0` fixo: o planner
via **zero linha** em toda tabela colunar. Re-corrida do MESMO benchmark, no MESMO droplet, no mesmo
dia, com as duas imagens construídas de commits vizinhos — `HEAD~1` e `HEAD` — diferindo só nisso.

## O mecanismo funcionou, e está medido

```
Seq Scan on prova  (cost=0.00..2097.00 rows=200000 width=0)     ← era rows=1
```

E a forma do plano do `GROUP BY` mudou nos **seis** pontos da faixa:

| | plano do topo |
|---|---|
| antes | `GroupAggregate` |
| depois | **`Sort`** → `HashAggregate` → `Seq Scan` |

É exatamente a forma que a guarda do M153 já admitia e que o planner cego nunca alcançava.

## CORREÇÃO (2026-08-22): o `GROUP BY` NÃO continua recusado

Medido com controle nas duas imagens, com `theodb.enable_columnar_agg=on`: sem a correção de
estimativa o plano é `GroupAggregate → Sort → Seq Scan`; com ela é **`Sort → Custom Scan
(theodb_columnar_agg)`**. **O pushdown engata, e o [[B-095]] fecha pelo [[B-097]].**

O portão de caminho analítico reprova nesta corrida porque o recurso é **opt-in e default OFF**, e o
arnês mede a configuração default. *"Não está ligado"* e *"não funciona"* são respostas diferentes, e
o portão dá a primeira — eu li como a segunda. A seção abaixo fica preservada, riscada.

## ~~Mas o `GROUP BY` continua recusado — e eu havia afirmado o contrário~~ (ERRADO — ver acima)

**RETRATAÇÃO.** Ao entregar o [[B-097]] eu escrevi, no `CHANGELOG.md` e na mensagem do commit
`a5abb85`, que o [[B-095]] *"fecha junto"* e que o `GROUP BY` voltava ao caminho colunar. **A medição
diz que não.** O portão de caminho analítico reprova nos seis pontos, na imagem corrigida, pela mesma
razão de antes:

```
'theodb_columnar_agg' is absent.  Plan: Sort
```

A forma do plano mudou; o **agregado vetorizado continua sem engatar**. Eu vi a forma mudar e supus o
resto — que é precisamente o erro que o portão de caminho existe para pegar, e que este conceito já
documentava: *"residência é necessária e não suficiente"*.

O [[B-095]] **permanece aberto**. Ele não era sintoma da estimativa degenerada, como o registro
supunha: é uma lacuna de pushdown própria, que a estimativa apenas escondia atrás de um plano ainda
pior.

## E o QPS não mudou

Latência p50 mediana no caminho colunar, base → fix, seis escalas:

| consulta | 10K | 50K | 100K | 500K | 1M | 2M |
|---|---|---|---|---|---|---|
| `total_rows` | 0,99× | 0,98× | 0,95× | 0,98× | 0,91× | 1,02× |
| `sum_amount` | 0,96× | 0,90× | 0,99× | 0,98× | 0,93× | 1,00× |
| `filtered_sum` | 1,00× | 0,95× | 0,97× | 0,98× | 1,00× | 0,99× |

**Ganho nulo, e publicado como nulo** — a DoD do [[B-097]] pede isso explicitamente. Faz sentido: as
três consultas que já rodavam pelo caminho colunar **já escolhiam o plano certo** mesmo com estimativa
degenerada, porque não havia alternativa a comparar. A estimativa decide entre formas; onde só há uma
forma, corrigi-la não muda nada.

## O que a correção evitou, e que teria sido pior

Com contagem real, uma tabela acima de `min_parallel_table_scan_size` passa a receber `Parallel Seq
Scan` — que o TAM colunar recusa. Sem a segunda metade da correção (`consider_parallel = false` mais
a limpeza do `partial_pathlist`), **um `SELECT count(*)` comum viraria ERRO** a partir de algumas
centenas de milhares de linhas. Medido depois da correção: `total_rows` roda nos seis pontos, inclusive
a 2 milhões. A estimativa zerada estava **mascarando uma capacidade não implementada**.

## Artefatos desta re-corrida

Os dois bundles saíram do `theodb-bench run` — benchmark registrado, validação de schema, registro de
ambiente e artefato imutável — e estão no repositório:

- baseline (`HEAD~1`): `benchmarks/artifacts/b097/base/20260821T212045Z-analytical-crossover-row-count-theodb-5a158dde/`
- corrigida (`HEAD`): `benchmarks/artifacts/b097/fix/20260821T213041Z-analytical-crossover-row-count-theodb-69c03e02/`
- smoke de validação do pipeline: `benchmarks/artifacts/b097/smoke/20260821T212008Z-analytical-synthetic-paths-theodb-865ec86a/`

Isto fecha o primeiro bullet do [[B-069]]: *"o crossover do colunar sai de `theodb-bench run` com
bundle válido, e o número publicado cita o bundle"*. A corrida original deste conceito citava bundle;
o adendo acima, escrito antes desta seção, **não citava** — que é precisamente o defeito que o gate do
próprio B-069 existe para pegar, e ele o pegaria.

## Ressalvas desta re-corrida

- Veredito do arnês **`EXPLORATORY`** nas duas corridas, pela mesma razão da original: código enviado
  por tarball, então `clean_source_tree` fica `UNAVAILABLE`. Ver
  [runbook § Tarball ou git clone](../runbooks/droplet-de-medicao.md).
- Droplet `g-16vcpu-64gb` em **nyc1**, PostgreSQL 18.6, `theodb_rs 1.5.0` — versões **lidas do
  servidor**, não da tag da imagem.
- As razões de latência ficam todas entre 0,90× e 1,02×, dentro do ruído que a corrida original já
  reportava como `(unstable)`. **Nenhuma diferença de desempenho é afirmada aqui**, em nenhuma direção.

# Reprodução

```bash
theodb-bench run analytical/crossover/row-count --system theodb --profile research
```
