# Blueprint: pg_scann — ScaNN (IVF + AVQ + AH) como Access Method PostgreSQL

> **/discover-confidence verdict:** SHIPPABLE_WITH_CAVEATS (89 · weighted_avg 99.7 · coverage 100% · 0 citação fabricada · caveat: soft-floor de densidade de citação, paths abreviados)
> **Slug:** `pg-scann-am` · **Created:** 2026-07-10 · **Owner:** paulohenriquevn
> **Fontes:** referências locais (rabitq-rs, vectorchord, pgvector) + prior-art nosso (m59/m33) + web (R0: AVQ paper
> arXiv:1908.10396, AlloyDB blog+whitepaper, arXiv:2603.23710 SIGMOD 2026 — VERIFICADO).

## Context

Este discovery foi disparado pela decisão do owner (2026-07-10) de perseguir o pilar de superioridade de QPS
vetorial (North Star, `docs/adr/0002`) via um pg_scann — **depois** do veredito medido M73/ADR-0035 (v0.65.0) que
fechou a superioridade "pelos levers já tentados" (SBQ/M57, AQ+AH-no-HNSW/M59, RaBitQ-1bit/M74), mas onde o próprio
M59/ADR-0019 apontou o carrier IVF batch-scan com layout contíguo como a peça NÃO-REFUTADA e deferida. A pesquisa
externa (arXiv:2603.23710, SIGMOD 2026) reforça a plausibilidade. Motivação medida: gap ScaNN ~25× @ recall 0.99
(`docs/benchmarks/m33-scann-headtohead.md`).

## Objective

Produzir o design grounded do pg_scann (layout + scan + lifecycle + planner) suficiente para (a) desenhar o spike
D3 de viabilidade e (b) planejar o AM completo se o spike validar — ancorado no SOTA (ScaNN/AlloyDB) e nos peers
permissivos, com honestidade sobre a hipótese não-provada.

## Sumário executivo

O TheoDB já tem o **algoritmo** do ScaNN own-code (AVQ em `theodb_rs/src/am/aq.rs`, AH-LUT16 em
`theodb_rs/src/vec/ah.rs`, IVF em `theodb_rs/src/ann/ivf.rs`) validado por correção. O que falta — e o alvo do
pg_scann — é a **integração de banco**: (a) layout de página com códigos AQ contíguos por inverted list; (b) scan
path IVF-AQ+AH (probe leaves → batch-scan LUT); (c) lifecycle transacional (INSERT/VACUUM/WAL); (d) planner. Este
blueprint fornece o design grounded, com o **modelo de referência direto** (rabitq-rs para o layout+scan; pgvector
para o contrato AM+WAL; vectorchord para o pending/frozen lifecycle — este só ESTUDO, é AGPL) e o **design do spike
D3** que mede a hipótese antes de construir o AM completo.

**Honestidade central (Regra 3):** o AQ+AH sobre carrier IVF batch-scan é a **hipótese NÃO-REFUTADA** que o M59
apontou — **não uma vitória garantida**. Os números de AQ+AH no nosso stack são **UNBENCHMARKED**; o valor do spike
é produzir o primeiro número honesto (sustentar OU refutar). A justificativa externa (arXiv:2603.23710 SIGMOD 2026)
mostra que cluster-indexes tipo ScaNN PODEM superar grafos em Postgres real — plausibilidade, não prova para o nosso caso.

## SOTA anchoring (R1) — ScaNN/AlloyDB vs o que o pg_scann fecha

- **ScaNN (algoritmo do AlloyDB):** AVQ (anisotropic loss penaliza o resíduo paralelo à direção do datapoint mais
  que o perpendicular — arXiv:1908.10396, confirmado no abstract) + AH-LUT SIMD + partition + rerank. Gap medido
  (M33): ~25× QPS sobre theodb_ivfflat full-precision @ recall 0.99 (`docs/benchmarks/m33-scann-headtohead.md`).
