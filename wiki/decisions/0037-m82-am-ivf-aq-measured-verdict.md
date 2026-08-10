---
type: Decision
title: ADR 0037 — pg_scann como Access Method: o ganho in-memory não sobrevive à página
description: O IVF-AQ+AH shipado como AM é lossless e correto, mas não entrega QPS — porque ler os códigos AQ pagina também os vetores f32 interleaved, e o scan é I/O-bound, não compute-bound.
resource: git:f7c7b93:docs/adr/0037-m82-am-ivf-aq-measured-verdict.md
tags: [adr, veredito, access-method, ivf-aq, layout, honest-negative, m82]
adr_id: "0037"
adr_status: Accepted
decision_date: 2026-07-11
milestone: M82
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0037
    resource: git:f7c7b93:docs/adr/0037-m82-am-ivf-aq-measured-verdict.md
    title: ADR-0037 — M82 veredito do pg_scann-as-AM
    last_modified: 2026-07-11
---

O último caminho aberto para a superioridade vetorial: shipar o **algoritmo do
[ScaNN](/technologies/scann.md)** — IVF com quantização e Asymmetric Hashing batched-LUT mais rerank
— como **access method próprio do PostgreSQL**, medido ponta a ponta **dentro do Postgres** a 1M.

# Decisão

O pilar é medido como **honest-negative final também por este caminho**:

1. O índice **é funcionalmente correto**: o `recall@10` é **byte-idêntico** ao IVF f32 exato em todos
   os níveis de probe — o pruning por AH somado ao rerank exato é **lossless** nestes ajustes.
2. O índice **não entrega ganho de QPS medível** sobre o IVF f32 no AM; as diferenças ficam dentro do
   ruído.
3. A recall 0,985 ele mede **78,5 QPS** — a classe do pgvector f32-IVF —, cerca de **24× abaixo** do
   ScaNN (1920 QPS a 0,99). Artefato: [m82](/benchmarks/m82-pgscann-headtohead.md).

# A causa-raiz — por que os 5–7× in-memory desapareceram

O spike anterior medira ~5–7× de QPS in-memory para IVF-AQ+AH, com o caveat explícito de que era
*single-thread, in-memory, sem o imposto de página e WAL*. **O caveat era load-bearing.**

No layout medido, os códigos AQ estão **interleaved** com os vetores f32 nas mesmas páginas por lista
(`[ids][f32][codes]`). Ler os códigos para pontuar por AH **também pagina os vetores f32**, então o
scan paga o **I/O f32 completo de qualquer jeito**. O LUT do AH economiza apenas o **compute** da
distância exata — e **compute não é o gargalo**. O scan do access method é limitado por **I/O e
sondagem de centroides**.

É a mesma classe de achado do [ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md), e a
literatura de sistemas documenta exatamente isso: o overhead de sistema mascara ganhos de compute
medidos in-memory.[^adr0037]

# Alternativas consideradas

**Reportar o achado honesto e fechar a track** (escolhida) — o track entregou o ciclo de vida
completo do índice, correto e testado; a performance medida é nula no AM; o valor é a prova final
mais a semente honesta da próxima alavanca.

**Redesenhar o layout separando códigos e f32 em páginas distintas e re-medir** — rejeitada aqui: é
redesenho de storage além do escopo, e não há evidência de que o gap de paradigma seja fechável por
extensão PG permissiva — o [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md) já mediu que
não é. Registrada como **semente**, não como trabalho feito.

**Forçar um número de superioridade reduzindo escala ou escolhendo probes a dedo** — violaria a regra
de que performance é claim medido.

# Consequências

O índice agrega **compressão de memória** — 16 bytes por vetor contra 512 do f32, isto é 32× nos
códigos, usados como filtro de candidatos lossless — **sem custo de recall**. Isso é benefício real de
**footprint**, não de QPS.

O modelo de custo do planner ficou ciente da nova versão, tratando o índice como um IVF com custo
proporcional ao número de probes — coerente com o medido.

**Track fechada** com o access method completo entregue — build, scan, página, WAL, VACUUM, fold e
custo, lossless — e o veredito de performance medido e honesto.

[^adr0037]: ADR-0037 — M82: veredito MEDIDO do pg_scann como Access Method
