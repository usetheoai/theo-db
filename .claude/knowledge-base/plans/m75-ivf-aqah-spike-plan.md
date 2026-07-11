---
slug: m75-ivf-aqah-spike
milestone_id: M75
created_at: 2026-07-10
goal: Medir recall@10×QPS de um índice IVF-AQ+AH sobre real SIFT1M vs ScaNN(M33)+f32 e emitir o veredito D3 GO/honest-negative.
---

# Plan: M75 — Fase 0 pg_scann: spike de viabilidade IVF-AQ+AH (o gate measurement-first/D3)

## Context

Fase 0 do ROADMAP v6 (pg_scann). Fonte de verdade: o blueprint SHIPPABLE_WITH_CAVEATS
`.claude/knowledge-base/discoveries/blueprints/pg-scann-am-blueprint.md` (Corner 1 + ADR-C). O TheoDB já tem o
algoritmo do ScaNN own-code (AVQ `am/aq.rs`, AH-LUT16 `vec/ah.rs`, IVF `ann/ivf.rs`); o M59/ADR-0019 apontou que o
ganho de QPS exige o carrier **IVF batch-scan contíguo** (não o HNSW pointer-chasing) — a hipótese NÃO-REFUTADA.
Este spike mede-a com o código que já existe + o kernel batched que falta, ANTES de construir o AM completo
(anti-sunk-cost/D3). **Honesto:** os números AQ+AH-no-nosso-stack são UNBENCHMARKED; honest-negative é saída válida
que fecha o pilar em M73.

## Goal

Medir recall@10×QPS de um índice IVF-AQ+AH in-memory sobre real SIFT1M vs o frontier ScaNN (M33) + f32 baseline,
≥3 runs mean±std, e emitir o veredito D3 (GO / honest-partial / honest-negative) em `docs/benchmarks/m75-ivf-aqah-spike.md`.

## ADRs

### D1 — Spike standalone in-memory (Rust puro), NÃO o AM pgrx completo
**Decisão:** o spike é um binário/example Rust in-memory no crate `theodb_rs` (sem pgrx/page-format), medindo o
scan IVF-AQ+AH direto. **Rationale:** measurement-first/D3 (blueprint ADR-C) — o valor é o primeiro número honesto,
e o AM pgrx (layout de página/WAL) é caro e só se justifica se o spike der GO (anti-sunk-cost). `rules/architecture.md §1`
(domínio sem `pg_sys`) é respeitado — o scorer/probe é Rust puro testável sem banco. **Alternativa rejeitada:**
construir o AM pgrx primeiro (viola measurement-first; se honest-negative, é semanas jogadas fora).

### D2 — Reusar ivf.rs/aq.rs/ah.rs; só o batched-block kernel + o layout contíguo + o harness são novos
**Decisão:** compor sobre `IvfflatIndex` (partição), `AqQuantizer` (encode) e `Lut16` (LUT); escrever NOVO apenas
(a) o batched-block AH-LUT kernel (pshufb sobre N códigos contíguos + oráculo escalar), (b) o store contíguo IVF-AQ,
(c) o harness. **Rationale:** Rule 9 (não reinventar o que existe) + parsimony-ladder rung 4. **Alternativa
rejeitada:** reimplementar IVF/AVQ do zero (viola Rule 9; os módulos são validados por 175 pg_tests).

### D3 — Batched-block kernel reimplementado limpo (padrão rabitq-rs Apache-2.0), com oráculo escalar
**Decisão:** o kernel batched (16 lookups/`pshufb`) é reimplementado do zero a partir do PADRÃO documentado
(rabitq-rs `simd.rs:1018-1110`, Apache-2.0 — estudo), com um oráculo escalar como correctness gate (ranking
idêntico ao `ah_score_scalar` em loop). **Rationale:** D1 (Apache-2.0 é reusável mas reimplementamos limpo/idiomático)
+ `rules/testing.md` (correção antes de performance: o SIMD só é aceito se casar o oráculo). **Alternativa rejeitada:**
usar só o `ah_score_scalar` per-code (não mostra o ganho de QPS do batch — mediria o que o M59 já sabe).