- **AlloyDB claims (blog):** 4× query / 8× build / 3-4× menos memória / 10× write throughput vs pgvector HNSW —
  mas o **mecanismo de update transacional NÃO é documentado** (blog E whitepaper deferem os detalhes). ⇒ o design
  de lifecycle do pg_scann é decisão nossa, ancorada no peer permissivo-de-estudo (vectorchord).
- **arXiv:2603.23710 (SIGMOD 2026, VERIFICADO):** "the optimal algorithm is not dictated by the cost of distance
  computations alone, but that system-level overheads (page accesses, data retrieval, filter checks) play a
  significant role" — grafos "incur prohibitive numbers of filter checks and system-level overheads compared with
  clustering-based indexes such as ScaNN". **É a justificativa técnica externa do pg_scann.**

## Coverage Corner 1 — Integration Tests

### O design do spike D3 de viabilidade (Q7) — o gate measurement-first antes do AM completo

O spike mede o **scan IVF-AQ+AH do TheoDB** vs o frontier ScaNN, espelhando o formato recall×QPS do peer
`.claude/knowledge-base/references/rabitq-rs/examples/bench_ivf_vs_mstg.rs` (carrega train/queries/GT de fvecs/ivecs
`:196-217`; `recall = |gt∩ret|/k` `:266-273`; warmup 10 queries `:288-290`; sweep de params `:319-334`) e reusando
`benchmarks/run_m33_scann.py` como comparador-frontier (o harness M33 já mede recall@10+QPS+p50/p95/p99+build+RSS
num sweep de `num_leaves_to_search`, por `.claude/knowledge-base/discoveries/blueprints/m33-scann-headtohead-blueprint.md`).

| Eixo | Escolha (matched com M33) |
|---|---|
| Dataset | **real SIFT1M** 1M×128 Euclidean (nunca sintético) |
| Queries / GT | subsample seeded 1000 (seed 42), mesma neighbors-GT, recall@10 distance-thresholded |
| Sistemas | pg_scann (IVF-AQ+AH), f32 HNSW baseline, SBQ, **ScaNN** re-rodado no mesmo subsample |
| Sweep | pg_scann: `nprobe`; ScaNN: `num_leaves_to_search` — frontier inteira |
| Métricas | recall, QPS, p50/p95/p99, build-time, peak-RSS, bytes/index |
| Rigor | **≥3 runs, mean±std, effect > variance** (`analysis-golden-rule §A1`) + criterion micro-bench do kernel LUT16 (same-graph, imune a ruído de box — lição m46) + `THEODB_SCAN_PROFILE=1` (AH move `score` E `reads`) |

**Gate de decisão D3 (measurement-first, anti-sunk-cost):** o gate DOCUMENTADO (m59 blueprint) é "IVF-AQ+AH bate o
f32 HNSW baseline em QPS @ recall ≥ 0.99, effect > variance". **Honesto:** o limiar "~2× do ScaNN" que motivou o
projeto **NÃO está nos blueprints locais** — proponho formalizá-lo no `/to-plan` do spike como critério explícito
("IVF-AQ+AH dentro de ~2× do ScaNN em QPS @ recall≥0.99 ⇒ justifica o AM dedicado"), reconhecendo que hoje é
UNBENCHMARKED. Saídas honestas: bate f32-@0.99 com margem material → GO para o AM; bate SBQ mas não f32 → honest
partial; não fecha fração material do 25× → honest-negative + seed (DiskANN disk-resident + SOAR).

## Coverage Corner 2 — Dependencies

### Licenças e o que é reusável sob D1 (Q6)

| Projeto | Licença | Sob D1 (Apache/MIT/BSD/PG) | Ação |
|---|---|---|---|
| **rabitq-rs** | **Apache-2.0** (`.claude/knowledge-base/references/rabitq-rs/Cargo.toml`, `.../rabitq-rs/LICENSE`) | ✅ vendorizável | Já vendorizado (`theodb_rs/src/rabitq/vendor/`); preservar LICENSE+NOTICE+versão |
| **vectorchord** | **AGPLv3-or-ELv2** (`.claude/knowledge-base/references/vectorchord/LICENSE`, header em todo arquivo) | ❌ copyleft | **SÓ ESTUDAR** o design; reimplementar do zero (`[[vectorchord-agpl-study-only]]`) |
| pgvector | PostgreSQL License | ✅ | Reimplementar a partir dele (contrato AM) |

