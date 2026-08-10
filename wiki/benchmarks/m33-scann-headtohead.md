---
type: Measurement
title: m33 — head-to-head contra o ScaNN (SIFT1M)
description: Mede o gap de QPS que definiu o pilar vetorial — ~25× a recall 0,99 — e declara explicitamente que compara uma biblioteca in-memory com um índice transacional.
resource: git:f7c7b93:docs/benchmarks/m33-scann-headtohead.md
tags: [benchmark, scann, ann, sift1m, gap, north-star]
dataset: sift-128-euclidean (SIFT1M)
milestone: M33
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m33
    resource: git:f7c7b93:docs/benchmarks/m33-scann-headtohead.md
    title: M33 — head-to-head vs AlloyDB/ScaNN
---

O benchmark que produziu o número mais citado do repositório — e cujas ressalvas são tão importantes
quanto os números.

**Método:** SIFT1M, n=1.000.000, dim=128, k=10, 1000 queries semeadas, 3 runs. O
[AlloyDB](/technologies/alloydb.md) é gerenciado e não roda localmente, então o
[ScaNN](/technologies/scann.md) OSS é o proxy sancionado — **é o algoritmo por trás do índice vetorial
do AlloyDB**.

# A fronteira recall × QPS

| Sistema | Params | recall@10 | QPS | p50 |
|---|---|---|---|---|
| ScaNN | leaves=50 | 0,9897 | **3182,6** | 0,28 ms |
| ScaNN | leaves=100 | 0,9969 | **1920,3** | 0,49 ms |
| theodb_ivfflat | probes=50 | 0,9924 | 77,9 | 12,77 ms |
| theodb_ivfflat | probes=100 | 0,9991 | 38,8 | 25,38 ms |
| pgvector_ivfflat | probes=50 | 0,9923 | 71,8 | 13,48 ms |
| pgvector_ivfflat | probes=100 | 0,9993 | 36,0 | 28,32 ms |

# Veredito no ponto de operação casado (recall ≥ 0,99)

| Dimensão | Veredito |
|---|---|
| Melhor QPS | **GAP** — 1920 contra 78 |
| Latência p50 | **GAP** — 0,49 ms contra 12,8 ms |
| Recall alcançável | **PARIDADE** — 0,997 contra 0,992 |
| Memória | **INDETERMINADO** — medidas diferentes |

**O recall está em paridade; o throughput não.** E note que o TheoDB e o pgvector estão praticamente
empatados entre si — o gap é do **paradigma**, não da implementação.

# A ressalva que muda a interpretação

**O ScaNN é uma biblioteca ANN puramente in-memory** — sem persistência, sem transações, sem SQL. O
índice do TheoDB é um índice **persistente e transacional** do PostgreSQL. Os números comparam **o eixo
algorítmico**; eles **não** tornam o ScaNN um banco de dados.

A memória fica marcada como indeterminada por motivo honesto: o pico de residência do ScaNN inclui o
corpus inteiro em memória, enquanto o número do TheoDB é o tamanho da estrutura **em disco** — **medidas
diferentes, que não se comparam**.

Outra ressalva de rigor: a partição do ScaNN **não é semeada**, então recall e QPS carregam variância
entre runs. O gap de throughput é **muito maior que a variância**, o que o torna robusto. E o tempo do
TheoDB inclui um round-trip de SQL sub-milissegundo — **o gap é algorítmico, não de comunicação**.

# O que isso significou

Este gap virou o eixo prioritário do projeto e foi perseguido por todos os caminhos honestos —
[SBQ](/decisions/0018-m57-sbq-inline-not-superior.md),
[anisotrópico](/decisions/0019-m59-ah-needs-code-vector-separation.md),
[RaBitQ](/decisions/0036-m74-rabitq-conditional-lever-verdict.md) e
[o algoritmo como access method](/decisions/0037-m82-am-ivf-aq-measured-verdict.md) — até o veredito de
que ele é **estruturalmente inalcançável** para uma extensão PostgreSQL permissiva
([ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md)).

A decomposição do gap em baldes recuperáveis e irrecuperáveis está em
[pesquisa de separação de storage](/references/scann-storage-separation-2026-07.md).
