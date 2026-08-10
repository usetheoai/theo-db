---
type: Decision
title: ADR 0004 — NO-FORK do ScaNN: DiskANN é o substituto permissivo de qualidade equivalente
description: Não construir um access method theodb_scann nativo; o índice ANN entregue é StreamingDiskANN via pgvectorscale — decisão provisional, depois reaberta.
resource: git:f7c7b93:docs/adr/0004-scann-fork-decision.md
tags: [adr, ann, scann, diskann, fork-gate, m14]
adr_id: "0004"
adr_status: Accepted (provisional, reaberto pelo ADR 0006)
decision_date: 2026-06-28
milestone: M14
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0004
    resource: git:f7c7b93:docs/adr/0004-scann-fork-decision.md
    title: ADR 0004 — ScaNN access-method fork decision
    last_modified: 2026-06-29
---

O primeiro uso real do fork-gate: uma decisão de **não construir**, ancorada em número medido.

# Contexto

A especificação de feature [05 — índice ScaNN](/features/05-indice-scann.md) documentava um
access method `theodb_scann` literal — o [ScaNN](/technologies/scann.md) do Google, com
quantização anisotrópica, que é o índice vetorial do [AlloyDB](/technologies/alloydb.md).
Implementá-lo no PostgreSQL é um fork/AM nativo. A política de fork autoriza isso **apenas**
quando um benchmark reproduzível mostra o substituto permissivo insuficiente — e o TheoDB já
embarcava **StreamingDiskANN** via [pgvectorscale](/technologies/pgvectorscale.md).

# Decisão

**Não construir o `theodb_scann` nativo.** O índice de qualidade-ScaNN entregue é o
[DiskANN](/technologies/diskann.md), já presente na imagem.

Ancorada em evidência medida e citada — [m14](/benchmarks/m14-scann-fork-decision.md):

- **Medido** (primeira-parte, reproduzível, `runs=3`, `seed=14`): DiskANN atinge a barra de
  recall qualidade-ScaNN — `recall@10 = 0,934` com `sls=500` e `0,978` com `sls=1000`
  (n=5000, **dim=32, gaussiano sintético**). A barra é 0,90.
- **Citado:** o ScaNN ocupa a faixa ~0,90–0,99 de `recall@10` no ann-benchmarks, e o
  pgvectorscale publica o StreamingDiskANN em paridade de recall com QPS superior ao
  [HNSW](/technologies/hnsw.md) do pgvector a ~99% de recall em datasets reais de embedding.

Construir o AM nativo antes dessa evidência violaria measurement-first e a regra anti-sunk-cost.

# Ressalvas que limitam a força da decisão

Duas, registradas explicitamente — é o que mantém o status **provisional**:

- Os números de primeira-parte são **gaussianos sintéticos em dim=32**, abaixo da
  dimensionalidade real de embeddings (768/1536). Gaussiano é *desfavorável* ao DiskANN/SBQ, o
  que torna a travessia da barra conservadora — mas não representativa.
- **"Qualidade-ScaNN" aqui está escopado a `recall@k`.** O quantizador anisotrópico (AH) e as
  árvores multi-nível do ScaNN são um eixo distinto — **memória e compressão**. O DiskANN cobre
  isso de outro modo, via SBQ. A decisão não reivindica paridade nesse eixo.

# Gates de reabertura

1. **Resgate de recall:** benchmark reproduzível mostrando DiskANN **abaixo** da barra
   (`recall@10 < 0,90` com QPS usável) num dataset real representativo, sem que nenhum tuning
   de `query_search_list_size`/`query_rescore` feche o gap.
2. **Superioridade north-star:** este NO-FORK não fecha a aposta ScaNN-as-PG-AM que o
   [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) mantém aberta. Um AM
   nativo continua autorizado se um benchmark mostrar ganho sobre o DiskANN, ou um gap de
   memória-a-recall contra o quantizador AH que o SBQ não feche.

# Alternativas rejeitadas

Construir o `theodb_scann` agora (paridade literal com o AlloyDB) — fork massivo, necessidade
não-medida, violação direta do fork-gate. Pular o milestone porque o DiskANN já estava
entregue — a especificação e o gatilho de fork exigem uma decisão explícita e auditável, não
silêncio. Construir o AM independentemente da evidência — a armadilha do sunk-cost.[^adr0004]

# O que aconteceu depois

O [ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) **reabriu** esta decisão: índice
e quantização próprios em Rust passaram a ser permitidos, ainda gateados por benchmark. O
projeto de fato construiu os seus próprios AMs — ver
[ADR 0010](/decisions/0010-m26-index-am-scope.md) — e o veredito final do eixo de superioridade
vetorial está no [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md).

[^adr0004]: ADR 0004 — ScaNN access-method fork decision: NO-FORK
