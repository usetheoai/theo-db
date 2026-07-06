---
slug: cosine-ip-opclasses
milestone_id: M49
created_at: 2026-07-06
goal: Registrar opclasses cosine/IP nos dois AMs com kernel fused zero-alloc, provando pushdown + paridade recall@10 vs pgvector sem regressão L2.
---

# M49 — Opclasses cosine + inner-product no AM

## Goal

Adicionar as opclasses `cosine`/`ip` a `theodb_hnsw` e `theodb_ivfflat` — com resolução de métrica do opclass no build, kernel SIMD fused zero-alloc no traverse, e pushdown de `<=>`/`<#>` provado por `EXPLAIN` — de forma que **recall@10 de um índice cosine/IP tenha paridade (overlap ≥ 8/10) com o seqscan-oracle e a suíte L2 continue verde**.

## Context

Consome o blueprint SHIPPABLE `cosine-ip-opclasses-blueprint.md` (deep-research pgvector-âncora). A lacuna central: enum `Metric`, `metric_tag` persistido e read-back do VACUUM já existem, mas o build inicial fixa `Metric::L2` (`build.rs:65,87`) — um índice cosine constrói/pontua como L2 hoje. Decisões fechadas: ADR-1 (resolução via `index_getprocinfo`, não opclass name — pgrx 0.16 expõe), ADR-2 (Design B: kernels sobre bytes raw, sem normalizar — preserva formato/crash-safety), ADR-3 (`<#>`=negative IP, chave cosine=`1-cos`).

## Baseline Context

| File | LoC hoje | git sha | Por que existe | Invariantes a preservar |
|---|---|---|---|---|
| `theodb_rs/src/am/mod.rs` | ~245 | `3a81927` | make_amroutine + opclass DDL L2 (`:233-244`) + amcostestimate | `amcanorderbyop=true` (`:77`); L2 DEFAULT (`:233,243`) |
| `theodb_rs/src/am/build.rs` | ~330 | `3a81927` | ambuild (hardcode L2 `:65,87`) + vacuum_rebuild (lê tag `:206,255`) | tag read-back já correto no rebuild; só o build inicial mente |
| `theodb_rs/src/ann/mod.rs` | ~90 | `3a81927` | enum Metric L2/Ip/Cosine (`:30-63`), dist alocante (`:71`), NaN-LAST (`:117`) | tag()/from_tag() estável; NaN ordena por último |
| `theodb_rs/src/am/hnsw_page.rs` | 790 | `3a81927` | score() traverse (`:426-437`), is_l2 fused vs non-L2 alocante | partial-read O(ef·M); fused L2 zero-alloc |
| `theodb_rs/src/am/scan.rs` | ~230 | `3a81927` | scan IVF+HNSW; is_l2 dispatch (`:171,197-204`) | centroid scoring small-K; rerank |
| `theodb_rs/src/vec.rs` | ~210 | `3a81927` | kernels: l2/ip/cosine escalar + `l2_dist_from_bytes` fused (`:192-206`) | byte-parity com pgvector; AVX2 dispatch + scalar fallback |

**Current callers:** `HnswIndex::build_cancellable` (build.rs:87), `IvfflatIndex::build` (build.rs:65), `score()` (hnsw_page traverse), `metric.dist` (ann/mod.rs:71, hot loop non-L2). **Glossário:** opclass=operator class (type,AM,metric); amproc=support function; fused kernel=SIMD sobre bytes de página sem decode `Vec<f32>`; MIPS=max inner-product search.

## Prior Art & Related Work

Blueprint `cosine-ip-opclasses-blueprint.md`; pgvector `sql/vector.sql:283-332` (opclass), `src/hnswutils.c:140-158` (resolução), `src/vector.c:554-689` (kernels); pgrx `pg17.rs:35331-35332` (`index_getprocinfo`). Precedente interno: `l2_dist_from_bytes` (vec.rs:192) é o kernel-molde.

## ADRs