Deps de runtime do rabitq-rs (`.claude/knowledge-base/references/rabitq-rs/Cargo.toml`): matrixmultiply, rayon, rand,
thiserror, roaring, libc, serde/bincode, memmap2, parking_lot, half, hnsw_rs — todas MIT/Apache no ecossistema Rust
(features `python`/`pyo3` optional → desligar no vendor; parquet/arrow são dev-only). **Honesto:** a verificação
SPDX autoritativa das transitivas roda via `cargo deny`/`/deps-audit` no gate de release (D1/PRD §11), não por
inspeção manual. Nota: a própria `deny.toml` do vectorchord só permite Apache/MIT/BSD/ISC — ou seja, a barreira D1
é sobre o **código do vectorchord**, não sobre a árvore de deps dele (que nós podemos usar independentemente).

## Coverage Corner 3 — Tools

### Contrato IndexAmRoutine + page format + WAL (Q4) — reimplementar a partir do pgvector (permissivo)

Tabela de callbacks (`.claude/knowledge-base/references/pgvector/src/ivfflat.c`):

| Callback | path:line | Responsabilidade | WAL |
|---|---|---|---|
| `ivfflatbuild` | `ivfflat.c:215` | k-means → assign → sort → load | `GenericXLog` por sub-passo; `log_newpage_range` no init fork (`ivfbuild.c:1046-1048`) |
| `aminsert` | `ivfflat.c:217` | insere na lista mais próxima | `GenericXLog` por página (`ivfinsert.c:124-176`) |
| `ambulkdelete` | `ivfflat.c:219` | remove mortas por callback | `GenericXLog`+`PageIndexMultiDelete` (`ivfvacuum.c:86-119`) |
| `amvacuumcleanup` | `ivfflat.c:220` | atualiza stats | nenhum (`ivfvacuum.c:148-164`) |
| `amcostestimate` | `ivfflat.c:222` | custo ∝ `probes/lists` | read-only (`ivfflat.c:85-151`) |
| `ambeginscan/amgettuple/amendscan` | `ivfflat.c:229-233` | scan: probes → tuplesort drain | read-only (`ivfscan.c:252-424`) |

**Page topology (payload-neutra — o achado-chave):** metapage (blkno 0, magic+version, `ivfflat.h:234-240`) →
**list pages** = diretório de centroids (`IvfflatListData{startPage,insertPage,center}`, `ivfflat.h:253-258`,
criadas em `ivfbuild.c:504-547`) → **cadeias de entry pages por lista** (`InsertTuples`, `ivfbuild.c:271-331`). É
exatamente onde o pg_scann troca `IndexTuple(vetor f32)` por **códigos AQ contíguos** — a topologia não muda.
Disciplina WAL encapsulada em `ivfutils.c` (`IvfflatInitRegisterPage`=`GenericXLogStart`+`RegisterBuffer(FULL_IMAGE)`+`PageInit`
`:153-159`; `IvfflatAppendPage` encadeia na mesma txn WAL `:176-198`; guarda insert↔vacuum `:247-256`).
**Format-change gate:** bump de `IVFFLAT_MAGIC_NUMBER`/`VERSION` (`ivfflat.h:42-43`) = BREAKING + REINDEX + CHANGELOG.

### VACUUM + região pending (Q5)

- **pgvector: SEM região pending** — inserts vão direto na cauda da lista; `ivfflatbulkdelete` segue a cadeia
  `startPage→nextblkno` e aplica `PageIndexMultiDelete` sob `GenericXLog` (`ivfvacuum.c:18-143`, `LockBufferForCleanup`
  `:84`). Baseline simples, mas a lista NÃO é imutável entre vacuums.
- **vectorchord: modelo appendable/frozen (o que queremos)** — cada folha tem 3 tapes: `frozen` (códigos empacotados,
  imutável) + **`appendable` (região pending append-only)**. INSERT = append no pending
  (`.claude/knowledge-base/references/vectorchord/crates/vchordrq/src/insert.rs:190-211`); DELETE = tombstone lógico
  `payload=None` (`.../bulkdelete.rs:87-93`); VACUUM cleanup = `maintain()` reempacota pending→frozen e libera páginas
  (`.../maintain.rs:203-250`). **Design para ESTUDO (AGPL), reimplementar.**

