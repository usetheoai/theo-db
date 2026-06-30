# Blueprint: M21 — Own ANN index (HNSW + IVFFlat) in Rust

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS (89.0, weighted 99.7; sole caveat: citation-density metric — the
> blueprint is densely cited in absolute terms). Scored 2026-06-30 by /discover-confidence. Synthesized 2026-06-30 from the m21-own-ann-index
> discovery plan (v1.1, SHIPPABLE_WITH_CAVEATS 89). Sources: pgvector (C — the recall@k parity target),
> pgvectorscale (Rust/pgrx DiskANN AM — scaffolding template), vectorchord (Rust/pgrx RaBitQ AM — second
> scaffolding datapoint). **Migration decision: COEXISTENCE, measurement-first** (see ADR D1/D3).

**Slug:** `m21-own-ann-index`
**Owner:** paulohenriquevn
**Created:** 2026-06-30

## Context

M21 (`ROADMAP-v2.md:116`) requires an **own ANN index access method (HNSW + IVFFlat) in Rust**, substituting
pgvector's index **only** when recall@k parity is proven on the existing harness — else an honest ADR keeps
pgvector (anti-sunk-cost, `ROADMAP-v2.md:124`). Risk is tagged ALTO/PhD-level; measurement-first is the guard-rail.
M20 shipped own f32-parity distance functions in coexistence with pgvector's type
(`.claude/knowledge-base/reviews/m20-own-vector-type-review-2026-06-30.md`) and deferred the index AM + opclass to
M21. This blueprint is the prior-art investigation that must precede any code (Unbreakable Rule 9; TheoDB rule 1).

The recall harness already exists and is reused, not rebuilt: `benchmarks/theodb_bench/recall.py:61`
(`recall_at_k`), `:41` (`brute_force_ground_truth`), `benchmarks/theodb_bench/harness.py:29` (`run_benchmark`).

## Objective

Decide whether and how TheoDB builds an own HNSW + IVFFlat index AM in Rust that reaches pgvector recall@k parity,
so the M21 implementation plan is evidence-backed and the parity gate is defined before any code is written.
**Decision reached:** build the own AM with its OWN index pages, **coexisting** with pgvector (separate `CREATE
INDEX … USING theodb_hnsw`), and gate substitution on a measured recall@k tolerance band — keeping "retain pgvector"
a valid outcome.

## Coverage Corner 1 — Integration Tests

How the Rust-AM references test their index, so M21 mirrors the shape and reuses `theodb_bench` for the parity gate.