## Baseline Context

### Files that will be touched

| File | LoC | git sha | Papel hoje | O que muda |
|---|---|---|---|---|
| `theodb_rs/src/vec/ah.rs` | 328 | ab9ac22 | AH-LUT16: `build_lut16`, `ah_score_scalar`, kernel pshufb PER-CODE | + batched-block kernel (scalar oracle + AVX2) |
| `theodb_rs/src/ann/ivf.rs` | 315 | 2376077 | IVF k-means++, campo `lists: Vec<Vec<usize>>`, `build/search/centroids` | + accessor pub(crate) das inverted lists |
| `theodb_rs/src/ann/ivf_aqah.rs` | 0 | (NEW) | — | NOVO: store contíguo IVF-AQ + query path (probe→block-scan→prune→rerank) |
| `theodb_rs/benches/ah_block.rs` | 0 | (NEW) | — | NOVO: criterion micro-bench do batched kernel (same-graph) |
| `theodb_rs/examples/m75_spike.rs` | 0 | (NEW) | — | NOVO: harness SIFT1M (load fvecs, build, sweep nprobe, emit JSON) |
| `docs/benchmarks/m75-ivf-aqah-spike.md` | 0 | (NEW) | — | NOVO: resultado medido + veredito D3 |

### Current callers (símbolos que o plano toca)

- `ah_score_scalar` / `build_lut16` (`vec/ah.rs`): chamados em `am/hnsw_page.rs` (o scan v3 HNSW) — `grep -rn "ah_score_scalar\|build_lut16" theodb_rs/src` confirma callers em `vec/ah.rs` (tests) + o wiring HNSW. O batched kernel é ADITIVO (novo símbolo), não muda os existentes.
- `AqQuantizer::encode` (`am/aq.rs`): chamado em `am/hnsw_page.rs:674` (build) — não muda.
- `IvfflatIndex.lists` (`ann/ivf.rs`): campo privado; adiciono um accessor pub(crate) — sem caller externo hoje.

### Glossary

- **AVQ (anisotropic vector quantization):** quantizador que pondera o resíduo paralelo à direção do datapoint (loss de Guo 2020) — `am/aq.rs`.
- **AH-LUT (asymmetric hashing lookup table):** scoring por tabela pré-computada por query; `Σ_i LUT[i][code_i]` — `vec/ah.rs`.
- **batched-block scan:** escanear N códigos contíguos numa varredura SIMD (`pshufb`, 16 lookups/instrução) — o que falta.
- **inverted list:** conjunto de vetores atribuídos a um centroide IVF — `ivf.rs` `lists`.
- **nprobe:** nº de listas sondadas por query (trade-off recall×QPS).
- **rerank stage-2:** re-score full-precision dos survivors do lower-bound prune.

### Architecture boundaries

`rules/architecture.md §1`: domínio (`ann/`, `vec/`, `am/aq.rs`) NÃO importa `pg_sys`. O spike é 100% domínio (Rust
puro, testável sem banco) — nenhum `pg_sys`. Camada: `ann/ivf_aqah.rs` consome `ann/ivf.rs` + `am/aq.rs` + `vec/ah.rs`.

## Prior Art & Related Work

- Blueprint `.claude/knowledge-base/discoveries/blueprints/pg-scann-am-blueprint.md` (Corner 1 spike design, Corner 4 v4-layout, ADR-C gate).
- rabitq-rs `.claude/knowledge-base/references/rabitq-rs/src/simd.rs:1018-1110` (batched pshufb — padrão de estudo, Apache-2.0) + `ivf.rs:1945-2016` (scan 2 estágios).
- Nosso: `.claude/knowledge-base/discoveries/blueprints/m59-anisotropic-ah-blueprint.md` (AQ+AH design + gate D3), `docs/benchmarks/m33-scann-headtohead.md` (frontier ScaNN 1920 QPS @ 0.99).
- Harness ScaNN frontier: `benchmarks/run_m33_scann.py`.

## Dependency Graph