### ADR-1 — Resolução de métrica via amproc support function (`index_getprocinfo`), não opclass name
**Decisão:** registrar `FUNCTION 1 theodb_metric_{ip,cosine}(internal)` retornando a tag; no ambuild, `resolve_metric(indexrel)` = `index_getprocid(indexrel,1,1)`→ InvalidOid⇒L2 (fallback DEFAULT); senão `FunctionCall0Coll`→`from_tag`. `amsupport=1`.
**Rationale:** é o mecanismo pgvector (`hnswutils.c:154-158`); pgrx 0.16 expõe `index_getprocinfo` (`pg17.rs:35332`) — supera o TODO "get_opfamily_name indisponível". Cita `architecture.md` (DIP: opclass é fonte única da métrica).
**Alternativa rejeitada:** reloption `WITH(metric=)` — permite contradição opclass×métrica (nova mina); introspecção do OID do operador — frágil, pgvector não faz.

### ADR-2 — Kernels fused cosine/IP sobre bytes RAW (Design B), sem normalizar no store
**Decisão:** `ip_dist_from_bytes` (`-Σq·r`, compartilhado IP+numerador cosine) + `cosine_dist_from_bytes` (uma passada sim/norma/normb, clamp), AVX2+FMA+fallback, `assert_eq!(raw.len(),q.len()*4)`. Dispatch `score()`/scan de `is_l2` bool → 3-way `match metric`; remove `metric.dist` alocante do hot loop.
**Rationale:** preserva o SIGNIFICADO do formato de página (raw f32) — crash-safety, VACUUM rebuild (`build.rs:275`), rerank intocados; sem mudança semântica forçando REINDEX. NaN de vetor-zero já tratado (`ann/mod.rs:117`).
**Alternativa rejeitada:** Design A (normalizar no build, à la pgvector `hnswutils.c:406-428`) — muda o significado dos bytes (format bump + REINDEX + spherical k-means IVF + query-norm plumbing). Documentada como escalação se o benchmark exigir.

### ADR-3 — Convenção de sinal: `<#>`=negative IP; chave cosine=`1-cos`
**Decisão:** opclass liga `<#>` (negative IP, smaller=closer, pgvector `vector.c:631`); a chave retornada = valor do operador (`1-cos`, `-dot`) para evitar surpresa de recheck.
**Alternativa rejeitada:** retornar `-cos` (diverge do valor de `<=>`).

## Dependencies

| Dep | Versão | CVE | Rule 9 (por que não reinventar) |
|---|---|---|---|
| pgrx `index_getprocinfo`/`FunctionCall0Coll` | pg_sys 0.16.1 (já) | n/a | binding FFI do PG core — não reimplementar resolução de proc |
| pgvector `vector` type + `<=>`/`<#>` operators | na imagem (já) | n/a | operadores já existem; só ligamos opclass |

Nenhuma dependência nova. Kernels SIMD = `std::arch` (stdlib, rung 2).

## Dependency Graph

Phase 1 (opclass DDL + support procs + amsupport) → Phase 2 (resolve_metric no build) → Phase 3 (kernels fused + dispatch) → Phase 4 (benchmark paridade + crash-safety) . P1/P3 disjuntos podem paralelizar; P2 depende de P1 (support proc existe); P4 depende de P1-P3.

## Phase 1 — Opclass DDL + support procs

### T1.1 — 4 opclasses non-default + 3 support procs + amsupport=1

#### Why this step
**Ação:** `#[pg_extern] theodb_metric_{l2,ip,cosine}(internal)->i32` (tags 0/1/2); `extension_sql!` com `CREATE OPERATOR CLASS theodb_{hnsw,ivfflat}_{cosine,ip}_ops FOR TYPE vector USING … AS OPERATOR 1 <=>|<#> FOR ORDER BY float_ops, FUNCTION 1 theodb_metric_{cosine,ip}(internal)`; `amsupport=1` (`mod.rs:75`); `amvalidate` tolera L2 com 0 procs.
**Raciocínio:** blueprint Q1; strategy sempre 1, métrica no operador+FUNCTION 1. L2 permanece DEFAULT.

#### Files to edit
- `theodb_rs/src/am/mod.rs` — support procs + 4 extension_sql opclasses + amsupport=1.

#### TDD
- **RED:** `test_cosine_ip_opclasses_registered` (pytest): `CREATE INDEX … USING theodb_hnsw (embedding vector_cosine_ops)` e `vector_ip_ops` (hnsw+ivfflat) SUCEDEM; `EXPLAIN SELECT … ORDER BY embedding <=> $1 LIMIT 5` e `<#>` contêm `Index Scan` (pushdown). RED pré-fix: `ERROR: operator class "vector_cosine_ops" does not exist for access method "theodb_hnsw"`.
- **GREEN:** registrar.

#### Concurrency tests
(none — single-threaded DDL)