## Coverage Corner 4 — Techniques

### O "v4 layout": códigos AQ contíguos por partition + batch-scan AH-LUT (Q1) — modelo direto: rabitq-rs (Apache-2.0)

**Layout de bytes por cluster** (`.claude/knowledge-base/references/rabitq-rs/src/ivf.rs:185-192`): um bloco contíguo
`batch_data: Vec<u8>` concatenando batches de 32 vetores; `batch_stride = padded_dim*32/8 (codes 4-bit packed) +
4*32 (f_add) + 4*32 (f_rescale) + 4*32 (f_error)` (`ivf.rs:218-222`). Dentro do batch: codes-block primeiro, depois
3 arrays SoA de params (zero-copy por offset, `ivf.rs:236-287`). Os ex-codes (refinamento) ficam per-vetor fora do
bloco (`ivf.rs:194-199`), desempacotados só para survivors.

**Packing SIMD** (`.../rabitq-rs/src/simd.rs:864-904`, `KPERM0` `:774`): transpõe vetor→coluna, split nibble
alto/baixo (4-bit codes), empacota via permutação `[0,8,1,9,…]` que alimenta o `pshufb`.

**Kernel batch-scan** (`.../rabitq-rs/src/simd.rs:1018-1110` AVX2, `:1117-1158` AVX-512): LUT int8 do query
(`fastscan.rs:26-73`, requant `delta=(vr−vl)/255`), `_mm256_shuffle_epi8(lut, code_nibble)` = 16 lookups/instrução,
acumula epi16; decode `est_dist = f_add + f_rescale*(lut_delta*accu + …)` + `lower_bound = est − f_error*g_error`
(`simd.rs:2053-2060`). **Scan de 2 estágios** (`.../rabitq-rs/src/ivf.rs:1945-2016`): FastScan int8 barato →
lower-bound prune (`:2011-2016`) → ex-code rerank só dos survivors (`fastscan_kernel.rs:124-153`). É a
materialização exata da hipótese "IVF batch-scan contíguo" do M59.

### Arquitetura domínio/adapter (Q2) — o padrão reusável (livre de licença) do vectorchord

O vectorchord separa: **domínio puro sem `pg_sys`** (`.claude/knowledge-base/references/vectorchord/crates/index/src/relation.rs:25-92`
— `Page/PageGuard/RelationRead/RelationWrite` como traits; algoritmo IVF genérico sobre `R: RelationRead`, testável
sem banco) vs **adapter com todo o `pg_sys`** (`.../vectorchord/src/index/storage.rs` implementa as traits com buffers
reais; `.../src/index/vchordrq/am/mod.rs` os hooks `extern "C-unwind"`). **A joia (Q2d): RAII WAL guard**
(`storage.rs:216-260`) — `Drop` faz `if panicking() { GenericXLogAbort }` else `GenericXLogFinish`, sempre
`UnlockReleaseBuffer` → fecha "sem buffer leak em nenhum path" + "panic-across-C → WAL descartado" via RAII. Page
layout `#[repr(C,align(8))]==BLCKSZ` com asserts const-eval + bounds-check-antes-de-fatiar (`storage.rs:30-146`).
Opclass resolvida por reflexão de catálogo (`opclass.rs:301-360`). **Padrão para ESTUDO (AGPL), reimplementar** —
mas ancora exatamente o nosso `rules/architecture.md §1` (domínio sem pg_sys) + o invariante "no panic across C".

### Update transacional do AlloyDB (Q3) — honest: não-documentado; adotamos o modelo vectorchord

O blog AlloyDB (cloud.google.com) e o whitepaper (services.google.com PDF) ambos **não documentam** o mecanismo
de update incremental (só offline build + query; "10× write throughput" sem mecanismo). ⇒ o design do pg_scann para
INSERT-entre-retrains adota o **appendable/frozen** do vectorchord (Corner 3 Q5): pending append-only por lista +
`maintain()` reempacota no frozen no vacuum-cleanup. Decisão nossa, grounded no peer permissivo-de-estudo, não
copiada do AlloyDB proprietário.