```
Phase 1 (batched kernel) ──▶ Phase 2 (layout + query path) ──▶ Phase 3 (harness + medição) ──▶ Phase 4 (integration validation)
   T1.1 oracle → T1.2 AVX2 → T1.3 bench       T2.1 accessor → T2.2 store → T2.3 query        T3.1 harness → T3.2 medição+veredito
```

## Phase 1 — Batched-block AH-LUT kernel (o SIMD que falta)

### T1.1 — Scalar batched-block oracle: `ah_score_block_scalar(lut, codes_block, n) -> Vec<i32>`

#### Why this step
Ação: escrever a versão escalar que scoreia N códigos contíguos (o oráculo de correção do SIMD). Raciocínio: por
`rules/testing.md` (correção antes de performance) e ADR-3, o kernel AVX2 só é aceito se casar bit-a-bit o ranking
de um oráculo escalar — que é `ah_score_scalar` aplicado em loop sobre o bloco. Sem o oráculo, o SIMD é
não-verificável.

#### TDD
```
test ah_block_scalar_matches_per_code_loop:  # RED
  given: um Lut16 e um bloco de 32 códigos aleatórios (seed fixo)
  when:  scores = ah_score_block_scalar(lut, block, 32)
  then:  scores[i] == ah_score_scalar(lut, block[i])  for all i   # ranking idêntico
```
Arquivo de teste: `theodb_rs/src/vec/ah.rs` (`#[cfg(test)] mod tests` co-localizado — convenção `rules/testing.md §5`).

#### Files to edit
- `theodb_rs/src/vec/ah.rs` (+ `ah_score_block_scalar` + teste)

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- `cargo test -p theodb_rs ah_block_scalar` GREEN; scores idênticos ao loop per-code para blocos de 1/16/32/33 códigos (edge: bloco parcial).
- Typed error se `codes_block.len() < n * bytes_per_code` (Rule 8).

#### DoD
- `cargo test -p theodb_rs ah_block_scalar` exit 0; sem `unwrap` em path não-teste.

### T1.2 — AVX2 `pshufb` batched-block kernel + runtime dispatch

#### Why this step
Ação: o kernel SIMD que scoreia o bloco contíguo com `_mm256_shuffle_epi8` (16 lookups/lane), com dispatch runtime
(`is_x86_feature_detected!`) e fallback ao oráculo escalar. Raciocínio: é o cerne do ganho de QPS (blueprint Corner
4, rabitq-rs `simd.rs:1018-1110`); o dispatch segue a forma dos kernels M58 existentes em `vec.rs` (`available()`/`force_for_test`).

#### TDD
```
test ah_block_avx2_matches_scalar_oracle:  # RED
  given: Lut16 + bloco de 32 códigos (seed)
  when:  simd = ah_score_block_avx2(lut, block, 32); scalar = ah_score_block_scalar(lut, block, 32)
  then:  simd == scalar   # ranking idêntico (o int8 requant preserva ranking, ah.rs:14)
test ah_block_dispatch_falls_back_when_forced_off:  # AVX2 forçado off → usa scalar
```

#### Files to edit
- `theodb_rs/src/vec/ah.rs` (+ `ah_score_block_avx2` sob `#[cfg(target_arch="x86_64")]` + dispatch `ah_score_block` + testes)

#### Concurrency tests
(none — single-threaded; o kernel é uma função pura sobre slices, sem estado compartilhado)

#### Acceptance criteria
- `ah_score_block_avx2` == `ah_score_block_scalar` em ≥1000 blocos aleatórios (seed) — ranking idêntico.
- Dispatch: `force_for_test(false)` → scalar; `available()` reflete `is_x86_feature_detected!("avx2")`.
- `#[target_feature(enable="avx2")]` isolado; sem UB (o teste roda sob dispatch).

#### DoD
- `cargo test -p theodb_rs ah_block` GREEN; `cargo clippy -p theodb_rs` clean no arquivo.

### T1.3 — Criterion micro-bench do batched kernel (same-graph, imune a ruído)

#### Why this step
Ação: bench criterion que mede LUT16-lookups/sec do batched vs per-code, same-graph (mesmo bloco). Raciocínio:
lição m46 (`goto-p0` / m46-measurement-learnings) — medir o kernel isolado (não A/B de containers) é o método
correto; o padrão já existe em `benches/scan_hot_path.rs` (`harness=false` + `#[path]`-include).

