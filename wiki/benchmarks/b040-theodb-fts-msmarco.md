---
type: Measurement
title: b040 — BM25 do TheoDB no MS MARCO 100K, com o handicap declarado antes do número
description: Medição do pilar lexical em arnês público. Com stemming (B-044): NDCG@10 0,7351, recall 0,8464, MRR 0,7034. O A/B na MESMA máquina mostra que stemming melhora qualidade E throughput; a primeira comparação, em máquinas diferentes, dizia o contrário.
tags: [benchmark, lexical, bm25, vectordbbench, msmarco, ndcg, b040]
item: B-040
generated: { by: claude-code/opus-5, at: 2026-08-13T01:00:00Z }
sources:
  - id: run
    resource: benchmarks/vectordbbench/results/
    title: log e JSON brutos da corrida
    last_modified: 2026-08-13
---

# Atualização de 2026-08-13 — o stemming entrou, e o A/B controlado inverteu o sinal

O [[B-044]] implementou stemming e remoção de stopwords. **A/B na MESMA máquina, mesmo caso, mesmo dataset
em cache — a única variável é a imagem:**

| | NDCG@10 | recall@10 | MRR | QPS | p99 serial | build |
|---|---|---|---|---|---|---|
| sem stemming (`theodb:b034`) | 0,6962 | 0,8025 | 0,6670 | 1.722,6 | 3,9 ms | 2,19 s |
| **com stemming (`theodb:b044`)** | **0,7351** | **0,8464** | **0,7034** | **1.910,3** | **3,5 ms** | 3,21 s |
| delta | **+5,6%** | **+5,5%** | **+5,5%** | **+10,9%** | **−10,3%** | +46,9% |

Qualidade sobe nos três eixos **e o throughput sobe junto** — remover stopwords encurta as listas de
postings mais do que o stemmer as alonga. O único custo é o build: +1,03 s sobre 100.000 documentos.

> **Significância, acrescentada em 2026-08-13 pelo [[B-045]].** O ganho de +5,6% em NDCG **não** foi
> submetido a teste pareado: as duas corridas do A/B foram feitas com imagens diferentes e os arrays por
> consulta do lado *sem stemming* não foram preservados. O que o B-045 mediu foi a comparação de três motores
> do [b047](b047-lexical-headtohead.md), onde os três estados coexistiam. **Este número segue observado, não
> demonstrado**, e refazê-lo com os arrays por consulta dos dois estados é trabalho de um ciclo próprio.

> **Uma primeira comparação minha dizia o oposto, e estava errada.** Medi a corrida com stemming num droplet
> Xeon 8168 e comparei com a corrida sem stemming, feita antes num Xeon **8358**. O delta aparente era
> **−31,8% de QPS** — atribuído ao stemmer. Refeito na mesma máquina, é **+10,9%**. NDCG, recall e MRR são
> independentes de hardware e não mudaram entre as duas leituras; **QPS e latência são**, e comparar CPUs
> diferentes lhes atribui uma diferença que é da máquina.
>
> É o mesmo erro que o [b035](b035-theodb-vs-pgvector-pg18.md) documentou no eixo vetorial, num eixo novo: lá
> o parâmetro era igual e o ponto de operação não; aqui o rótulo era igual e a máquina não. O
> [ADR-0061](../decisions/0061-benchmark-oficial-por-pilar.md) já exigia "mesma máquina" para concorrentes —
> esta corrida mostra que a exigência vale igual para **antes-e-depois do mesmo motor**.

Os números da corrida original, abaixo, ficam como registro do estado sem stemming.

# O estado sem stemming (medido em 2026-08-12)

O TheoDB **não fazia stemming**. `jumping` não casa `jumps` — medido termo a termo. Também não tem operadores
de consulta: `"frase exata"` é tratada como palavras soltas, `AND` vira um termo (casa documentos que contêm
a palavra "and"), `-exclusão` é ignorada e `prefixo*` devolve vazio. E `k1`/`b`, os dois parâmetros do BM25,
**não são configuráveis** — não existe GUC para nenhum dos dois, então esta corrida é **product-default** e
não poderia ser outra coisa.

Os motores contra os quais o pilar lexical seria comparado — Elasticsearch, OpenSearch — usam analisadores
que **stemmizam por padrão**. Qualquer número abaixo carrega essa diferença de pré-processamento embutida.
Ela não é defeito de implementação a corrigir: é uma propriedade a considerar antes de atribuir a diferença
ao motor de ranqueamento.

# O que foi medido

| | |
|---|---|
| Caso | `FTSBm25Performance` — **MS MARCO Small, 100.000 documentos** |
| k | 10 |
| Máquina | droplet DigitalOcean `g-16vcpu-64gb`, nyc3, **IP 167.172.229.34** (efêmero, destruído ao fim) |
| CPU | Intel Xeon Platinum 8358 @ 2,60 GHz, 16 vCPU, 62 GB, kernel 5.15.0-186 |
| Por que esta máquina | `16c64g` é o rótulo de referência do próprio upstream |
| Motor | TheoDB `theodb:b034` sobre PostgreSQL 18.4 |
| Concorrência | 1, 5, 10, 20, 30, 40, 60, 80 clientes |
| Dataset | via `ir_datasets`, **3,9 GB** baixados (o arnês não serve FTS do S3 da Zilliz) |
| Duração total | **887 s**, download incluído |