## Prior Art (R2 — perfil PhD-rigor)

- **Nosso (medido/decidido):** M36 falsificou "distância é o gargalo" (candidate-count + O(C·logC) sort dominam —
  `docs/benchmarks/m59-anisotropic-ah.md` citando m36). M57/ADR-0018 falsificou bit-quant escalar (SBQ = 0.35-0.77×
  do QPS f32). M59/ADR-0019 implementou AQ+AH (175 pg_tests GREEN) e concluiu que o ganho exige **carrier IVF
  batch-scan contíguo**, não HNSW pointer-chasing — mas **os números AQ+AH no nosso stack são UNBENCHMARKED** (M59 é
  design; a layout-sensitivity é a Unresolved Question aberta). ⇒ "AQ+AH não ganha no HNSW" é hipótese de risco
  fundamentada, não resultado medido. Blueprints: `.claude/knowledge-base/discoveries/blueprints/m59-anisotropic-ah-blueprint.md`,
  `.../m33-scann-headtohead-blueprint.md`.
- **Externo (SOTA):** arXiv:1908.10396 (AVQ — anisotropic loss); arXiv:2603.23710 SIGMOD 2026 (cluster-indexes
  superam grafos em Postgres real por overheads de sistema — a justificativa do pg_scann).

## Recommendations

Proposta de decisão concreta por eixo de pesquisa (para o `/to-plan`):

1. **(Q7/Corner-tests) Arrancar por um SPIKE measurement-first, não pelo AM completo.** Fase 0 = medir o scan
   IVF-AQ+AH (reusando `ann/ivf.rs` + `am/aq.rs` + `vec/ah.rs` num layout de códigos contíguos) vs f32 baseline +
   ScaNN em real SIFT1M, ≥3 runs. Gate: bater f32-@0.99 com margem material (proposta ~2× do ScaNN — formalizar).
2. **(Q1/Q4/Corner-techniques+tools) Layout "v4" = topologia IVF do pgvector (payload-neutra) + byte-layout
   contíguo do rabitq-rs** (`ivf.rs:185-222`, Apache-2.0 vendorizável). Trocar `IndexTuple(f32)` por códigos AQ
   4-bit empacotados em batches de 32.
3. **(Q3/Q5/Corner-techniques+tools) Lifecycle = modelo appendable/frozen** (estudo do vectorchord, reimplementado
   do zero — AGPL): INSERT→pending append-only; DELETE→tombstone lógico; VACUUM→`maintain()` reempacota. O AlloyDB
   não documenta o mecanismo; este é o análogo permissivo.
4. **(Q2/Corner-techniques) Arquitetura = domínio Rust puro sem `pg_sys` (scorer/probe testável sem banco) +
   adapter pgrx com RAII WAL guard** (Drop→panic?abort:finish) — ancora `rules/architecture.md §1` e o invariante
   "no panic across C".
5. **(Q6/Corner-deps) Só rabitq-rs é vendorizável (Apache-2.0); vectorchord é AGPL → só design.** Rodar
   `/deps-audit` no gate de release para o SPDX autoritativo das transitivas.
6. **Roadmap sugerido (7 fases + Fase 0):** F0 spike D3 → F1 AM scaffold (já temos) → F2 partition/train IVF →
   F3 wire AVQ (`am/aq.rs`) no layout contíguo → F4 scan AH-LUT → F5 rerank → F6 lifecycle INSERT/VACUUM/WAL →
   F7 planner (amcostestimate). Cada fase gated por medição (recall-neutro + sem regressão).

## ADRs (decisões de design sintetizadas)

