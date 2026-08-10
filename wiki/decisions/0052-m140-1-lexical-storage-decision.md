---
type: Decision
title: ADR 0052 — Storage da engine lexical: heap buffer-then-flush, não access method próprio
description: O índice Tantivy medido é 1,7× a 5× menor que o GIN dependendo do enquadramento, então o argumento clássico de que um AM próprio seria mais compacto não se sustenta.
resource: git:f7c7b93:docs/adr/0052-m140-1-lexical-storage-decision.md
tags: [adr, lexical, storage, heap, tantivy, yagni, m140]
adr_id: "0052"
adr_status: Accepted
decision_date: 2026-07-22
milestone: M140.1
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0052
    resource: git:f7c7b93:docs/adr/0052-m140-1-lexical-storage-decision.md
    title: ADR 0052 — Storage da engine lexical
    last_modified: 2026-07-22
---

O critério original previa a engine lexical como **access method próprio**, à la ParadeDB. O spike
anterior descobriu, medindo, uma alternativa — e este ADR decide entre as duas **medindo em vez de
presumir**, porque a decisão é de fato irreversível.

# Decisão

**Heap buffer-then-flush é o storage da engine lexical.** O access method próprio é **rejeitado por
over-engineering**, salvo se uma medição futura provar inversão de custo — que é justamente o que
esta medição diz **não** ocorrer.

# Evidência

Corpus de logs reais com 2000 linhas, PostgreSQL 18. Um índice [Tantivy](/technologies/tantivy.md)
limpo de um segmento ocupa **313 KB** — número reproduzível, depois de corrigir um artefato de dupla
indexação que inflava o rascunho. Três enquadramentos comparáveis
([m140.1](/benchmarks/m140-1-lexical-measurement.md)):

| Enquadramento | Tantivy | PostgreSQL | Fator |
|---|---|---|---|
| índice contra índice (o Tantivy guarda o corpo; o GIN não) | 313 KB | GIN 532 KB | **1,7× menor** |
| footprint enxuto (heap + GIN + pkey + toast) | 313 KB | 1097 KB | **3,5× menor** |
| footprint fiel (com a coluna de vetor de texto materializada) | 313 KB | 1565 KB | **5,0× menor** |

**A direção é robusta em todos os enquadramentos**, e o argumento decisório é independente do fator
exato: mesmo o mais conservador, 1,7×, favorece o heap. Ingestão de 2000 documentos em ~41 ms.

# Racional

**Não reinventar, e anti-YAGNI.** O heap dá **MVCC, WAL e crash-safety de graça** — provado por um
teste de SIGABRT com replay de WAL. Um AM próprio reimplementaria página, resource manager e VACUUM
que o heap já entrega. O precedente de AM lexical na indústria tem ~105 mil linhas; o caminho pelo
heap é da ordem de milhares.

**Sem custo medido do heap.** O índice é **menor**, não maior — então o argumento clássico "AM próprio
é mais compacto e mais rápido" **não se sustenta na medição**.

**Superfície de crash menor.** Sem código próprio de página e WAL, a superfície de bugs de
durabilidade é a do heap do PostgreSQL, battle-tested, e não código novo nosso.[^adr0052]

# Alternativas consideradas

**Access method próprio** — **nenhum benefício medido**: o índice seria no máximo comparável,
provavelmente maior, e ganharia complexidade de página, resource manager e VACUUM. Reconsiderar
**apenas** se uma medição futura provar inversão, por exemplo amplificação de escrita proibitiva sob
taxa alta de update. **ParadeDB como código** — inelegível: AGPL. Estudo apenas.

# Consequências

**Habilita** construir a engine BM25 de produção sobre heap, com menos código e menos superfície de
durabilidade. **Restringe** a forma da superfície pública a uma função de busca sobre heap, e não a
sintaxe `USING <am>` — decidido aqui, sem reabrir depois. E **rastreia** o risco de amplificação de
escrita sob merge em escala, a ser provado pelas suítes de isolamento e crash.

[^adr0052]: ADR 0052 — Storage da engine lexical: heap buffer-then-flush