#### Acceptance criteria
- 4 opclasses criáveis; EXPLAIN prova pushdown de `<=>`/`<#>` nos 2 AMs; L2 default intocado.

#### DoD
- `pytest -k opclasses` passa; `grep amsupport theodb_rs/src/am/mod.rs` = 1.

## Phase 2 — Resolução de métrica no build

### T2.1 — resolve_metric(indexrel) no ambuild (fecha o hardcode L2)

#### Why this step
**Ação:** `unsafe fn resolve_metric(rel)->Metric` via `index_getprocid(rel,1,1)`→InvalidOid⇒L2; senão `FunctionCall0Coll(index_getprocinfo(rel,1,1))`→`Metric::from_tag`. Substituir `Metric::L2` em `ambuild`/`ambuild_hnsw` (`build.rs:65,87`) por `resolve_metric(indexrel)`; persistir `metric.tag()`.
**Raciocínio:** blueprint Q2/ADR-1; downstream (scan/vacuum) já honra a tag — só o build inicial mentia.

#### Files to edit
- `theodb_rs/src/am/build.rs` — resolve_metric + threading nos 2 builds.

#### TDD
- **RED:** `test_build_resolves_cosine_metric` (pytest): CREATE INDEX cosine, inserir vetores onde a ORDEM cosine ≠ ordem L2 (vetores de normas diferentes), `SELECT … ORDER BY embedding <=> q LIMIT 5` retorna a ordem COSINE (não L2). RED pré-fix: retorna ordem L2 (métrica errada).
- **GREEN:** resolve_metric.

#### Failure scenarios
- opclass sem support proc (L2) → `index_getprocid` InvalidOid → fallback L2 (não aborta). Testado por `test_l2_default_still_works`.
- meta ilegível no rebuild → já fail-safe (M48).

#### Acceptance criteria
- Índice cosine retorna ordem cosine; índice L2 default inalterado; tag persistida correta (build→scan→vacuum consistente).

#### DoD
- `pytest -k resolves_cosine` passa; suíte L2 (M45/M46) verde.

## Phase 3 — Kernels fused zero-alloc

### T3.1 — ip_dist_from_bytes + cosine_dist_from_bytes + dispatch 3-way

#### Why this step
**Ação:** em `vec.rs`, `ip_dist_from_bytes(q,&[u8])->f64` (`-Σq·r`) e `cosine_dist_from_bytes` (uma passada, clamp), AVX2+FMA + fallback escalar, `assert_eq!(raw.len(),q.len()*4)`. Generalizar `score()` (`hnsw_page.rs:426`) e scan (`scan.rs:197`) de `is_l2` bool → `match metric` selecionando o kernel fused; remover `metric.dist` alocante do hot loop (mantido em centroid small-K + rerank).
**Raciocínio:** blueprint Q3/ADR-2; a mina de alocação por nó (`hnsw_page.rs:431-435`) é o que M49 fecha — mesmo contrato zero-alloc do L2.

#### Files to edit
- `theodb_rs/src/vec.rs` — 2 kernels fused.
- `theodb_rs/src/am/hnsw_page.rs` — dispatch 3-way em score().
- `theodb_rs/src/am/scan.rs` — dispatch 3-way no scan IVF.

#### TDD
- **RED (Rust unit):** `ip_from_bytes_matches_scalar` + `cosine_from_bytes_matches_scalar` (`#[cfg(test)]` em vec.rs): kernel fused == `metric.dist` escalar dentro de 1e-5 sobre vetores seeded; inclui vetor-zero (cosine→NaN, ordenado por último). RED pré-impl: fn não existe.
- **GREEN:** kernels.
- **REFACTOR:** dispatch 3-way; deletar o path alocante do hot loop.

#### Concurrency tests
(none — kernels são funções puras; o build paralelo M44 usa `metric.dist` que permanece igual)

#### Acceptance criteria
- fused == escalar (1e-5); zero alocação por nó no traverse cosine/IP (grep: hot loop sem `Vec<f32>` decode não-L2); AVX2 dispatch preservado.

#### DoD
- `cargo test --lib vec` (builder) passa os testes de kernel; `cargo bench --no-run` linka.

## Phase 4 — Benchmark de paridade + crash-safety (o gate do milestone)

### T4.1 — Paridade recall@10 vs pgvector + crash-safety + artefato

