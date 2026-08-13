---
type: Measurement
title: b040 — BM25 do TheoDB no MS MARCO 100K, com o handicap declarado antes do número
description: Primeira medição do pilar lexical num arnês público, e ela vem com handicap — sem stemming, sem operadores de consulta, k1/b não configuráveis. Sob product-default, NDCG@10 0,6962, recall 0,8025, MRR 0,667 a 1.616 QPS.
tags: [benchmark, lexical, bm25, vectordbbench, msmarco, ndcg, b040]
item: B-040
generated: { by: claude-code/opus-5, at: 2026-08-13T01:00:00Z }
sources:
  - id: run
    resource: benchmarks/vectordbbench/results/
    title: log e JSON brutos da corrida
    last_modified: 2026-08-13
---

# Leia isto antes da tabela

O TheoDB **não faz stemming**. `jumping` não casa `jumps` — medido termo a termo. Também não tem operadores
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
- **Sem stemming, sem operadores, `k1`/`b` fixos** — dito no topo, repetido aqui porque a tabela pode ser
  lida sozinha.
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