#### TDD
(bench não tem TDD de comportamento — a correção é garantida por T1.1/T1.2; o bench mede throughput. Marca a ausência: **bench-only, correção coberta por T1.1/T1.2**)

#### Files to edit
- `theodb_rs/benches/ah_block.rs` (NEW), `theodb_rs/Cargo.toml` (+ `[[bench]] name="ah_block" harness=false`)

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- `cargo bench -p theodb_rs --bench ah_block` roda e reporta ns/bloco para batched e per-code.

#### DoD
- Bench compila e roda; número reportado (não fabricado — do run local/droplet).

## Phase 2 — IVF-AQ contiguous layout + query path

### T2.1 — Accessor pub(crate) das inverted lists do IVF

#### Why this step
Ação: expor `IvfflatIndex::lists()` e o acesso aos vetores por lista (hoje `lists` é privado). Raciocínio: o layout
contíguo (T2.2) precisa iterar cada lista + seus vetores para encodar; parsimony (rung 4) — só um getter, sem
reescrever o IVF.

#### TDD
```
test ivf_lists_accessor_returns_partition:  # RED
  given: IvfflatIndex::build(corpus de 100 vetores, lists=8, ...)
  then:  index.lists().len() == 8; soma dos tamanhos == 100 (partição cobre tudo)
```

#### Files to edit
- `theodb_rs/src/ann/ivf.rs` (+ `pub(crate) fn lists()` + `pub(crate) fn vector(i)` ou similar + teste)

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- `index.lists()` retorna as N listas; cada índice mapeia a um `(id, vec)` válido; soma == n.

#### DoD
- `cargo test -p theodb_rs ivf_lists` GREEN.

### T2.2 — Store contíguo IVF-AQ: encodar + empacotar códigos por lista

#### Why this step
Ação: `IvfAqahIndex::build` — para cada inverted list, treinar/encodar via `AqQuantizer::encode` e empacotar os
códigos CONTÍGUOS (por lista), guardando o mapeamento código→id. Raciocínio: é o "v4 layout" do M59 (códigos
contíguos, separados do f32) que o batched-scan (T1.2) consome; blueprint ADR-A.

#### TDD
```
test ivf_aqah_build_packs_contiguous_codes:  # RED
  given: corpus 256 vetores dim=8 (m divisível), lists=4
  when:  idx = IvfAqahIndex::build(corpus, lists=4, m=4, bits=4, seed)
  then:  cada lista tem um bloco contíguo de |list|*bytes_per_code bytes; decode de um código == encode direto
test ivf_aqah_build_rejects_bad_dim:  # negative: dim % m != 0 → typed Err (Rule 8)
```

#### Files to edit
- `theodb_rs/src/ann/ivf_aqah.rs` (NEW: `IvfAqahIndex` struct + `build`), `theodb_rs/src/ann/mod.rs` (+ `mod ivf_aqah`)

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- Bloco contíguo por lista; `bytes_per_code` consistente com `AqQuantizer`; typed `Err` em dim inválida.
- Arquivo < 500 LoC (`rules/architecture.md`).

#### DoD
- `cargo test -p theodb_rs ivf_aqah_build` GREEN.

### T2.3 — Query path: probe → batched-block-scan → prune → rerank

#### Why this step
Ação: `IvfAqahIndex::search(q, k, nprobe, rerank_n)` — probe os nprobe centroides mais próximos (reusa
`ivf.centroids()`), batched-block-scan de cada lista (T1.2) → coletar candidatos → rerank top-N full-precision →
top-k. Raciocínio: é o scan de 2 estágios do ScaNN/rabitq (`ivf.rs:1945-2016`, blueprint Corner 4); o rerank
recupera recall alto (M80/Fase 5 embrionária, mas o spike precisa dele para medir @0.99).

