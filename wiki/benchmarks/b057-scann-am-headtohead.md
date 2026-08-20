---
type: Measurement
title: scann AM contra theodb_hnsw e pg_scann — o gap do ADR-0035 medido contra o produto
description: A recall casado no SIFT-128 a 100k, o scann AM do AlloyDB é 1,2-1,6× o theodb_hnsw e 0,92× o pg_scann — não os ~25× que o ADR-0035 mediu contra a biblioteca. Três configurações erradas vieram antes da certa, e cada uma produziu bundle válido.
resource: .claude/knowledge-base/discoveries/opportunities/b057-scann-am-headtohead-opportunity.md
tags: [benchmark, scann, alloydb, north-star, honest-negative, retratacao, b057]
generated: { by: claude-code/opus-5, at: 2026-08-17 }
sources:
  - id: b057
    resource: .claude/knowledge-base/discoveries/opportunities/b057-scann-am-headtohead-opportunity.md
    title: B-057 — oportunidade medida
    last_modified: 2026-08-17
  - id: adr0035
    resource: /decisions/0035-m73-northstar-vector-verdict.md
    title: ADR-0035 — veredito do North Star vetorial
---

**A recall casado, o `scann` AM do AlloyDB é 1,2–1,6× o `theodb_hnsw` e 0,92× o `pg_scann`** — não os
~25–44× que o [ADR-0035](/decisions/0035-m73-northstar-vector-verdict.md) registrou. Medido no droplet
efêmero `138.197.22.192` (s-8vcpu-16gb, nyc3), SIFT-128 verificado por checksum
(`dd6f0a6e…ca5984`), 100 000 vetores, k=10, 500 consultas, mesmo arnês, mesma máquina.

# Por que o número do ADR era outro

O ADR-0035 atribui o gap a *"AH-LUT anisotrópico **+ não pagar o imposto MVCC/WAL"***, e mediu a
**biblioteca** [ScaNN](/technologies/scann.md) — que o [m33](/benchmarks/m33-scann-headtohead.md)
declara "proxy sancionado". O produto não expõe a biblioteca: expõe `CREATE INDEX … USING scann`, um
access method do PostgreSQL, que paga o mesmo imposto de página, MVCC e WAL que nós. **A segunda
metade da causa não se aplica ao AM**, e é ela que respondia pela maior parte do gap.

# Os números

| recall casado | sistema | QPS | recall@10 | razão |
|---|---|---|---|---|
| ≈ 0,96 | `theodb_hnsw` ef=64 | 365,6 | 0,9616 | scann **1,20×** |
| | `scann` AH+rescore leaves=20 | 438,8 | 0,9590 | |
| ≈ 0,996 | `theodb_hnsw` ef=256 | 148,9 | 0,9956 | scann **1,64×** |
| | `scann` AH+rescore leaves=80 | 244,7 | 0,9958 | |
| ≈ 0,957 | **`pg_scann` AQ 64 subespaços, probes=20** | **476,5** | **0,9570** | **pg_scann 1,09×** |
| | `scann` AH+rescore leaves=20 | 438,8 | 0,9590 | |

A 1M, só o lado do Omni fechou: 213,6 QPS @ 0,9094 e 156,1 QPS @ 0,9832. O lado do TheoDB foi
cancelado pelo `statement_timeout` do arnês, não pelo motor.

# Três configurações erradas, e todas produziram bundle válido

Este é o achado que vale mais que o número, e é a mesma classe que
[o instrumento reporta o pedido](/guides/instrumento-reporta-o-pedido.md) documenta.

1. **Sem `LOAD 'alloydb_scann'`** — `SET scann.num_leaves_to_search` sucede, `pg_settings` não lista o
   GUC, a busca corre no default `0`. O portão de knob recusou; sem ele, três pontos idênticos.
2. **Com `quantizer='SQ8'`** — `VALID`, fronteira completa, e quantização **escalar**. `quantizer='AH'`
   falha com `AH quantization is not enabled for the index` a menos que `scann.enable_ah_quantizer`
   esteja ligado **no build**; o flag vem `off`.
3. **Com AH e sem rescore** — teto em recall **0,6582**, e 4× mais leaves comprando 1,4 ponto.
   `scann.pre_reordering_num_neighbors` vem `-1`; medido no mesmo índice e nos mesmos 80 leaves:
   `-1` → **0,6568**, `100` → **0,9964**, `500` → **0,9998**.

A terceira é a perigosa: publicar *"o scann teto em 0,66 enquanto o nosso chega a 0,9956"* seria
**alegação falsa contra outro produto**, e nos favorecia.

# A retratação do nosso lado

O primeiro par publicado neste ADR comparava `theodb_hnsw` — grafo puro, sem quantizador, sem AH, sem
rescore — contra o `scann` com AH e rescore. O TheoDB **tem** a receita: o
[pg_scann](/features/05-indice-scann.md) é `theodb_ivfflat` com `pq_subspaces` (quantizador
anisotrópico), `pq_bits=4` (LUT16), `aq_threshold` (o T), `soar_lambda` (SOAR) e
`separate_storage=1, refine=1` (rescore exato).

A primeira fronteira do `pg_scann` teto em 0,8212, e a causa foi medida: `pq_subspaces=16` sobre 128
dimensões são 8 dimensões por subespaço, e o ScaNN usa 2.

| `pq_subspaces` | dims/subespaço | QPS | recall@10 |
|---|---|---|---|
| 16 | 8 | 446,8 | 0,8172 |
| 32 | 4 | 485,8 | 0,9270 |
| **64** | **2** | 476,5 | **0,9570** |

# Ressalvas, e nenhuma é opcional

- **Escala 100k**; o ADR-0035 mediu a 1M, e índice IVF-quantizado e índice de grafo não escalam igual.
- O par `pg_scann` × `scann` é **um ponto**, não uma fronteira: só `probes=20` foi varrido nas três
  larguras, e o nosso recall é 0,002 **menor** que o deles.
- Quase todas as linhas vêm `(unstable)` do próprio arnês — perfil `smoke`, 3 repetições, **sem teste
  pareado de significância**.
- Majors diferentes: TheoDB 18.6, Omni 17.9, pgvector 17.11.
- **Ninguém foi tunado.** `num_leaves=316`, `pre_reordering=100`, `m=16`, `pq_subspaces=64` são escolhas.

**Não é claim de performance.** Responde ao critério de DoD *"o gap encolhe ao medir contra o AM em vez
da biblioteca?"* — e a resposta medida é sim, drasticamente.