### D1 — (ADR-A) Layout "v4": inverted lists com códigos AQ contíguos (topologia pgvector + byte-layout rabitq-rs)
**Decisão:** reimplementar o índice IVF do pgvector (metapage → list pages/centroids → entry pages) trocando o
payload da entry page de `IndexTuple(f32)` por **códigos AQ empacotados 4-bit contíguos em batches de 32** (o layout
`batch_stride` do rabitq-rs `ivf.rs:218-222`). **Rationale:** o M59 identificou o layout contíguo como causa
primária; a topologia pgvector é payload-neutra (Q4); o byte-layout rabitq-rs é Apache-2.0 vendorizável. Alternativa
rejeitada: manter no carrier HNSW (M59 apontou o pointer-chasing como o limite; Rule 9 — reusa `ann/ivf.rs`).

### D2 — (ADR-B) Lifecycle transacional: modelo appendable/frozen (estudo do vectorchord, reimplementado)
**Decisão:** cada lista = `frozen (AQ imutável) + appendable (pending append-only)`; INSERT→append pending;
DELETE→tombstone lógico; VACUUM cleanup→`maintain()` reempacota. **Rationale:** pgvector não tem pending; AlloyDB
não documenta; vectorchord é o único análogo (AGPL→estudar, reimplementar do zero). Alternativa rejeitada: rebuild
total por insert (inviável a 1M+). D1: nenhum código AGPL entra.

### D3 — (ADR-C) Gate D3 measurement-first: medir antes de construir o AM completo
**Decisão:** Fase 0 = spike (Corner 1) medindo IVF-AQ+AH vs f32 baseline + ScaNN, ≥3 runs mean±std, real SIFT1M.
GO para o AM só se bater f32-@0.99 com margem material (proposta: dentro de ~2× do ScaNN — a formalizar, hoje
UNBENCHMARKED). **Rationale:** anti-sunk-cost/D3; a hipótese é fundamentada mas não-provada; honest-negative é saída
válida. Alternativa rejeitada: construir o AM inteiro assumindo o ganho (viola measurement-first).

### D4 — (ADR-D) Arquitetura domínio/adapter (padrão vectorchord, livre de licença) + RAII WAL guard
**Decisão:** scorer/probe/quantizador IVF num módulo Rust puro sem `pg_sys` (atrás de trait `RelationRead`-like),
adapter pgrx com RAII guard (`Drop`→panic?abort:finish). **Rationale:** ancora `rules/architecture.md §1` + o
invariante "no panic across C"; testável sem banco (nosso padrão M35). Padrão é técnica (livre); reimplementar.

## Blocked / UNVERIFIED questions

- Nenhuma questão BLOCKED. Fontes web todas resolveram. **UNVERIFIED marcados honestamente:** o mecanismo exato de
  update do AlloyDB (blog+whitepaper deferem); o limiar "~2× ScaNN" (não está em blueprint local — a formalizar);
  os números AQ+AH-no-nosso-stack (UNBENCHMARKED — o spike os produz); SPDX das transitivas do rabitq-rs (via
  `/deps-audit` no release).

## Cross-cutting comparison

| Dimensão | ScaNN/AlloyDB | vectorchord (AGPL, estudo) | rabitq-rs (Apache, vendor) | pgvector (PG, reimpl) | pg_scann (proposto) |
|---|---|---|---|---|---|
| Carrier | tree/partition | IVF (RaBitQ) | IVF (RaBitQ) | IVF/HNSW (f32) | **IVF (AQ own-code)** |
| Quantização | AVQ anisotrópica | RaBitQ 1-bit | RaBitQ 1-bit | nenhuma | **AVQ (`am/aq.rs`)** |
| Scan | AH-LUT | FastScan LUT | FastScan LUT (`simd.rs`) | full-precision | **AH-LUT16 (`vec/ah.rs`)** |
| Layout códigos | contíguo (proprietário) | frozen tape | batch contíguo (`ivf.rs:185`) | IndexTuple f32 | **contíguo (ADR-A)** |
| Pending/update | não-documentado | appendable tape | mmap batch | cauda da lista | **appendable (ADR-B)** |
| WAL/pgrx | AlloyDB engine | RAII guard | n/a (lib) | GenericXLog | **RAII guard (ADR-D)** |
| Licença | proprietário | AGPL | Apache-2.0 | PG | Apache/PG (own-code) |
