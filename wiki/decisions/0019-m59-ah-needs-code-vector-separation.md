---
type: Decision
title: ADR 0019 — PQ anisotrópico + AH implementados, mas o ganho exige separar códigos dos vetores f32
description: O eixo algorítmico do ScaNN foi implementado corretamente e mesmo assim mede paridade — a causa-raiz é o layout co-localizado, e o carrier HNSW acaba sendo o limite estrutural.
resource: git:f7c7b93:docs/adr/0019-m59-ah-needs-code-vector-separation.md
tags: [adr, quantizacao, anisotropica, asymmetric-hashing, layout, honest-negative, m59]
adr_id: "0019"
adr_status: Accepted
decision_date: 2026-07-08
owner: human:paulohenriquevn
milestone: M59
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0019
    resource: git:f7c7b93:docs/adr/0019-m59-ah-needs-code-vector-separation.md
    title: ADR 0019 — Anisotropic-PQ + AH
    last_modified: 2026-07-08
---

Um honest-negative rigoroso: o eixo algorítmico certo foi implementado, medido, e ainda assim não
venceu — e a análise identifica exatamente por quê, com aritmética que bate com a medição.

# Contexto

O gap de ~25× de QPS contra o [ScaNN](/technologies/scann.md), medido em
[m33](/benchmarks/m33-scann-headtohead.md), era o eixo prioritário do north-star vetorial. O
[M57](/decisions/0018-m57-sbq-inline-not-superior.md) mediu que o SBQ não fecha esse gap. O eixo
algorítmico real identificado foi **quantização anisotrópica** (a score-aware loss do ScaNN) com
**Asymmetric Hashing** e LUT16 SIMD via `pshufb`. O M59 implementou e mediu.

# Decisão

Reconhecer, por medição, análise e implementação testada, que **o eixo anisotrópico está
corretamente implementado — tanto no layout v3 co-localizado quanto no v4 separado — e ainda assim
NÃO supera o f32 em QPS a recall casado em nenhuma configuração medida no carrier HNSW**. A
superioridade que o ScaNN reporta exige o carrier IVF de batch-scan, não o pointer-chasing do
[HNSW](/technologies/hnsw.md).

# Evidência

500k × 768d cosine ([m59](/benchmarks/m59-anisotropic-ah.md)): paridade entre AQ e f32 a recall
casado ≥0,957 — 1,01× in-RAM e 1,03× sob pressão de 1,3 GB. Não os ≥2× buscados. A 20k in-RAM o AQ
mede 1,16×, dentro do ruído mas consistente com a alavanca: nessa escala o índice inteiro cabe em
cache por acidente, e o AQ leva vantagem onde nada thrasha.

# O mecanismo — validado por aritmética, não por hand-waving

O gargalo **não é o scoring**: o AH single-code é ~100× mais barato que o cosine f32 (3,8 µs por
candidato no f32 SIMD contra dezenas de nanossegundos no LUT). O gargalo é o **page-read / working
set** do walk. E a causa-raiz é o layout.

O layout v3 co-localizava o código AQ com o f32 no mesmo element tuple:
`[header][vetor f32: dim×4 = 3072 B][código AQ: 4 B]`. Para ler os **4 bytes** de código de um nó,
o walk **pagina o tuple inteiro de ~3 KB**:

| | working set quente a 500k × 768d |
|---|---|
| índice f32 | 500k × 3072 B ≈ **1,5 GB** |
| AQ v3 (co-localizado) | 500k × 3076 B ≈ **1,5 GB** |

O índice AQ tem **o mesmo tamanho** do f32. Os "códigos 768× menores" são irrelevantes ao I/O
porque estão guardados *ao lado* do f32, não *no lugar* dele. A conta bate exatamente com a
medição de paridade.

O que ScaNN e Faiss fazem — e o v3 não fez — é guardar **apenas os códigos** numa estrutura
compacta e contígua no hot-path, deixando os f32 numa região **separada**, tocada só no
reordenamento final do top-k.

## O v4 separou — e ainda assim não bastou

O v4 fez exatamente essa separação: element tuple contendo só o código, com o f32 numa região raw
contígua, apenas para rerank. Um byte-test provou o f32 fora do hot-path e a contiguidade das
páginas foi confirmada por inspeção. Medido a 500k × 768d sob pressão forte: **ainda paridade**
(AQ 2,3 contra f32 2,1).

A causa estrutural: sob pressão, o rerank lê `k·over_fetch` vetores f32 **frios** — folhas de NN,
pouco cacheáveis —, compensando a economia de reads do walk, que por sua vez revisita hub-nodes
cache-friendly no f32. A quantização reduz o **volume** de reads mas não os torna cache-friendly
num walk de pointer-chasing. A correção de layout era **necessária mas não suficiente**: o carrier
HNSW é o limite.[^adr0019]

A projeção que motivara o v4 — working set quente de ~50 MB (2 MB de códigos mais 48 MB de listas
de vizinhos) contra 1,5 GB de f32 frio — estava aritmeticamente certa para o *walk*, e mesmo assim
o rerank recolocou o custo.

# Consequências

O eixo algorítmico está resolvido e a fundação permanece no código, testada e opt-in: codebook
anisotrópico, kernel AH SIMD, persistência versionada, reloption e wiring do scan, com
compatibilidade retroativa intacta. O batching SIMD é ganho de **segunda ordem** — só importa
depois que o layout separa o hot-path, e o scoring já não era o gargalo.

Nenhum claim de superioridade vetorial é feito: o artefato honest-negative é o único claim.

# Opções consideradas

**Declarar o M59 fechando o gap** — a medição mostra paridade, não ≥2×. **Atribuir a paridade à
falta de carrier IVF** — rejeitada *na época* por análise, já que a causa-raiz medida era a
co-localização; o v4 depois mostrou que o carrier também pesa, e o registro preserva as duas
etapas do raciocínio em vez de reescrever a história.

# Ressalvas

Dados gaussian-mixture sintéticos. O teto de recall (0,958–0,974, abaixo de 0,99) é gap de grafo,
ortogonal — tratado em [m60](/benchmarks/m60-hnsw-recall.md). E o benchmark sob pressão foi gentil
demais: provar a tese estrutural exigiria medir a 2M, onde o f32 excede largamente a RAM.

[^adr0019]: ADR 0019 — Anisotropic-PQ + AH implementado, mas o ganho exige separar códigos dos vetores f32