#### TDD
```
test ivf_aqah_search_recall_vs_exact:  # RED
  given: corpus 2000 vetores dim=32 (seed), GT exato por brute-force
  when:  hits = IvfAqahIndex::search(q, k=10, nprobe=all_lists, rerank_n=100)
  then:  recall@10 >= 0.95 com nprobe=todas as listas + rerank (correção do pipeline; não é o número de perf)
test ivf_aqah_search_nprobe_monotonic:  # recall sobe com nprobe (edge: nprobe=1 vs nprobe=all)
```

#### Files to edit
- `theodb_rs/src/ann/ivf_aqah.rs` (+ `search` + testes)

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- Com nprobe=todas + rerank, recall@10 ≥ 0.95 (prova que o pipeline está correto); recall monotônico em nprobe.
- Sem `unwrap` em path não-teste; typed errors.

#### DoD
- `cargo test -p theodb_rs ivf_aqah_search` GREEN.

## Phase 3 — SIFT1M spike harness + medição + veredito D3

### T3.1 — Harness SIFT1M (example Rust): load fvecs → build → sweep nprobe → JSON

#### Why this step
Ação: `examples/m75_spike.rs` — carrega SIFT1M real (fvecs base+query + ivecs GT), constrói `IvfAqahIndex` +
um baseline f32 (brute-force ou `ivf.search`), faz sweep de nprobe medindo recall@10×QPS+p50/p95/p99 (≥3 runs),
emite JSON. Raciocínio: DoD do M75; espelha o formato do `bench_ivf_vs_mstg.rs` + reusa a semântica recall@10 do M33.

#### TDD
```
test m75_harness_recall_on_tiny_fixture:  # RED — usa um fixture pequeno (100 vetores) commitado, não SIFT1M
  given: fixture fvecs sintético determinístico
  when:  roda o pipeline do harness (build+search+recall)
  then:  emite JSON com campos {recall, qps, p50, p95, p99, nprobe} bem-formados
```
(o SIFT1M real roda no droplet em T3.2 — o teste valida a MECÂNICA do harness com fixture, não fabrica o número)

#### Files to edit
- `theodb_rs/examples/m75_spike.rs` (NEW) + um fixture pequeno em `theodb_rs/tests/fixtures/` (NEW), `theodb_rs/tests/m75_harness_test.rs` (NEW)

#### Failure scenarios
- fvecs truncado/corrompido → typed `Err` (não panic) — teste com arquivo truncado.
- SIFT1M ausente → mensagem clara de erro com o path esperado (Rule 8).

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- Harness roda no fixture, emite JSON bem-formado; erro claro se o dataset falta.

#### DoD
- `cargo test -p theodb_rs m75_harness` GREEN; `cargo run --example m75_spike -- --help` mostra o uso.

### T3.2 — Medição real (droplet) + `docs/benchmarks/m75-ivf-aqah-spike.{md,json}` + veredito D3

#### Why this step
Ação: rodar o harness em real SIFT1M num droplet (≥3 runs mean±std), rodar `run_m33_scann.py` para o frontier ScaNN,
consolidar recall×QPS (IVF-AQ+AH vs f32 vs ScaNN) + o micro-bench do kernel, escrever o doc + o veredito D3 explícito.
Raciocínio: DoD do M75; Rule 5 (número só de run real — NUNCA fabricar). O veredito determina M76-M82.

#### TDD
(não-código — é medição + escrita do veredito. A correção do pipeline foi provada em T2.3/T3.1; aqui produz-se o
NÚMERO honesto. Marca: **measurement task, correção coberta por Phase 1-2**)

#### Files to edit
- `docs/benchmarks/m75-ivf-aqah-spike.md` (NEW), `docs/benchmarks/m75-ivf-aqah-spike.json` (NEW), `docs/benchmarks/m75-raw/` (NEW)

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- Doc com tabela recall×QPS (IVF-AQ+AH vs f32 vs ScaNN), mean±std ≥3 runs, hardware+metodologia, reprodução.
- **Veredito D3 explícito:** GO (bate f32-@0.99 com margem material, proposta ~2× do ScaNN, effect>variance) / honest-partial / honest-negative — com a origem identificada.
- Zero número fabricado (todos de run real; raw em m75-raw/).