# Resultado

| Métrica | Valor |
|---|---|
| **NDCG@10** | **0,6962** |
| **recall@10** | **0,8025** |
| **MRR** | **0,667** |
| QPS (pico, concorrência 60) | **1.616,4** |
| p99 serial | 4,8 ms |
| p95 serial | 4,3 ms |
| Carga de 100.000 documentos | 25,6 s |
| Build do índice lexical + carga | 28,1 s |

Curva de concorrência:

| clientes | 1 | 5 | 10 | 20 | 30 | 40 | 60 | 80 |
|---|---|---|---|---|---|---|---|---|
| QPS | 251,2 | 1.025,9 | 1.460,4 | 1.613,2 | 1.507,3 | 1.592,6 | **1.616,4** | 1.600,0 |
| p99 (s) | 0,005 | 0,007 | 0,010 | 0,027 | 0,046 | 0,060 | 0,086 | 0,112 |

O throughput satura por volta de 20 clientes e fica plano até 80, enquanto a latência p99 cresce linearmente
— o comportamento esperado de um sistema no teto de vazão, não de um que degrada.

# O que esta corrida NÃO cobre — e o mais importante primeiro

- **Não há comparação com nenhum outro motor.** O leaderboard público de full-text da Zilliz tem
  Elasticsearch, OpenSearch, Milvus e outros; **não rodei nenhum deles**. Citar os números publicados deles
  ao lado destes seria comparar corridas feitas em máquinas, versões e datas diferentes — exatamente o erro
  que o [`b035`](b035-theodb-vs-pgvector-pg18.md) documentou num eixo vizinho. Uma comparação honesta exige
  rodá-los no mesmo arnês, na mesma máquina, e é trabalho seguinte.
- **Sem teste de significância pareada.** O arnês não tem. Este é **um** número, de **uma** corrida: não há
  nem variância medida, quanto mais significância.
- **Um único dataset e um único tamanho.** MS MARCO Small é o default do caso. O arnês oferece MS MARCO
  Medium/Large e HotpotQA; nenhum foi rodado.
- **Operadores de consulta e `k1`/`b` continuam ausentes.** O stemming entrou no B-044; frase exata,
  booleanos, exclusão e prefixo não — são superfície de consulta, não analisador.
- **Índices construídos antes do B-044 não stemizam e não precisam migrar** — o nome do analisador vai no
  schema de cada índice. Para ganhar stemming, reconstrua com `bm25_build`.
- **Sem filtro.** O caso rodado é `NonFilter`.

# Relação com o m186

O [`m186-lexical-ndcg-scifact-verdict`](m186-lexical-ndcg-scifact-verdict.md) mediu **NDCG@10 0,6269** no
BEIR SciFact, contra 0,3016 do `ts_rank_cd` nativo. Este mede **0,6962** no MS MARCO 100K.

Os dois **não são comparáveis entre si** — corpora, domínios, tamanhos e conjuntos de consultas diferentes —
e a proximidade dos valores é coincidência, não confirmação. O que esta corrida acrescenta ao m186 é o que
faltava ao [[B-004]]: **um segundo corpus, maior e de outro domínio**, medido com o mesmo rigor. O item
continua aberto porque dois pontos ainda não são uma curva.

# Como o cliente lida com o que o motor não oferece

Registrado porque afeta a leitura do número:

- **`bm25_build` exige chave `BIGINT`**, e os ids do arnês são strings opacas. A tabela carrega uma chave
  substituta `GENERATED ALWAYS AS IDENTITY` ao lado do id real, e a busca faz `JOIN` de volta preservando a
  ordem do motor (`ORDER BY score DESC`). **O cliente nunca reordena** — NDCG e MRR saem do ranking do
  TheoDB, não do Python.
- **`bm25_search` sobre índice nunca construído devolve zero linhas sem erro**, o que é indistinguível de
  "nada casou". O cliente consulta `theodb.lexical_index_meta` antes de buscar e levanta. Sem isso, uma
  corrida que esquecesse o build reportaria recall 0 como se fosse medição. Item aberto: [[B-041]].
- **O cliente não stemmiza a consulta.** Stemmar só a consulta pioraria o casamento; stemmar os dois lados
  mediria o adaptador Python em vez do motor, e publicaria um número que a instalação real do TheoDB não
  reproduz.

# Política que esta corrida ajudou a fixar

O [ADR-0061](../decisions/0061-benchmark-oficial-por-pilar.md) tornou obrigatório declarar o handicap antes
do número e recusar comparação com leaderboard de outro ambiente — as duas coisas que este artefato faz.

# Reproduzir

```bash
docker compose -f benchmarks/vectordbbench/docker-compose.yml up -d theodb
uv venv --python 3.11 /tmp/vdbb && . /tmp/vdbb/bin/activate
uv pip install "vectordb-bench[theodb] @ git+https://github.com/usetheoai/VectorDBBench@theodb"
CASE=FTSBm25Performance K=10 ./benchmarks/vectordbbench/run-fts.sh
```

Log e JSON brutos em `benchmarks/vectordbbench/results/`; a spec da máquina em `results/machine-fts.txt`.
