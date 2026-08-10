---
type: Decision
title: ADR 0050 — Benchmarks oficiais: ADOTAR-E-ENVOLVER, não substituir
description: As ferramentas oficiais são runners de tempo e leaderboard — nenhuma oferece significância pareada, A/B byte-idêntico ou gate de correção de resultado, que é o que o projeto já tinha.
resource: git:f7c7b93:docs/adr/0050-official-benchmark-adopt-and-wrap.md
tags: [adr, benchmark, metodologia, clickbench, rigor, licenca]
adr_id: "0050"
adr_status: Accepted
decision_date: 2026-07-20
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0050
    resource: git:f7c7b93:docs/adr/0050-official-benchmark-adopt-and-wrap.md
    title: ADR-0050 — Official benchmark harness
    last_modified: 2026-07-20
---

Supersede uma preferência inicial do próprio owner — *substituir* o harness próprio pelos oficiais —
depois que a investigação mostrou o que a substituição custaria.

# Contexto

Os ~40 scripts de benchmark próprios do projeto são auto-autorados, e a disciplina de copy pública
exige **artefato reproduzível por terceiro** para qualquer claim comparativo. O pedido foi adotar os
benchmarks oficialmente usados pela indústria nos quatro pilares.

A investigação nos quatro pilares encontrou — **unanimemente** — que as ferramentas oficiais
(ann-benchmarks, VectorDBBench, big-ann-benchmarks, ClickBench, TPC-H/DS, pgbench, HammerDB,
CH-benCHmark) são **runners de tempo e leaderboard**. Nenhuma oferece:

1. teste de **significância estatística pareada**;
2. regressão de resultado por **A/B byte-idêntico**;
3. **gate de correção de resultado** — o `check` do ClickBench é literalmente um `SELECT 1`; pgbench
   e HammerDB publicam throughput com `fsync=off`; e o BenchBase valida tempo, não resultado.

O projeto já embarcava exatamente essas três capacidades.

# Decisão — adotar e envolver

Por pilar, adotar o **driver, os datasets reais e a entrada no leaderboard público** do benchmark
canônico — o que dá comparabilidade externa reproduzível por terceiros — **e reter uma camada fina
própria de análise** por cima: significância pareada, regressão de resultado byte-idêntica entre
versões, e gate de correção e de crash-safety.

Aposentam-se apenas os scripts **comparativos** redundantes, à medida que a entrada oficial de cada
pilar aterrissa; a camada de envelopamento é mantida e generalizada em biblioteca compartilhada.

**Rollout: piloto no vetor primeiro** — a fatia vertical que estabelece o padrão reutilizável, antes
de aplicá-lo aos demais pilares.

# Racional

Times de banco de dados de grande porte rodam os benchmarks padrão da indústria **para
comparabilidade externa** *e* põem sua própria camada de significância e regressão por cima — eles
nunca descartam significância. Substituir puramente (i) descartaria capacidades que as ferramentas
oficiais não têm e (ii) aceitaria um harness cujo gate é um `SELECT 1`, onde **uma engine rápida e
errada poderia liderar o ClickBench sem ser detectada** — o oposto de rigor.[^adr0050]

# Alternativas rejeitadas

**Substituição pura** — rejeitada por evidência unânime: derruba significância, regressão e correção,
e nada disso é reposto pelas ferramentas oficiais. **Manter só o próprio** — sem reprodutibilidade
por terceiros, que é precisamente a lacuna. **Big-bang nos quatro pilares** — rejeitada como rollout;
a fatia vertical prova a biblioteca antes do reuso.

# Consequências

Um programa de milestones sequenciado por pilar — vetor, colunar, OLTP, HTAP.

**Guardas de licença:** o dataset do ClickBench (CC-BY-NC-SA) e os do TEXMEX são **apenas download em
CI**, nunca empacotados; o HammerDB (GPLv3) é driver externo, fora da árvore; resultados TPC são
rotulados como "derivados de TPC"; e só os datasets com licença permissiva podem ser embarcados. A
licença do TEXMEX fica registrada como item aberto a verificar.

**Posicionamento preservado:** a magnitude do gap de QPS contra o ScaNN cita o **benchmark próprio**;
fontes públicas são usadas apenas para a *direção* do gap.

[^adr0050]: ADR-0050 — Official benchmark harness: ADOPT-AND-WRAP