#### DoD
- Doc existe com o veredito; `docs/benchmarks/m75-ivf-aqah-spike.json` bem-formado; CHANGELOG `[Unreleased]` atualizado.

## Phase 4 — Integration Validation

### T4.1 — Suíte completa + gates

#### Why this step
Ação: rodar `cargo test -p theodb_rs` (toda a suíte), `cargo clippy`, `cargo bench --bench ah_block`, e confirmar o
artefato medido. Raciocínio: "eat your own cooking" — o plano não fecha sem a cadeia verde + o número honesto.

#### Concurrency tests
(none — single-threaded; funções puras sobre slices, sem estado compartilhado)

#### Acceptance criteria
- `cargo test -p theodb_rs` GREEN (sem regressão dos pg_tests existentes); clippy clean; o doc de benchmark existe com veredito.
- `/code-quality` sem FAIL_HARD (sem símbolo fabricado / dead code não-allowlisted).

#### DoD
- Todos os gates verdes; o veredito D3 registrado.

## Coverage Matrix

| Requisito (DoD M75 / blueprint) | Task(s) |
|---|---|
| Batched-block AH-LUT kernel (o que falta) | T1.1, T1.2, T1.3 |
| Layout contíguo IVF-AQ (v4 layout) | T2.1, T2.2 |
| Query path probe→scan→prune→rerank | T2.3 |
| Harness SIFT1M recall×QPS ≥3 runs | T3.1, T3.2 |
| Micro-bench criterion do kernel | T1.3 |
| Veredito D3 GO/honest-negative + doc | T3.2 |
| Integração/gates | T4.1 |

**Coverage: 7/7 requisitos mapeados (100%).**

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| O batched kernel pode não bater o gap (honest-negative) | ALTA | É o propósito do spike (measurement-first); honest-negative fecha o pilar em M73, sem retrabalho no AM | paulo |
| int8 requant do AH degrada recall a ponto de precisar de rerank caro | MÉDIA | rerank_n como parâmetro; medir o trade-off recall×QPS; o bound do AVQ ajuda | paulo |
| SIFT1M no droplet (download/mem) | BAIXA | reusar o padrão dos droplets M45/M72 (fvecs), c-8 16GB comporta 1M×128 | paulo |
| O spike in-memory não reflete o custo de página do AM real | MÉDIA | honesto no doc: o spike mede o ALGORITMO; o custo de página/WAL é medido em M76+ (declarado como caveat) | paulo |

## Unresolved Questions

- O limiar exato do GATE ("~2× do ScaNN") não está em blueprint local — a formalizar neste plano como critério explícito de T3.2 (reconhecido honestamente como proposta, hoje UNBENCHMARKED).
- `m`/`bits` ótimos do AVQ para SIFT1M (128d): T3.2 pode varrer 1-2 configs; não é bloqueante para o veredito GO/no-go.

## Global Definition of Done

- [ ] Todas as tasks com TDD RED→GREEN→REFACTOR; testes co-localizados (`rules/testing.md §5`).
- [ ] `cargo test -p theodb_rs` GREEN (sem regressão); `cargo clippy` clean; arquivos < 500 LoC (`rules/architecture.md`).
- [ ] Batched kernel com oráculo escalar (correção antes de performance); dispatch runtime.
- [ ] Medição REAL em SIFT1M (droplet), ≥3 runs mean±std → `docs/benchmarks/m75-ivf-aqah-spike.{md,json}` — ZERO número fabricado (Rule 5).
- [ ] Veredito D3 explícito (GO/honest-partial/honest-negative) com origem identificada.
- [ ] `/code-quality` sem FAIL_HARD; CHANGELOG `[Unreleased]` atualizado (Rule 6).
- [ ] Wiring triad: o `IvfAqahIndex` é exercido pelo harness (caller) + testes de integração (T2.3/T3.1) + o número medido (métrica).

## Final Phase: Integration Validation

T4.1 é a validação de integração: a cadeia completa (test+clippy+bench+artefato medido) verde, o veredito D3
registrado. O plano NÃO fecha sem o número honesto — se o pipeline falhar ou o dataset não medir, o plano falhou
(não se fabrica o veredito).