**pgvectorscale** — distance-thresholded recall test inside the AM build module: `CREATE INDEX … USING diskann
(embedding {opclass}) WITH (…)`, then `ORDER BY embedding {op} $1 LIMIT k` with `enable_seqscan=0,
enable_indexscan=1`, comparing the index result to a brute-force sequential scan and asserting `matches > 9` for
k=10 (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/build.rs:1232` index DDL,
`:1260`-`:1323` query, `:1363`-`:1396` recall check). Tests are `#[pg_test]` (run under `cargo pgrx test`).

**vectorchord** — SQL logic test (`sqllogictest`): `CREATE INDEX idx1 ON t USING vchordrq (val vector_l2_ops)`
then `… ORDER BY val <-> '[…]' LIMIT 10`, recall asserted via a `vchordrq_evaluate_query_recall(...)` UDF that
compares the indexed result against an exhaustive scan and expects `1`
(`.claude/knowledge-base/references/vectorchord/tests/vchordrq/recall.slt:9,36-38`).

**M21 reuse of the existing TheoDB harness (no rebuild):** the harness is distance-agnostic. The parity gate:
1. Load a corpus + queries; build `CREATE INDEX … USING theodb_hnsw (embed vector_l2_ops)`.
2. `gt_idx, gt_dist = brute_force_ground_truth(corpus, queries, k=10, metric='l2')` (`recall.py:41`).
3. Per query: collect the own-AM `ORDER BY embed <-> q LIMIT 10` distances → `run_dists`.
4. `recall = recall_at_k(gt_dist, run_dists, k=10, eps=1e-3)` (`recall.py:61`).
5. Run the SAME harness for a pgvector hnsw index → compare recall curves at matched `ef_search`; the gate is
   `recall_theodb >= recall_pgvector − tolerance` across the ef_search sweep (`harness.py:29` orchestrates).

## Coverage Corner 2 — Dependencies

What an own Rust AM pulls in beyond pgrx + std, with licenses (TheoDB D1 forbids AGPL — only Apache/MIT/BSD/PG).

| Dependency | Version (ref) | License | Purpose | Citation |
|---|---|---|---|---|
| pgrx | 0.16.1 (pgvectorscale) / 0.17.0 (vectorchord) | Apache-2.0/MIT | PG extension framework | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:31`; `.claude/knowledge-base/references/vectorchord/Cargo.toml:43` |
| simdeez | 1.0.8 | MIT/Apache-2.0 | SIMD distance | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:33` |
| rand / rand_chacha | 0.8 / 0.10.1 | MIT/Apache-2.0 | RNG (HNSW layer assignment, IVF sampling) | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:34`; `.claude/knowledge-base/references/vectorchord/Cargo.toml:73` |
| dary_heap | 0.3.9 | MIT/Apache-2.0 | Candidate/result heaps for graph traversal | `.claude/knowledge-base/references/vectorchord/Cargo.toml:40` |
| half | 2.7.1 | MIT/Apache-2.0 | float16 (optional quantization — M22) | `.claude/knowledge-base/references/vectorchord/crates/simd/Cargo.toml:20` |
| rkyv / zerocopy | 0.7.43 / 0.8.48 | MIT/Apache-2.0 | zero-copy on-page struct layout | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:32`; `.claude/knowledge-base/references/vectorchord/Cargo.toml:53` |
| rayon | 1.12.0 | MIT/Apache-2.0 | parallel build (optional) | `.claude/knowledge-base/references/vectorchord/Cargo.toml:46` |

**No AGPL/GPL found** in either project's manifests — all Apache-2.0 / MIT (D1-clean). **M21 parsimony stance
(Rule 9 / parsimony-ladder):** the minimal own HNSW/IVFFlat needs only `pgrx` + `std` + the M20 `vec.rs` f32
distance (rung 4 — reuse what is installed) + at most a small RNG and a binary-heap (std `BinaryHeap` suffices →
rung 2). SIMD/quantization crates are **deferred** (perf, not correctness — M22). pgrx stays at **0.16.1** (the
theodb_rs current version), not 0.17.

## Coverage Corner 3 — Tools

The pgrx index-AM scaffolding: the `IndexAmRoutine` wiring + own-page storage (Q3) and the registration SQL (Q4).

### IndexAmRoutine hooks (Q3) — pgvectorscale `access_method/mod.rs` fills these

| Hook | Rust fn | Role | Citation |
|---|---|---|---|
| `ambuild` | `build::ambuild` | build index from heap | `.../pgvectorscale/pgvectorscale/src/access_method/mod.rs:71`, `build.rs:296` |
| `ambuildempty` | `build::ambuildempty` | empty (unlogged) build | `…/mod.rs:72` |
| `aminsert` | `build::aminsert` | insert one tuple | `…/mod.rs:73`, `build.rs:464` |
| `ambulkdelete` / `amvacuumcleanup` | `vacuum::*` | vacuum | `…/mod.rs:74-75` |
| `amcostestimate` | `cost_estimate::amcostestimate` | planner cost | `…/mod.rs:76` |
| `amoptions` | `options::amoptions` | reloptions (M, ef_construction, lists) | `…/mod.rs:77` |
| `ambeginscan` / `amrescan` / `amgettuple` / `amendscan` | `scan::*` | scan lifecycle | `…/mod.rs:78-82`, `scan.rs:309/336/370/439` |

**Storage model — OWN index pages (not heap).** pgvectorscale allocates its own pages on the **index relation**
via `ReadBufferExtended(index, InvalidBlockNumber)` under exclusive lock, `PageInit`, and a custom page-special
struct `TsvPageOpaqueData` (magic `0xAE24`), WAL-logged with `GenericXLogStart/Finish`
(`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/util/page.rs:16-22,64-80,110-136,227-231`;
`util/buffer.rs:56-67`). **Coexistence is therefore proven:** each `CREATE INDEX` owns an isolated index relation
(its own OID + page space); a `USING theodb_hnsw` index and a pgvector `USING hnsw` index on the same column do not
collide (PG isolates buffers/WAL by relation OID). The own AM never touches pgvector's index or the table data.

**Unsafe/FFI surface:** buffer I/O (`ReadBufferExtended`/`LockBuffer`/`UnlockReleaseBuffer`), WAL
(`GenericXLog*`), page ops (`PageInit`/`PageAddItemExtended`), memory (`palloc0`/`pfree`), relation-extension locks
— all wrapped in `unsafe` at precise boundaries; `#[pg_guard]` / `extern "C-unwind"` prevent Rust panics unwinding
into C (`…/pgvectorscale/.../util/buffer.rs:78-87,158-166`; `…/page.rs:113-130`; vectorchord
`.../vectorchord/src/index/vchordrq/am/mod.rs:246-247`).

### Registration SQL (Q4)

1. **amhandler fn** — `#[pg_extern(sql = "CREATE OR REPLACE FUNCTION diskann_amhandler(internal) RETURNS
   index_am_handler PARALLEL SAFE IMMUTABLE STRICT … LANGUAGE c AS '@MODULE_PATHNAME@','@FUNCTION_NAME@';")] fn
   amhandler(_fcinfo) -> PgBox<pg_sys::IndexAmRoutine>` (`.../pgvectorscale/pgvectorscale/src/access_method/mod.rs:27-92`).
   vectorchord uses a `const AM_HANDLER: IndexAmRoutine` written via `palloc0` in `_vchordrq_amhandler`
   (`.../vectorchord/src/index/vchordrq/am/mod.rs:192-199`; SQL `.../vectorchord/sql/install/vchord--1.1.0.sql:983-984`).
2. **CREATE ACCESS METHOD** — `CREATE ACCESS METHOD diskann TYPE INDEX HANDLER diskann_amhandler;`
   (`.../pgvectorscale/pgvectorscale/src/access_method/mod.rs:40`; vectorchord `vchord--1.1.0.sql:1070`).
3. **CREATE OPERATOR CLASS** — binds the distance operator as ORDER BY strategy 1:
   `CREATE OPERATOR CLASS vector_cosine_ops DEFAULT FOR TYPE vector USING diskann AS OPERATOR 1 <=> (vector,vector)
   FOR ORDER BY float_ops, FUNCTION 1 distance_type_cosine();`
   (`.../pgvectorscale/pgvectorscale/src/access_method/mod.rs:207-211`; vectorchord l2 variant
   `.../vectorchord/sql/install/vchord--1.1.0.sql:1106-1110`). M21 needs three opclasses: `<->` (l2), `<#>` (ip),
   `<=>` (cosine), each `OPERATOR 1 … FOR ORDER BY`.

Build/install: `cargo pgrx`; SQL emitted via `#[pg_extern(sql=…)]` + `extension_sql!`
(`.../pgvectorscale/pgvectorscale/src/access_method/mod.rs:166`).

## Coverage Corner 4 — Techniques

The HNSW (Q1) + IVFFlat (Q2) algorithms (pgvector, the parity target) and the recall determinants (Q5).

### HNSW (Q1) — pgvector

- **Params:** `M`=16 default (range 2-100, `.claude/knowledge-base/references/pgvector/src/hnsw.h:50`,
  `hnsw.c:88`), `ef_construction`=64 (range 4-1000, `hnsw.h:53-55`, `hnsw.c:90`).
- **Layer assignment:** random `level = (int)(-log(RandomDouble()) * ml)`, `ml = 1/log(M)`
  (`.claude/knowledge-base/references/pgvector/src/hnswutils.c:115,249`).
- **Entry point:** highest-level element; replaced when a higher-level element is inserted
  (`.claude/knowledge-base/references/pgvector/src/hnswbuild.c:430`).
- **Search layer:** two pairing heaps — candidates (min-dist) + results (max-dist), with a visited set
  (`.claude/knowledge-base/references/pgvector/src/hnswutils.c:826-849`); neighbor pruning via the "closer"
  heuristic `SelectNeighbors` (`hnswutils.c:1063-1163`).
- **Scan:** greedy descent ef=1 on upper layers, then `HnswSearchLayer(ef = hnsw_ef_search)` at layer 0
  (`.claude/knowledge-base/references/pgvector/src/hnswscan.c:25-56`).

```
build:  for tuple: level=rand_level(M); neighbors=search+SelectNeighbors(ef_construction,M) per layer;
        link bidirectionally; update entry point if level>entry.level
search: ep=entry; for lc=entry.level..1: ep=greedy(ep,ef=1,lc); W=SearchLayer(ep,ef=ef_search,0); return W sorted
```

### IVFFlat (Q2) — pgvector

- **Build:** kmeans++ init + Lloyd iterations (`.claude/knowledge-base/references/pgvector/src/ivfkmeans.c:19-88`)
  on a sample; `lists`=100 default (range 1-32768, `ivfflat.h:51-53`, `ivfflat.c:42`); each tuple assigned to its
  nearest centroid list (`.claude/knowledge-base/references/pgvector/src/ivfbuild.c:182-192`).
- **Scan:** `probes`=1 default (range 1-32768, `ivfflat.h:54`, `ivfflat.c:45`); select the `probes` closest
  centroid lists, scan their tuples, sort by distance (`.claude/knowledge-base/references/pgvector/src/ivfscan.c:133-135,173,182`).

```
build:  centers = kmeans++(sample, lists); for tuple: assign argmin dist(tuple,center); tuplesort by listId
search: lists = top-`probes` argmin dist(query,center); scan tuples in those lists; tuplesort by dist; return
```

### Recall determinants (Q5)

- **HNSW:** `hnsw_ef_search`=40 default (range 1-1000, PGC_USERSET,
  `.claude/knowledge-base/references/pgvector/src/hnsw.h:56-58`, `hnsw.c:93-95`); optional `hnsw_iterative_scan`
  (off/relaxed/strict, `hnsw.h:146-151`, `hnswscan.c:313-319`). Higher ef_search → higher recall + latency.
- **IVFFlat:** `ivfflat_probes`=1 default (`ivfflat.c:45`); higher probes → higher recall + latency.
- **Parity is a TOLERANCE BAND, not bit-exact.** HNSW recall is build-order-dependent (random layer assignment
  `hnswutils.c:249`; entry-point churn `hnswbuild.c:430`; heuristic pruning `hnswutils.c:1063-1163`). Two builds
  with the same params but different insert order return different neighbor sets. Parity ⇒ `recall@k` within `eps`
  of pgvector at matched `ef_search`/`probes` over a sweep — reusing `recall.py`'s eps semantics — NOT identical
  result sets (mirrors M20 ADR D3 tolerance lesson).

## Cross-cutting Comparison

| Dimension | pgvector (C) | pgvectorscale (Rust) | vectorchord (Rust) | M21 implication |
|---|---|---|---|---|
| ANN family | HNSW + IVFFlat | DiskANN + SBQ | RaBitQ + graph-quant | Port HNSW+IVF **algorithm** from pgvector; borrow **AM scaffolding** from the Rust two |
| AM wiring | C `IndexAmRoutine` (`hnsw.c:271`) | `amhandler`→PgBox (`mod.rs:45`) | `const AM_HANDLER` (`vchordrq/am/mod.rs`) | Use pgvectorscale's `#[pg_extern(sql=…)]`+PgBox pattern (cleaner) |
| Storage | own index pages | own index pages (`page.rs`) | own index pages | Own pages ⇒ **coexistence** with pgvector index proven |
| Distance | f32 accum (`vector.c`) | simdeez | internal simd crate | Reuse M20 `theodb_rs/src/vec.rs` f32-parity math (no new dep) |
| Recall knob | ef_search / probes | search_list_size | probes/epsilon | Mirror ef_search/probes; gate via tolerance band |
| Test shape | regression SQL | `#[pg_test]` recall vs brute-force | sqllogictest recall UDF | `#[pg_test]` + reuse `theodb_bench` parity gate |
| Deps | C/PG only | pgrx, simdeez, rand, rkyv (MIT/Apache) | pgrx, rand, dary_heap, half… (MIT/Apache) | All D1-clean; M21 minimal = pgrx+std+vec.rs |

## ADRs

### D1 — COEXISTENCE: own AM as a separate index, never replacing pgvector's storage/operators

**Decision:** Implement the own AM (`theodb_hnsw` / `theodb_ivfflat`) so it stores its graph/lists on its **own
index relation pages** and is created via a separate `CREATE INDEX … USING theodb_hnsw`. It does NOT redefine
pgvector's `vector` type, its operators, or its `hnsw`/`ivfflat` AMs. Existing pgvector indexes + `theodb.embed/
hybrid/import` keep working untouched.

**Rationale:** the storage evidence (`util/page.rs`, `buffer.rs`) shows a Rust AM owns isolated index pages keyed
by relation OID — coexistence is mechanically free and risk-free, and it is the only path that respects
measurement-first (substitute only when proven). Mirrors M20's coexistence decision.

**Alternatives considered:** (a) replace pgvector's `hnsw` AM in place — rejected (breaks existing indexes + embed/
hybrid/import; violates anti-sunk-cost before any measurement). (b) reuse pgvector's index pages — rejected
(undocumented binary coupling, fragile).

**Consequences:** users opt in per index; A/B recall comparison is trivial (both indexes coexist on the same
column); substitution is a later, evidence-gated decision.

### D2 — Distance reuse + recall parity as a tolerance band

**Decision:** the own AM computes distances with M20's `theodb_rs/src/vec.rs` f32-parity functions (no new
distance code, no SIMD dep in M21). Recall@k parity is defined as `recall_theodb >= recall_pgvector − tolerance`
across an `ef_search`/`probes` sweep on `theodb_bench`, NOT bit-exact neighbor-set identity.

**Rationale:** Rule 9 (don't reinvent — reuse M20); HNSW is build-order non-deterministic (Q5 evidence) so a
band is the only physically correct contract.

**Alternatives considered:** new SIMD distance (rejected — perf, M22, YAGNI now); exact-match recall (rejected —
physically impossible across builds).

**Consequences:** the parity gate is a benchmark sweep with a documented tolerance; the M20 distance is the
shared kernel.

### D3 — Measurement-first scope + anti-sunk-cost fallback (the M21 DoD)

**Decision:** M21 delivers (a) a functional own `theodb_hnsw` + `theodb_ivfflat` AM that BUILDS and answers
`<->`/`<#>`/`<=>` ORDER-BY queries, and (b) a reproducible recall@k + latency benchmark vs pgvector in
`docs/benchmarks/`. If parity is reached → recommend own AM (opt-in, coexisting). If NOT → an honest ADR keeps
pgvector and M21 still ships the measurement + the AM behind a GUC/flag. WAL-crash-safety completeness, parallel
build, and full vacuum are flagged as **durability hardening deferred** (noted, not the M21 acceptance bar) — the
measurement-first milestone targets correctness + recall, not production-grade durability (honest per Rule 3).

**Rationale:** the DoD (`ROADMAP-v2.md:124`) explicitly makes the milestone deliver the *measurement*, not a
forced substitution; a full WAL/vacuum-hardened AM at parity is multi-month (pgvector hnsw ≈ 3k LOC C), so scoping
the milestone to functional+recall-gated is honest, and the anti-sunk-cost fallback is built into the DoD.

**Alternatives considered:** attempt a fully production-hardened AM in one milestone (rejected — high risk of not
finishing; violates "100% functional" honesty); skip M21 and keep pgvector (rejected — the milestone's value is the
measurement).

**Consequences:** the M21 plan's acceptance criterion is the benchmark gate; "retain pgvector" is a first-class
valid outcome; durability hardening becomes a follow-up.

### D4 — pgrx 0.16.1 + `#[pg_extern(sql=…)]` PgBox handler pattern

**Decision:** stay on pgrx 0.16.1 (theodb_rs current) and follow pgvectorscale's `#[pg_extern(sql=…)]` +
`PgBox<IndexAmRoutine>` registration pattern (cleaner than vectorchord's raw `palloc0`+const), emitting the
`CREATE ACCESS METHOD` + opclass SQL via `extension_sql!`.

**Rationale:** consistency with the existing theodb_rs crate (Rule: KISS, no version churn); pgvectorscale's
pattern is the most idiomatic pgrx index-AM template (Q4 evidence).

**Alternatives considered:** bump to 0.17 like vectorchord (rejected — gratuitous churn, M21 needs no 0.17 API);
vectorchord const-handler (rejected — more unsafe, less idiomatic).

**Consequences:** the AM handler lives in theodb_rs at the current pgrx version; opclasses bind `<->`/`<#>`/`<=>`
as ORDER BY strategy 1.

## Recommendations

1. **Build own `theodb_hnsw` + `theodb_ivfflat` index AMs in Rust (coexistence), per ADR D1** — own index pages,
   separate `CREATE INDEX … USING`, zero touch to pgvector/embed/hybrid/import. (feeds Q3/Q4)
2. **Reuse M20 `vec.rs` f32 distance; gate recall@k as a tolerance band on `theodb_bench`, per ADR D2** — no new
   distance code, no SIMD dep in M21. (feeds Q1/Q2/Q5/Q7)
3. **Scope measurement-first with anti-sunk-cost fallback, per ADR D3** — deliver functional AM + reproducible
   benchmark; "retain pgvector" is a valid honest outcome; durability hardening deferred. (feeds the DoD)
4. **Minimal deps (parsimony): pgrx 0.16.1 + std (`BinaryHeap`) + small RNG only; defer SIMD/quantization to M22,
   per ADR D4 + Corner 2** — all candidate deps are D1-clean if later needed.
5. **Mirror the `#[pg_test]` recall-vs-brute-force test shape (pgvectorscale) and wire the existing
   `theodb_bench` harness as the gate** — no harness rebuild. (feeds Q7)

## Blocked questions (if any)

(none — all 7 questions answered with citations.)