#### Why this step
**Ação:** medir recall@10 de um índice cosine E ip (theodb_hnsw + theodb_ivfflat) vs o seqscan-oracle (mesma métrica) E vs pgvector `vector_cosine_ops`/`vector_ip_ops` no MESMO dataset seeded; teste crash-safety (build cosine → docker kill → recovery → scan idêntico); artefato `docs/benchmarks/m49-cosine-ip-opclasses.{md,json}`.
**Raciocínio:** blueprint edge #4/#6 + DoD do ROADMAP; nenhum claim de paridade sem número reproduzível (`public-copy.md`).

#### Files to edit
- `benchmarks/tests/test_am_cosine_ip.py` (NEW) — recall@10 paridade + pushdown + crash-safety.
- `docs/benchmarks/m49-cosine-ip-opclasses.{md,json}` (NEW).
- `CHANGELOG.md` (`[Unreleased] § Added`).

#### TDD
- **RED:** `test_cosine_recall_parity` (pytest): índice cosine recall@10 overlap ≥ 8/10 vs seqscan-oracle cosine; `test_ip_recall_parity` idem; `test_cosine_crash_safe` (docker kill → scan idêntico). RED pré-fix: cosine index dá ordem L2 → overlap baixo.
- **GREEN:** (já pelas fases 1-3) + gerar artefato.

#### Failure scenarios
- vetor-zero cosine (NaN) presente no dataset → não crasha, ordena por último (testado).
- IVF cosine centroids raw vs spherical (edge #6) → benchmark documenta paridade ou o gap honesto.

#### Acceptance criteria
- recall@10 ≥ 8/10 paridade cosine E ip, hnsw E ivfflat; crash-safe; artefato com metodologia + caveat MIPS (IP não é métrica).

#### DoD
- `pytest -k cosine or ip` passa; artefato commitado; CHANGELOG; sem regressão L2.

## Failure scenarios

- `index_getprocid` InvalidOid (L2 default) → fallback L2, não aborta (T2.1).
- vetor-zero sob cosine → NaN ordenado por último, sem panic atravessando C (T3.1/T4.1).
- opclass×métrica inconsistente build↔scan → impossível por construção (ADR-1: mesma resolução em build; scan lê a tag persistida).

## Coverage Matrix

| # | Gap (Goal/blueprint) | Task | Resolução |
|---|---|---|---|
| 1 | Opclasses cosine/IP registradas (Q1, DoD-1) | T1.1 | 4 extension_sql + support procs |
| 2 | Pushdown `<=>`/`<#>` provado (DoD-1) | T1.1 | EXPLAIN Index Scan |
| 3 | Métrica resolvida do opclass no build (Q2/ADR-1) | T2.1 | resolve_metric via index_getprocinfo |
| 4 | Kernel fused zero-alloc cosine/IP (Q3/ADR-2, DoD-2) | T3.1 | ip/cosine_dist_from_bytes + dispatch 3-way |
| 5 | Paridade numérica + recall@10 vs pgvector (DoD-3) | T4.1 | benchmark artefato |
| 6 | Coexistência L2 sem regressão + erros tipados (DoD-4) | T2.1,T4.1 | fallback L2 + suíte L2 verde |
| 7 | Crash-safety métrica consistente (edge #4) | T4.1 | build→kill→scan idêntico |
| 8 | Caveat MIPS (IP não-métrica) | T4.1 | seção caveat no artefato |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Design B (raw) recomputa norma por nó → cosine latency > pgvector | MÉDIO | ADR-2 escalação documentada (normalizar) se benchmark exigir | impl |
| IVF cosine k-means raw ≠ spherical → recall menor | MÉDIO | T4.1 mede paridade; se gap, documenta honesto (não silencioso) | impl |
| amsupport=1 quebra amvalidate p/ L2 (0 procs) | BAIXO | amvalidate tolera 0 procs no L2 (T1.1) | impl |

## Unresolved Questions

- IVF cosine com k-means raw atinge paridade de recall ou precisa de spherical k-means? → medido em T4.1 (se falhar, vira follow-up com ADR).

## Final Phase: Integration Validation

Suíte inteira verde na imagem rebuilt (maintenance + crash + regressão + os novos cosine/ip); `cargo bench --no-run` linka; artefato de paridade com mean±std; CHANGELOG por fase. O plano NÃO está completo até recall@10 ≥ 8/10 paridade em cosine E ip, hnsw E ivfflat, + crash-safety verde.
