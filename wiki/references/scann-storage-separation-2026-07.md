---
type: Reference
title: Pesquisa — separação de storage num access method IVF-AQ fiel ao ScaNN
description: Decompõe o gap de QPS em três baldes e mostra que a comparação com a biblioteca era o sistema errado; o alvo honesto passa a ser o teto publicado do AlloyDB.
resource: git:f7c7b93:docs/research/scann-storage-separation-2026-07.md
tags: [referencia, pesquisa, scann, layout, io, fastscan, gap]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: scannsep
    resource: git:f7c7b93:docs/research/scann-storage-separation-2026-07.md
    title: Deep Research — Storage-Separated ScaNN-fidelity IVF-AQ
    last_modified: 2026-07-11
---

Dossiê que responde à semente aberta do [ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md) —
onde o índice IVF-AQ deu **zero ganho de QPS porque os códigos estavam interleaved com os vetores nas
mesmas páginas**.

# A reformulação honesta do alvo — leia isto primeiro

**O "gap de ~24× contra o [ScaNN](/technologies/scann.md)" é a comparação errada de sistema.** O ScaNN é
uma **biblioteca in-memory sem imposto de MVCC, WAL, heap e buffer manager**. A referência correta e
alcançável dentro do PostgreSQL é o **teto publicado do próprio [AlloyDB](/technologies/alloydb.md)** —
algo em torno de 4× sobre o pgvector HNSW.

E há evidência externa decisiva: literatura de sistemas mede que **o overhead de sistema consome 84,4%
dos ciclos de CPU do ScaNN quando ele roda dentro do Postgres**, e que quantização no pgvector rende
entre 0,75× e 1,04× de QPS — **nenhum ganho consistente**.

**Isso é exatamente o resultado nulo do ADR 0037.** O projeto **re-derivou independentemente um achado
SOTA publicado** — sinal forte de que a medição estava sã, e não era bug.

# A decomposição do gap em três baldes

| Balde | Mecanismo | Recuperável? | Confiança |
|---|---|---|---|
| **A — Layout e I/O** | separar códigos dos vetores, para o scan ler só códigos | **~4–6×** | média |
| **B — SIMD e LUT** | kernel já existe; o resíduo são anos de tuning | pouco | média-baixa |
| **C — Paradigma / imposto de sistema** | MVCC, WAL, heap, buffer, por tupla | **~4–6× IRRECUPERÁVEL** para extensão permissiva | **alta** |

**Alvo honesto:** um access method com storage separado pode plausivelmente recuperar **~4–6×**,
chegando à classe "AlloyDB-ScaNN dentro do Postgres" — e ainda ficando ~4–6× abaixo da biblioteca, o que
é **estruturalmente inalcançável** para uma extensão transacional.

**"Igual ao AlloyDB" é alcançável e gated; "vencer o ScaNN" não é.** Essa frase é o que o
[ADR 0033](/decisions/0033-north-star-reposition-proposal.md) formalizou depois.

# A convergência — os quatro SOTA separam, e nós éramos o outlier

Todas as implementações de referência guardam os códigos separados dos vetores brutos, com o refinamento
lendo de uma estrutura distinta. Uma delas passou por uma reescrita **que existe exatamente para parar de
ler os vetores um a um** — confirmando de fora que a separação é a alavanca, e que o I/O sobre precisão
plena era o gargalo.

O layout proposto: por lista, **duas cadeias de páginas** — uma só de códigos (~24 bytes por vetor) e
outra só de vetores (512 bytes por vetor) —, com scan em duas fases: a primeira lê **apenas** as páginas
de código e poda; a segunda lê os vetores **apenas** dos sobreviventes.

Redução estimada: a primeira fase lê **~22× menos bytes por lista**, crescendo com o número de listas
sondadas.

# O quantizador não é o problema

Decisão por não reinventar: o projeto **já tinha os dois lados** do caminho — o codebook e o kernel de
LUT. **O delta é layout, não codec.** A quantização escalar de alta fidelidade é codec de **rerank**, não
de **poda**, e adicioná-la seria resolver o problema errado.

**Ressalva honesta registrada:** o recall dos códigos de poda isolados é baixo — eles são **filtro de
candidatos, nunca resposta final**, e exigem rerank exato, que já fora medido como lossless.

# A lição de método sobre o spike

O spike de validação **não pode ser in-memory**. Um spike anterior já medira a separação in-memory em
5–7×, e o ADR 0037 viu isso **evaporar** no access method. Um segundo spike in-memory seria **teatro de
medição** — o valor está no **modelo de I/O**, que só existe dentro do banco.

Essa é a mesma lição que o [índice SymQG](/features/17-indice-symqg.md) reencontrou por conta própria.

# Nota de licença

Uma das implementações de referência estudadas é **AGPL** — foi **apenas estudada, nunca copiada**,
conforme a política registrada em [auditoria de licenças](/references/license-audit.md).
