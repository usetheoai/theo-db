---
type: Measurement
title: b046/b042 — a recall casado o déficit vai de 3,4% a 19,6%, e é MENOR no alto recall
description: Fronteira completa contra o pgvector 0.8.6 no mesmo PG18.6, mesma máquina, mesmos parâmetros. Dois dos três pontos casam recall diretamente. O build é 1,82× e não 3,6×, e nosso índice é menor.
tags: [vetorial, pgvector, recall, qps, build, b-046, b-042]
item: B-046
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Peças: [ADR-0066](../decisions/0066-b018-ef-search-default-fica-em-64.md) — a mesma fronteira decidiu
o default; [b018](b018-planner-hnsw-juncao.md); [runbook do droplet](../runbooks/droplet-de-medicao.md).

# Por que esta medição existe

O [[B-046]] registrava **déficit de 16,3%** de QPS a recall casado, e o [[B-042]] registrava **build
3,6× mais lento**. Os dois números vieram do `Performance1536D50K` (50.000 × 1536d, cosseno) rodado
pelo VectorDBBench — **fora do arnês**, que é o que o [[B-069]] existe para corrigir.

Esta é a mesma pergunta, medida dentro do arnês, em SIFT1M (1.000.000 × 128d, L2).

# O controle que faz a comparação valer

Uma máquina, dois contêineres, **parâmetros idênticos** e — o que a rodada anterior não tinha —
**a mesma versão do PostgreSQL**:

| | motor | PostgreSQL | shared_buffers | maintenance_work_mem | workers |
|---|---|---|---|---|---|
| A | `theodb_rs` 1.5.0 | **18.6** | 16 GB | 8 GB | 8 |
| B | `pgvector` 0.8.6 | **18.6** | 16 GB | 8 GB | 8 |

A primeira tentativa usou `pgvector/pgvector:pg17` e teria comparado PG 17.11 contra 18.6 — um
confundidor de versão de engine dentro de uma comparação de extensão. A imagem `pg18` existe, e trocar
custou dois minutos.

# A fronteira

`g-16vcpu-64gb` (nyc3), SIFT1M completo, m=16, k=10, 500 consultas, 3 repetições, benchmark registrado
`vector/sift1m/frontier`.

| `ef_search` | theodb QPS | theodb recall | pgvector QPS | pgvector recall |
|---|---|---|---|---|
| 40 | 850,6 | 0,8320 | 628,3 | 0,9188 |
| 64 | 672,7 | 0,8994 | 491,7 | 0,9580 |
| 128 | 411,2 | 0,9574 | 270,4 | 0,9884 |
| 256 | 261,6 | 0,9890 | 168,8 | 0,9968 |

**Ler esta tabela por linha é o erro que ela existe para evitar.** No mesmo `ef` nós somos mais
rápidos em tudo — e também entregamos menos recall em tudo. `ef` não é a mesma unidade em dois grafos.

# A leitura honesta: recall casado

| recall | theodb | pgvector | déficit |
|---|---|---|---|
| 0,9188 | ~585 QPS (interpolado) | 628,3 QPS (`ef=40`) | **7,4%** |
| **0,9574** | 411,2 QPS (`ef=128`) | 491,7 QPS (`ef=64`) | **19,6%** |
| **0,9890** | 261,6 QPS (`ef=256`) | 270,4 QPS (`ef=128`) | **3,4%** |

**Dois dos três casam diretamente**, sem interpolação: 0,9574 contra 0,9580 e 0,9890 contra 0,9884 —
diferença na quarta casa decimal.

**O déficit não é constante, e a variação é o achado.** Ele vai de 19,6% no meio da curva a **3,4% no
alto recall** — e é o alto recall que um RAG de produção usa. O número único de "16,3%" que o
[[B-046]] registrava descreve um ponto, não o comportamento.

Nenhum dos três diz "paridade": o pgvector é mais rápido a recall casado em toda a faixa medida. O que
muda é a magnitude, e a direção em que ela se move — **fechamos conforme o recall sobe**.

# Build e tamanho (B-042)

| | build | índice |
|---|---|---|
| theodb | **142,0 s** | **724,1 MB** |
| pgvector | **78,0 s** | 782,4 MB |

**1,82× mais lento, não 3,6×** — com `max_parallel_maintenance_workers=8` declarado nos dois lados,
que era a assimetria que o [[B-042]] apontava (ele mediu o pgvector com 2 workers, o default). Sob
paridade de workers a razão cai pela metade.

E **nosso índice é 7,4% MENOR**: 724,1 MB contra 782,4 MB para o mesmo corpus. Esse eixo nunca tinha
sido medido, e ele corta a nosso favor.

# Ressalvas declaradas

- Veredito do arnês: **`EXPLORATORY`**, não `release` — faltaram CPU set declarado, limite de memória
  declarado e árvore git limpa (o código foi enviado por tarball). O recall é determinístico e não é
  afetado; o QPS tem CV abaixo de 2% nos oito pontos.
- **Corpus único.** SIFT1M é 128d/L2. O `b035` mediu 1536d/cosseno e achou 16,3%; os dois podem estar
  certos, e a diferença entre eles é dimensionalidade e métrica — não contradição.
- O ponto de 0,9188 é **interpolado linearmente** entre `ef=64` e `ef=128`. Os outros dois não.

# Reprodução

```bash
theodb-bench run vector/sift1m/frontier --system theodb   --profile research --dataset sift-128-euclidean
theodb-bench run vector/sift1m/frontier --system pgvector --profile research --dataset sift-128-euclidean
```

Artefatos: `benchmarks/artifacts/20260821T095912Z-vector-sift1m-frontier-theodb-03a84625/` e
`benchmarks/artifacts/20260821T100946Z-vector-sift1m-frontier-pgvector-08f2a02c/`.
