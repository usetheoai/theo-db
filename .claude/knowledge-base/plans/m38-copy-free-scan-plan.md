---
slug: m38-copy-free-scan
milestone_id: M38
created_at: 2026-07-03
goal: Cut the theodb index-scan `reads` phase (~44% of scan cost, dominated by read_chunked's byte copy) with a recall-zero-risk change — eliminate the redundant reassembly copy and score directly off the pinned buffer pages — measured by THEODB_SCAN_PROFILE showing reads drop + end-to-end QPS up at byte-identical recall (61 coexistence tests return the same kNN ids).
---

# M38 — Scan sem-cópia: pontuar direto nas páginas (RE-ESCOPADO por medição)

## Goal

Cortar a fase `reads` do scan (`am/scan.rs` ivf; ~44% do custo medido no M36, dominada pela cópia de bytes do
`read_chunked`) **sem custo de recall**: eliminar a cópia redundante de reassembly e pontuar direto nos bytes da
página fixada — medido por `THEODB_SCAN_PROFILE` (fase `reads` cai) + o harness (QPS end-to-end sobe) a **recall
byte-idêntico** (os 61 testes de coexistência retornam os mesmos kNN ids).

## Context

O gate measurement-first do M38 (blueprint `m38-io-quantization-blueprint.md`) FALSIFICOU a abordagem SBQ original
(recall 0.77–0.95 < 1.0 em SIFT real — quantização escalar perde ranking). O achado decisivo: o profiler do M36 já
mostrou `reads` (44%) = `read_chunked` COPIANDO os bytes de cada lista, `score` (15%) = a distância SIMD — gastamos
~3× mais copiando que usando. O usuário escolheu o lever recall-zero-risco. Pior ainda: a cópia é **dupla**
(`read_page_item` faz `to_vec()`, depois `read_chunked` faz `extend_from_slice`).

## Baseline Context

### Files that will be touched

| File | LoC | git sha | Why |
|---|---|---|---|
| `theodb_rs/src/am/page.rs` | ~830 | (M36) | Phase 1: `read_page_item_into(&mut out)` (uma cópia, elimina a dupla). Phase 2 (se necessário): formato de lista v3 alinhado à página + leitura que pontua direto no buffer |
| `theodb_rs/src/am/scan.rs` | ~336 | (M36) | Phase 2 (se necessário): pontuar direto nos bytes da página fixada (sem o Vec de reassembly) |
| `benchmarks/run_m38_scan.py` | (NEW) | — | driver: profiler (reads antes/depois) + QPS a recall idêntico |
| `docs/benchmarks/m38-copy-free-scan.{md,json}` | (NEW) | — | a evidência |

### Current callers / dependents

- `am/page.rs` `read_chunked` (`unsafe fn read_chunked`) — chama `read_page_item` por chunk (`to_vec()`) e faz
  `out.extend_from_slice` (2ª cópia). Chamado por `read_ivf_list_bytes` (`am/scan.rs` scan hot path) e
  `main_index_pages`/`read_ivf_meta` (leitura de meta/diretório — pequenos, não hot).
- `am/scan.rs:181` `scan_ivf_structured` — `let bytes = read_ivf_list_bytes(...)` timed como `read_us`, depois o
  laço de score. Este é o hot path.
- `am/page.rs` `read_page_item` — pin buffer + `to_vec()` + unpin; o precedente de FFI seguro a reusar.

### Domain glossary

- **cópia dupla** — `read_page_item` copia o item da página num Vec (`to_vec()`); `read_chunked` copia esse Vec num
  `out` que cresce (`extend_from_slice`). Dois memcpies + realloc por chunk page.
- **read_page_item_into** — variante que copia o item DIRETO no buffer de saída do chamador (uma cópia, sem realloc
  do intermediário).
- **score-off-page** (Phase 2) — pontuar `l2_dist_from_bytes` direto nos bytes da página fixada, sem cópia nenhuma;
  exige candidatos alinhados à página (formato v3) para não cruzar fronteira.

### Architecture boundaries affected

Interno à camada de página do index-AM (`am/`). `read_page_item_into` reusa o scaffold FFI de `read_page_item`.
Sem nova dependência (parsimony rung 2 — só reorganiza cópias existentes).

## Prior Art & Related Work

- Blueprint (este ciclo): `m38-io-quantization-blueprint.md` (a falsificação do SBQ + o achado da cópia).
- In-repo: `l2_dist_from_bytes` (`vec.rs:167`, M31b) — já pontua sobre bytes; `read_page_item` (`am/page.rs`) — o
  padrão FFI de leitura de página fixada.

## ADRs

### ADR-1 — eliminar a cópia (recall-zero-risco), NÃO quantizar (SBQ falsificado)
**Decisão:** atacar o `reads` eliminando a cópia de reassembly, não com quantização lossy. **Rationale:** a
medição mostrou SBQ regride recall (0.77–0.95 < 1.0) e que o `reads` é dominado pela cópia (44% vs score 15%). A
cópia é puro desperdício (recall-zero-risco de remover). **Rejeitado:** SBQ/PQ (SBQ falsificado; PQ é milestone
grande — documentado no blueprint como futuro).

### ADR-2 — Phase 1 (dupla→simples cópia, sem mudança de formato) primeiro; medir; Phase 2 (score-off-page) se preciso
**Decisão:** entregar `read_page_item_into` (elimina a cópia dupla, zero mudança de formato) como slice 1, medir o
ganho, e só fazer o formato v3 alinhado + score-off-page (slice 2, BREAKING) se o slice 1 não capturar o ganho.
**Rationale:** parsimony + measurement-first — a menor mudança que testa a hipótese "cópia domina reads" antes do
formato BREAKING. **Rejeitado:** ir direto ao formato v3 (mudança grande antes de confirmar a hipótese barato).

## Dependencies

### Existing — use as-is
| Package | Version | Ecosystem | Why |
|---|---|---|---|
| (std + pgrx) | — | Rust | reorganiza cópias; sem nova dep |

### New — to be introduced
| Package | Version | Ecosystem | Rule 9 rationale | Why |
|---|---|---|---|---|
| (none) | | | — | — |

### Removed
| Package | Last version | Why |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 1 (read_page_item_into — elimina a cópia dupla, sem mudança de formato; mede a hipótese)
   ──▶ Phase 2 [gated pela medição] (formato lista v3 alinhado + score-off-page — cópia zero; BREAKING)
```

## Phase 1 — eliminar a cópia dupla (recall-zero-risco, sem mudança de formato)

### T1.1 — read_page_item_into: uma cópia direto no buffer de saída

#### Why this step
A cópia dupla em `read_chunked` (`to_vec()` + `extend_from_slice`) é puro desperdício. Uma cópia direto no `out`
elimina metade do memcpy + o realloc do intermediário — recall-zero-risco, sem mudança de formato, testando a
hipótese "cópia domina reads" barato antes de qualquer mudança BREAKING.

#### Files to edit
- `theodb_rs/src/am/page.rs`

#### TDD
- RED: `read_page_item_into_equals_read_page_item` (`#[pg_test]`) — para uma página com um item, `read_page_item_into`
  produz os MESMOS bytes que `read_page_item` (+ preserva conteúdo pré-existente do `out`). Given uma página com um
  item conhecido, when lida via `_into` num buffer com prefixo, then o buffer = prefixo + os bytes do item.
- RED: `read_chunked_bytes_unchanged` — `read_chunked` (agora usando `_into`) reassembla os MESMOS bytes que antes
  (round-trip: escrever um blob multi-chunk, ler de volta, bytes idênticos).
- GREEN: `read_page_item_into(rel, block, out: &mut Vec<u8>)` que pin+copia o item direto no `out` (uma cópia);
  `read_chunked` chama `_into` em vez de `extend_from_slice(&read_page_item(...))`.
- REFACTOR: `read_page_item` passa a delegar para `_into` (DRY — uma implementação de leitura de item).

#### Concurrency tests
(none — single-threaded). Leitura share-locked, contrato inalterado

#### Failure scenarios
- Página vazia / offset inválido → mesmo comportamento tipado que `read_page_item` (retorna vazio / Err); testado.
- Buffer não liberado num erro → o padrão `read_page_item` (unpin antes de retornar) é preservado.

#### Acceptance criteria
- `cargo pgrx test` verde (`_into` == `read_page_item`; `read_chunked` bytes idênticos).
- `cargo pgrx install --release` 0 warnings; **61 testes de coexistência verdes** (mesmos kNN ids — recall idêntico).

#### DoD
- `read_chunked` faz uma cópia por chunk (grep: sem `extend_from_slice(&read_page_item`); FFI de buffer preservado.

## Phase 2 — score-off-page (cópia zero; gated pela medição do Phase 1; BREAKING)

### T2.1 — formato lista v3 alinhado à página + scan pontua direto no buffer fixado

#### Why this step
Se o Phase 1 (uma cópia) não capturar o ganho de `reads`, eliminar a cópia POR COMPLETO: pontuar
`l2_dist_from_bytes` direto nos bytes da página fixada. Exige candidatos alinhados à página (formato v3, sem
straddle). Recall byte-idêntico (mesmos f32).

#### Files to edit
- `theodb_rs/src/am/page.rs` (formato v3 alinhado + leitura que expõe páginas fixadas), `theodb_rs/src/am/build.rs`
  (escrever alinhado), `theodb_rs/src/am/scan.rs` (pontuar direto no buffer)

#### TDD
- RED: `scan_off_page_knn_identical` — kNN SQL idêntico ao baseline (recall byte-idêntico) num índice v3.
- RED (negative): página v2 (não alinhada) rejeitada com REINDEX (magic/version bump).
- GREEN: formato v3 alinhado; scan fixa a página, pontua direto, desfixadura após; sem Vec de reassembly no hot path.
- REFACTOR: reusar o scaffold de pin de `read_page_item`.

#### Concurrency tests
(none — single-threaded). Buffer share-locked durante o scoring, desfixado após — sem novo estado mutável compartilhado

#### Failure scenarios
- Buffer fixado e não liberado (leak) num erro → RAII/guard ou unpin explícito em todo path; crash-safety test.
- Índice v2 legado → REINDEX (BREAKING documentado no CHANGELOG).

#### Acceptance criteria
- `cargo pgrx test` verde (kNN idêntico v3; v2 rejeitado); 0 warnings; 61 coexistência verdes.

#### DoD
- Hot path do scan sem cópia de reassembly (grep); recall byte-idêntico; formato v3 + REINDEX no CHANGELOG.

## Phase 3 — benchmark (a evidência)

### T3.1 — m38-copy-free-scan.{md,json}: reads antes/depois + QPS a recall idêntico

#### Why this step
A evidência measurement-first: a fase `reads` cai e o QPS end-to-end sobe, a recall IDÊNTICO — honesto sobre quanto
(a cópia era ~44%; o floor é o pin/unpin do buffer + a leitura de score).

#### Files to edit
- `benchmarks/run_m38_scan.py` (NEW), `docs/benchmarks/m38-copy-free-scan.{md,json}` (NEW), `CHANGELOG.md`

#### TDD
(none — artefato de medição, como M35/M36)

#### Concurrency tests
(none — single-threaded) — measurement benchmark

#### Failure scenarios
- Se `reads` não cair (a cópia não era o custo, era o pin/unpin) → honesto no artefato; o profiler localiza; a
  hipótese foi testada barato no Phase 1 (measurement-first — não inflar).

#### Acceptance criteria
- `docs/benchmarks/m38-copy-free-scan.json`: `THEODB_SCAN_PROFILE` mostra `reads` menor antes/depois; QPS ≥ baseline
  a recall IDÊNTICO; hardware + repro + veredito honesto.

#### DoD
- Artefato reproduzível; CHANGELOG linka; recall idêntico provado (61 testes).

## Coverage Matrix

| Goal / DoD item | Task(s) |
|---|---|
| Measurement-first (SBQ falsificado; reads = cópia) | ✅ concluído no discover (blueprint) |
| Cortar `reads` sem custo de recall (cópia eliminada) | T1.1 (dupla→simples), T2.1 (zero, se preciso) |
| Recall byte-idêntico ao baseline | T1.1, T2.1 (61 testes de coexistência) |
| Benchmark reads↓ + QPS↑ a recall idêntico | T3.1 |
| Coexistência M20–M36 verde; sem nova dependência | T1.1, T2.1 |
| CHANGELOG (Rule 6) + REINDEX se formato mudar | T2.1, T3.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| A cópia não era o custo (pin/unpin domina) → ganho pequeno | MÉDIO | Phase 1 testa a hipótese barato antes do formato BREAKING; honesto no artefato se o ganho for pequeno | paulohenriquevn |
| FFI: buffer fixado não liberado (leak/corrupção) no score-off-page | MÉDIO | reusar o scaffold `read_page_item`; unpin em todo path; crash-safety + review pgrx | paulohenriquevn |
| Formato v3 invalida índices v2 | BAIXO (pré-1.0) | REINDEX gate + CHANGELOG BREAKING | paulohenriquevn |

## Unresolved Questions

- O floor do `reads` (pin/unpin do buffer, inevitável) — quanto do 44% é cópia vs pin/unpin? Resolvido por: a
  medição do Phase 1 (se `reads` cair muito, era cópia; se pouco, era pin/unpin → Phase 2 não ajuda e paramos honesto).

## Failure scenarios

- **Cópia não era o custo** → ganho pequeno, honesto no artefato (Phase 1 testou barato). (T1.1, T3.1)
- **Buffer leak no score-off-page** → unpin em todo path + crash-safety test. (T2.1)
- **Índice v2 legado** → REINDEX. (T2.1)

## Final Phase — Integration Validation

- `cargo pgrx test` verde (`_into` == `read_page_item`; kNN idêntico; recall byte-idêntico).
- `cargo pgrx install --release` 0 warnings; 61 coexistência verdes no container.
- `docs/benchmarks/m38-copy-free-scan.{md,json}` committado: reads↓ + QPS↑ a recall idêntico, honesto. CHANGELOG.
