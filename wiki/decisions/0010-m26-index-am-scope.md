---
type: Decision
title: ADR 0010 — M26: escopo dos index AMs vetoriais (l2 primeiro, persistência em blob, params fixos)
description: Registra as quatro decisões de escopo dos primeiros access methods próprios do TheoDB, incluindo o scan O(N) aceito como limitação conhecida do MVP.
resource: git:f7c7b93:docs/adr/0010-m26-index-am-scope.md
tags: [adr, index-am, ivfflat, hnsw, rust, pgrx, m26, mvp]
adr_id: "0010"
adr_status: Accepted
decision_date: 2026-07-01
owner: human:paulohenriquevn
milestone: M26
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0010
    resource: git:f7c7b93:docs/adr/0010-m26-index-am-scope.md
    title: ADR 0010 — M26 vector index AM scope decisions
    last_modified: 2026-07-01
---

O momento em que o TheoDB deixou de compor índices de terceiros e passou a registrar os seus
próprios no PostgreSQL — com quatro limitações documentadas em vez de mascaradas.

# Contexto

O M26 promoveu o ANN in-memory (que reconstruía o índice a cada query) a **index AMs Postgres
persistidos**: `theodb_ivfflat` e `theodb_hnsw`. Isso significou registrar o `IndexAmRoutine`,
persistir em páginas WAL-logged via GenericXLog, fazer pushdown do planner
(`ORDER BY <-> LIMIT k` vira Index Scan) e manter o índice incrementalmente (pending-buffer no
`aminsert` mais fold no VACUUM). Validado ponta a ponta, com benchmark de **16×** contra o
rebuild-por-query ([m26](/benchmarks/m26-index-am.md)).

# As quatro decisões de escopo

## D1 — Operator class l2 primeiro; cosine e ip são follow-up

Ambos os AMs registram `theodb_{ivfflat,hnsw}_l2_ops` como DEFAULT. A métrica é **gravada no blob
persistido** e o scan a lê de volta — mas resolver a métrica a partir do *opclass no momento do
build* exigiria ler o nome da opfamily via catálogo, e o [pgrx](/technologies/pgrx.md) 0.16 não
expõe `get_opfamily_name`. Em vez de um lookup de syscache frágil sob o callback FFI de build,
`ambuild` fixa `Metric::L2`.

Rejeitadas: syscache `SearchSysCache1` com `GETSTRUCT` manual — FFI frágil, risco alto sob o
build callback; e SPI dentro de `ambuild` — evita-se SPI em callbacks de baixo nível. A camada
`Persisted` e o scan já são métrica-agnósticos, então o follow-up é aditivo.

## D2 — Persistência blob-por-scan; o scan O(N) é limitação conhecida

`ambuild` serializa o índice inteiro num blob; `amrescan` **deserializa o blob inteiro a cada
scan**. Correto, e 16× mais rápido que o rebuild-por-query (86 ms contra 1372 ms) — mas **O(N)
por scan**, portanto mais lento que um seq scan não-reconstruinte em N pequeno ou médio.

Aceito para o MVP sob measurement-first: correção, persistência, pushdown e manutenção primeiro,
com o número honesto publicado. Documentado, não mascarado.

## D3 — Parâmetros de build fixos

`lists=100` para IVFFlat, `m=16, ef_construction=64` para HNSW, espelhando os defaults da função
SQL. Reloptions (`WITH (lists=…)`) são follow-up; `amoptions` é `None`. Não bloqueia nenhum
critério de pronto — é tuning.

## D4 — Concorrência: lock advisório serializa o fold do VACUUM

O review apontou que o `rewrite_blob` do VACUUM re-mapeia o layout de páginas de forma
**não-atômica**, e que o `ShareUpdateExclusiveLock` do VACUUM não bloqueia SELECT/INSERT — risco
de torn-read e lost-insert sob concorrência. **Corrigido** com lock advisório xact-scoped
chaveado no OID do índice: scans e inserts pegam SHARE, o `vacuum_rebuild` pega EXCLUSIVE.
Trade-off de liveness aceito: um cursor longo pode atrasar o VACUUM.

**Follow-up documentado e não corrigido:** o `ScanState` é um `Box` Rust em `scan.opaque`,
liberado só pelo `amendscan`. Num `ereport(ERROR)` no meio do scan, o (sub)xact aborta sem chamar
`amendscan` e o `Box` vaza até o backend sair. É **limitado** — nos pontos de erro do `amrescan`
o `results` está vazio — e **não é undefined behaviour nem resultado errado**. A correção
apropriada é scan-state via `PgMemoryContexts`, liberado no reset de contexto, como faz o
pgvectorscale.

# Atualização posterior

O gargalo O(N) por scan está **fechado para o `theodb_ivfflat`**: leitura parcial de páginas
estruturadas, em que o scan lê apenas as listas sondadas. Medido em ~45× contra o blob O(N), e
~2,7× atrás do pgvector — resíduo de fator constante SIMD, tratado no
[ADR 0011](/decisions/0011-m31-rescope-simd-followup.md). O `theodb_hnsw` permanece no blob
O(N).[^adr0010]

# Consequências

Critério de pronto do M26 cumprido e validado, com camada de página, scan e vacuum compartilhada
e métrica-agnóstica — pronta para os follow-ups sem re-arquitetura. As negativas (l2-only, scan
O(N), params fixos) são aceitas e documentadas: nenhuma é workaround silencioso, cada uma tem
número e caminho honesto.

[^adr0010]: ADR 0010 — M26 vector index AM: scope decisions
