---
type: Decision
title: ADR 0038 — Separação de storage: tamanho confirmado a 16M, QPS out-of-RAM inconclusivo
description: O índice SQ8 é 3,52× menor que o f32 a 16M, mas o crossover de QPS não foi provado — o build estoura a RAM antes, e o ADR registra isso como dívida em vez de esconder.
resource: git:f7c7b93:docs/adr/0038-m88-billion-scale-regime-verdict.md
tags: [adr, veredito, storage-separation, sq8, escala, oom, honest-negative, m88]
adr_id: "0038"
adr_status: Accepted
decision_date: 2026-07-12
milestone: M88
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0038
    resource: git:f7c7b93:docs/adr/0038-m88-billion-scale-verdict.md
    title: ADR-0038 — M88 veredito billion-scale
    last_modified: 2026-07-12
---

A hipótese em teste: num regime em que os dados de refine f32 **não cabem em RAM**, a separação de
storage com refine SQ8 — ~4× menor — converteria vantagem de **memória** em vantagem de **QPS**,
porque menos páginas seriam lidas do disco por query.

# Decisão — `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`

Medido a 16M ([m88](/benchmarks/m88-billion-scale-verdict.md)):

1. **Vantagem de tamanho CONFIRMADA em escala.** O índice SQ8 é **3,52× menor** que o f32 a 16M
   (2382 MB contra 8382 MB), confirmando a 16× a escala o achado anterior de 3,5× a 1M. É a base
   mecânica da vantagem out-of-RAM: um terço e meio dos bytes de refine para paginar.
2. **QPS out-of-RAM é DIRECIONAL, não definitivo.** O SQ8 mostra **+21% de QPS a frio** com 32
   probes (10,2 contra 8,4) — mas isso é um **limite inferior**, porque a medição a frio limpa o
   cache uma vez por sweep, então só a primeira query é realmente fria. Consistente com a tese, e
   **não** uma medição limpa de crossover.
3. **A neutralidade de recall do SQ8 NÃO é reestabelecida aqui.** Ambos medem 0,291 num ponto
   **degenerado** — clusters sintéticos saturados de empates —, o que é artefato, não prova de
   qualidade do rerank. A neutralidade vem de um run anterior em dataset real.

# Por que o critério literal (≥100M) não foi atingido

É achado honesto de escala, não etapa pulada. O build segura o índice inteiro em memória mais uma
cópia coletada mais os buffers de página, resultando num **pico de ~4× o tamanho da base**.

**Dois OOM-kills observados a 30M** — 47 GB e depois 64 GB de anon-rss, excedendo os 62 GB usáveis
da máquina. 16M foi o maior que coube. Um índice **genuinamente out-of-RAM** não foi construível.

Registrado como **dívida técnica honesta**, não como falha silenciosa.[^adr0038]

# Alternativas rejeitadas

**Insistir a 30M na mesma máquina** — dois OOM-kills medidos; o build não cabe. **Provisionar máquina
maior** — rejeitado por ora, porque o gargalo é o build custar 4× a base, que é um bug de escala do
*build*, não do *query*; a alavanca correta é o build em streaming, não comprar RAM para mascarar
ineficiência. **Publicar o QPS a frio como vitória out-of-RAM** — a medição é limite inferior e o
recall é degenerado; seria spin.

# Consequências

A track fecha com a vantagem de **tamanho medida e confirmada em escala**, e a superioridade de **QPS
out-of-RAM como direcional e não provada** — a mesma disciplina dos vereditos anteriores.

**Nenhuma alegação** de superioridade de QPS vetorial sobre o ScaNN/AlloyDB é feita ou permitida: o
teto de paradigma permanece o medido no
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md).

**Follow-ups registrados como próxima linhagem:** o **build em streaming**, com flush incremental de
páginas em vez de bufferizar o índice inteiro, que é a maior alavanca e tornaria 100M+ construível em
hardware commodity — atacado no [ADR 0039](/decisions/0039-m89-ambuild-streaming-verdict.md); e dados
ANN reais em escala de bilhão com harness de cache frio por query, que é o setup capaz de transformar
o +21% direcional num crossover definitivo.

[^adr0038]: ADR-0038 — M88: veredito terminal da track storage-separation
