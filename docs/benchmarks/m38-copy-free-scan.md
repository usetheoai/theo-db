# M38 — Investigação do gargalo `reads` (medição, não um win de QPS)

**Hardware:** i7-1355U · 12 cores · 15.3 GB · AVX2 — CPU móvel, thermal-throttled + carga pesada de containers
durante a medição (**variância ALTA; o efeito medido é MENOR que essa variância**).

> **Outcome honesto (measurement-first):** este milestone entregou uma **MEDIÇÃO** que fechou 3 hipóteses, **não**
> um win de QPS. A mudança de código (eliminar a cópia dupla) é recall-idêntica e estritamente melhor, mas o
> benchmark **não** sustenta um claim de performance.

## F1 — SBQ falsificado (recall) ❌

`theodb.sbq_knn` vs o seqscan exato em **SIFT real 120k×128** (100 queries, baseline f32 recall@10 = **1.0000**):

| config SBQ | recall@10 |
|---|---|
| bits=1, over_fetch=40 | 0.774 |
| bits=2, over_fetch=40 | 0.854 |
| bits=4, over_fetch=40 | 0.947 |

Mesmo na melhor config, o SBQ fica abaixo de 1.0. Quantização **escalar** perde informação de ranking demais por
byte — por isso ScaNN/FAISS usam quantização de **produto (PQ)** + distância assimétrica via LUT. O gate "recall
preservado" não é atingível com SBQ.

## F2 — a cópia NÃO é o gargalo end-to-end ❌

O profiler do M36 sugeria `reads` = ~44% do scan, dominado pela **cópia dupla** do `read_chunked`
(`read_page_item.to_vec()` + `extend_from_slice` num Vec que cresce). Eliminei a cópia dupla
(`read_page_item_into`, uma cópia). O profiler **interno** caiu ~metade (~8ms→~4ms a probes=50). **Mas o QPS
end-to-end NÃO mostrou win confiável:**

| probes | ratio m38/m36 (várias medições) |
|---|---|
| 50 | 1.03 / 1.52 / 0.94 / 1.03 |
| 100 | 1.16 / 0.94 / 0.94 |
| 200 | 0.92 |

A **mesma comparação varia 50% entre runs** — o efeito é menor que a variância de medição. **Lição:** a atribuição
do profiler estava **inflada pelo overhead da própria instrumentação** (`Instant::now()` — ~400 syscalls/query a
probes=50), fazendo `reads` parecer 44% quando o custo real da cópia é pequeno. No end-to-end real, a cópia **não
é** o gargalo. Fazer a Phase 2 (score-off-page, eliminar a outra metade da cópia) daria o mesmo resultado.

## F3 — byproduct: a cópia dupla era desperdício (mantido como code-quality) ✅

`read_page_item_into` faz uma cópia (append direto) em vez das duas cópias + realloc do `read_chunked` antigo.
**Recall byte-idêntico** (61 testes de coexistência retornam os mesmos kNN ids). Merged como **refactor de
code-quality** (menos alocação/tráfego de memória), **NÃO** como win de QPS — o benchmark não sustenta o claim (F2).

## Veredito

- **Win de QPS?** Não. Nenhum lever de `reads` recall-zero-risco produz win end-to-end mensurável.
- **Byproduct:** `read_page_item_into` (dupla→simples cópia, recall-idêntico) — código melhor, sem claim de perf.
- **O lever vetorial real restante é algorítmico:** **PQ (product quantization + ADC via LUT — o algoritmo do
  ScaNN)** reduz candidatos/bytes preservando recall. É PhD-level (codebooks + LUT SIMD + persistência + gate de
  recall a 1M) — um milestone futuro grande, registrado no blueprint.

**Honestidade (Regra 3):** o measurement-first (3ª vez na saga M36/M38) reformulou o milestone. Shippar M38 como um
win de QPS seria não-sustentado pelos dados — exatamente o workaround que a disciplina proíbe.

## Reprodução

```
# recall SBQ: carregar subset SIFT numa tabela, theodb.sbq_knn vs seqscan
# QPS cópia: PGPORT=<port> python3 benchmarks/run_m36_scan.py --n 200000 --probes 50 --k 10 --runs 10
#   (theo-db:m36 = cópia dupla; theo-db:m38 = cópia simples)
```
