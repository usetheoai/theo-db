# ADR 0052 — Storage da engine lexical: heap buffer-then-flush (não index AM custom)

- **Status:** Aceito
- **Data:** 2026-07-22
- **Milestone:** M140.1 (medição + decisão de arquitetura de storage lexical)
- **Contexto anterior:** ADR 0051 (M139 — spike Tantivy-sobre-PG deu GO); M138 (fusão BM25 honest-negative)

## Contexto

O DoD original do M140 (anterior ao spike M139) previa a engine lexical como um **index
Access Method custom** (à la ParadeDB). O spike M139 (ADR 0051) descobriu, medindo, uma
alternativa: **heap buffer-then-flush** — o Tantivy escreve num `MemStore` em memória
(pgrx-free, thread-safe) e o flush vai para uma tabela heap do PG (`theodb.lexical_files`),
herdando **MVCC + WAL + TOAST de graça**, sem rmgr/página custom. Este ADR decide qual
storage a engine de produção (M140.3+) usa, **medindo** em vez de presumir (a decisão é de
fato irreversível — daí ser ADR medido).

## Decisão

**Heap buffer-then-flush é o storage da engine lexical.** O index AM custom é a alternativa
**rejeitada** por over-engineering — salvo se uma medição futura provar inversão de custo, o
que o M140.1 mediu que **não** ocorre.

## Evidência medida (M140.1, T3.1 — `docs/benchmarks/m140-1-data/logproxy.json`)

Corpus LogHub HDFS_2k (2000 linhas reais), PostgreSQL 18 em docker. Índice Tantivy limpo de
1 segmento (**313 KB** reproduzível; o número "626 KB" do draft era um artefato de dupla-indexação,
corrigido — review H1). Três framings apples-to-apples (review H2):

| Framing | Tantivy | PostgreSQL | Fator |
|---|---|---|---|
| índice-vs-índice (Tantivy guarda o body; GIN não) | 313 KB | GIN 532 KB | **1,7× menor** |
| footprint enxuto (heap+GIN+pkey+toast, sem coluna tsv) | 313 KB | 1 097 KB | **3,5× menor** |
| footprint fiel (baseline theo-lens c/ `search_tsv` materializado) | 313 KB | 1 565 KB | **5,0× menor** |

Ingest Tantivy (2000 docs): ~41 ms. A **direção (Tantivy menor) é robusta em todos os framings** e
consistente com o M139 (que mediu 2,8× menor vs `pg_textsearch`, ADR 0051). **Não há inversão de
custo** que justifique o AM custom — o argumento decisório abaixo é, aliás, independente do fator
exato (mesmo o mais conservador, 1,7×, favorece o heap).

## Rationale

- **Rule 9 (não reinventar) + anti-YAGNI.** O heap dá MVCC/WAL/crash-safety **de graça** —
  provado no M139 (`m139-lexical-crash-smoke.sh`: SIGABRT + replay do WAL, índice consistente).
  Um AM custom reimplementaria página/rmgr/VACUUM que o heap já entrega. O ParadeDB (o
  precedente de AM lexical) tem ~105k LoC; o caminho heap é O(milhares de LoC).
- **Sem custo medido do heap.** O índice é 2,5× menor, não maior — o argumento clássico "AM
  custom é mais compacto/rápido" **não se sustenta na medição**.
- **Superfície de crash menor.** Sem código de página/WAL próprio, a superfície de bugs de
  durabilidade é a do heap do PG (battle-tested), não código novo nosso.

## Alternativas consideradas

- **Index AM custom (o DoD original do M140).** Rejeitado: **nenhum benefício medido** (índice
  seria no máximo comparável, provavelmente maior; ganharia complexidade de página/rmgr/VACUUM).
  Reconsiderar SÓ se uma medição futura provar inversão (ex.: heap com write-amplification
  proibitiva sob update-rate alto — risco residual #153, a validar no M140.4).
- **ParadeDB (pg_search) como referência de forma.** Inelegível como código: **AGPL** (D1 barra
  AGPL na distribuição). Estudo apenas.

## Consequências

- **Habilita:** M140.3 constrói a engine BM25 de produção **sobre heap** (cache do Directory +
  superfície `bm25_search`/função), sem AM custom — menos LoC, menos superfície de durabilidade.
- **Restringe:** a forma da superfície pública (função `bm25_search` sobre heap, não `USING <am>`)
  — decidido aqui; M140.3 não reabre.
- **Rastreia:** o risco de flush-sob-merge / write-amplification em escala (#153) fica para o
  M140.4 provar pelas suítes de isolamento+crash contra o binário shipado.

## Referências

- ADR 0051 (M139 — spike GO, buffer-then-flush)
- `docs/benchmarks/m140-1-lexical-measurement.md` (o report deste milestone)
- `docs/benchmarks/m140-1-data/logproxy.json` (os números de storage)
- `docs/benchmarks/m138-bm25-fusion.md` (o honest-negative da fusão; contexto do pilar lexical)
- CLAUDE.md (TheoDB rule 4 — não reinvente; Esforço ≠ Complexidade)
