# Discovery Plan: Product Quantization (PQ) as the vector-superiority lever

> **Version 1.0** — Investigate Product Quantization (PQ) — the k-means subspace-codebook quantizer with asymmetric distance computation (ADC) via lookup tables — as the algorithmic lever to close TheoDB's vector-superiority claim vs AlloyDB/ScaNN. Cross-reference how ScaNN/FAISS (primary literature) and the cloned permissive peers pgvectorscale (SBQ) and vectorchord (RaBitQ) implement scan-time quantized distance, and how to integrate PQ+ADC as a distance path in our `theodb_ivfflat`/`theodb_hnsw` index-AM alongside the existing `sbq.rs`. The blueprint decides: is PQ worth building (recall×QPS×memory), and if so, what is the minimal integration shape.

**Slug:** `m39-pq-product-quantization`
**Owner:** paulohenriquevn
**Created:** 2026-07-03
**milestone_id:** M39
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

TheoDB's North Star is "equal or superior to AlloyDB" (`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`), with the P0 GOTO being **vector-superiority** — today only recall-parity is proven, not the latency/QPS-at-scale win. The M38 measurement (`docs/benchmarks/m38-copy-free-scan.md`) established two honest facts: (a) our existing **SBQ regresses recall** (0.77–0.95 < 1.0 on real SIFT), so it is not the path to a superiority claim at fixed recall; and (b) the scan copy is not the end-to-end bottleneck. M38 explicitly named **PQ (product quantization) + LUT ADC** — the technique ScaNN and FAISS use — as the real remaining algorithmic lever. This discovery gathers the SOTA evidence required BEFORE building (measurement-first, anti-sunk-cost; `CLAUDE.md § Esforço ≠ Complexidade`), honoring the phd-rigor profile (`.claude/rules/discover-phd-rigor.md`, R1–R6: SOTA-anchored, ≥2 primary sources per technique, benchmark evidence or `UNBENCHMARKED`). Any borrowed pattern must respect our index-AM boundaries (`.claude/rules/architecture.md`) and the std-only, permissive-license posture of `theodb_rs/src/sbq.rs` (Apache/MIT/BSD only — D1; no AGPL).

## Objective

Produce a blueprint that lets us decide **whether to build PQ+ADC for TheoDB, and in what minimal shape**, so that a subsequent `/to-plan` can implement a benchmark-gated PQ distance path. Success criteria:

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/` or allowlisted primary sources
- [ ] Cross-cutting comparison table populated (PQ vs SBQ vs RaBitQ vs full-precision: recall, QPS, bytes/vector, build cost)
- [ ] Recommendations section gives at least one concrete decision proposal per research question, incl. a go/no-go on PQ with the benchmark trigger (PRD D3 fork/build discipline)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/sbq/`, `pgvectorscale/src/access_method/distance/`, `pgvectorscale/benches/` | Permissive peer that integrates a quantizer (SBQ) as a scan-time distance path in a PG index-AM — the exact integration shape PQ must mirror |
| `.claude/knowledge-base/references/vectorchord/` | `crates/index/src/` | Permissive peer using RaBitQ quantization + reranking — SOTA alternative to PQ; informs the tradeoff table |
| Primary literature (allowlisted WebFetch) | arxiv.org, dl.acm.org, ieeexplore.ieee.org, research.google, github.com | PQ algorithm (Jégou et al. 2011) + ScaNN anisotropic quantization (Guo et al. 2020) + FAISS PQ source — the SOTA anchor (phd-rigor R1/R2/R5) |
| `theodb_rs/src/` (our own code, read-only for discovery) | `sbq.rs`, `am/scan.rs`, `vec.rs` | The existing SBQ + the exact scan-time distance integration point PQ+ADC would extend |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/*/target/`, `docs/`, `.github/` | Build artifacts / marketing / CI config — not algorithm source of truth |
| FAISS / ScaNN full source clone | Not cloned into references; investigated via primary papers + the projects' public GitHub (allowlisted WebFetch), never claimed from memory |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a project feature without reading its source |
| Implementing PQ | This is discovery — output is a blueprint, not code (`cycle-discover` anti-pattern) |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvectorscale: 2h; vectorchord: 1.5h; primary literature (PQ/ScaNN/FAISS): 2h; our own `sbq.rs`/`scan.rs` cross-read: 0.5h.

**Rationale:** pgvectorscale is the closest structural analog (Rust + pgrx + PG index-AM + a scan-time quantizer at `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs`), so it gets the deepest dive for the integration shape. Primary literature gets a large slice because the phd-rigor profile (`.claude/rules/discover-phd-rigor.md` R1/R2) requires ≥2 primary sources for the PQ technique and SOTA anchoring against ScaNN — the algorithm's correctness lives in the papers, not in a peer's code. vectorchord is a SOTA cross-check (RaBitQ at `.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs`), not the primary path.

**Alternatives considered:** equal split (rejected — pgvectorscale/literature deserve more), single-source deep dive (rejected — phd-rigor R2 needs ≥2 primary sources per technique).

**Stop condition — per question (mandatory):** When a question's Fase A returns empty matches after 3 consecutive retries with different query variants, mark BLOCKED with reason "Fase A exhausted"; continue. Do NOT pad with unrelated hotspots.

**Stop condition — per project (mandatory):** When a project's time budget is exhausted with N questions pending, mark them BLOCKED with reason "budget exhausted"; continue with the next. If every remaining question is `done`/`blocked`, emit `<promise>BLUEPRINT_BLOCKED</promise>` with the honest report — never `BLUEPRINT_COMPLETE` from a blocked state.

**Anti-pattern:** NEVER fabricate Fase B answers to close a question whose Fase A was exhausted (Unbreakable Rule 3). A paywalled paper → BLOCKED with reason, becomes a next-discovery seed (phd-rigor R6).

**Consequences:** the halt-loop surfaces blocked questions in `## Blocked questions` — next-discovery seed.

### D2 — Investigation depth

**Decision:** For the algorithm (techniques corner), read the primary paper's method section end-to-end + the peer's encode/distance functions line-by-line. For deps/tools/tests, Grep/Glob + read the matched file in context (not end-to-end).

**Rationale:** PQ correctness (codebook training, ADC LUT construction, distance reconstruction) is subtle — a shallow grep would miss the anisotropic-loss detail that distinguishes ScaNN from vanilla PQ. The integration shape is read line-by-line in the peers' encode/distance code (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/distance/mod.rs`, `.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs`) and cross-read against our own scan-distance call site (`theodb_rs/src/am/scan.rs`) and existing quantizer (`theodb_rs/src/sbq.rs`). Deps/tools/tests are structural and don't need full reads.

**Consequences:** deeper token spend on techniques; bounded elsewhere. Honors phd-rigor R4 (techniques corner earns deeper interrogation).

### D3 — Primary-source citation discipline (phd-rigor R2/R3/R5)

**Decision:** Every performance number in the blueprint carries methodology + source, or the literal marker `UNBENCHMARKED`. External sources restricted to `.claude/rules/discover-web-allowlist.txt`. Each technique claim cites ≥2 independent primary sources.

**Rationale:** `CLAUDE.md § TheoDB rule 5` — performance is a claim, not an opinion; `public-copy.md` §4/§5. A PQ "recall" number without its dataset (SIFT1M / GIST1M / DEEP1B) and code-size is meaningless.

**Consequences:** blueprint prose is citation-dense; unbenchmarked claims are explicitly flagged as next-benchmark seeds, not asserted.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — grep/ast map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | What exactly is the PQ algorithm — how are subspace k-means codebooks trained, and how is asymmetric distance (ADC) computed via a per-query lookup table? | techniques | Primary literature (Jégou et al. 2011, arxiv/IEEE) | WebFetch the PQ paper (allowlisted `arxiv.org` / `ieeexplore.ieee.org`); locate the "product quantizer" + "asymmetric distance computation" sections | Read the method section end-to-end; capture codebook count `m`, centroids `k*` per subspace, LUT construction, distance reconstruction formula | Algorithm description + math (codebook train, ADC LUT, distance) + ≥2 primary citations |
| Q2 | How does ScaNN improve vanilla PQ (anisotropic/score-aware loss), what recall×QPS does it publish, and how does that position PQ vs SBQ vs RaBitQ vs full-precision (the tradeoff table)? | techniques | Primary literature (Guo et al. 2020, ScaNN; `research.google`) + `docs/benchmarks/m38-copy-free-scan.md` + peers | WebFetch the ScaNN paper + `research.google` blog (allowlisted); locate the anisotropic-loss + benchmark sections; aggregate the recall numbers from Q1/Q3 + our M38 SBQ recall (`docs/benchmarks/m38-copy-free-scan.md`) | Read the loss derivation + the published recall@k × QPS table (dataset named); cross-tabulate each method's recall/QPS/bytes/build | Delta vs vanilla PQ + a comparison table (rows PQ/SBQ/RaBitQ/full-precision; cols recall@10, QPS, bytes/vec, build) with per-cell citation or `UNBENCHMARKED`; ≥2 primary citations |
| Q3 | How do the permissive peers integrate a scan-time quantizer into a Rust PG index-AM — pgvectorscale's SBQ encode+distance and vectorchord's RaBitQ encode+rerank — the exact integration shape PQ must mirror? | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs`, `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/distance/mod.rs`, `.claude/knowledge-base/references/vectorchord/crates/index/src/accessor.rs` | Grep `quantize`/`distance`/`Distance` in pgvectorscale's `sbq/mod.rs` + `distance/mod.rs`; Grep `rabitq`/`quant`/`rerank`/`Accessor` in vectorchord's `accessor.rs` | Read the encode + scan-time distance/rerank functions in both peers; capture where in the AM lifecycle the quantized code is stored + how distance is computed at scan | Integration-shape description for BOTH peers + how RaBitQ (bits) differs from PQ (codebooks) + `file:line` citations per encode/distance |
| Q4 | How does our existing SBQ get tested (encode/decode + recall gate) and wired into the scan — the test+integration pattern PQ must follow, and the exact `scan.rs` distance call site a PQ ADC path would branch at? | tests | `theodb_rs/src/sbq.rs`, `theodb_rs/src/am/scan.rs`, `theodb_rs/src/vec.rs` | Grep `#[pg_test]`/`#[test]`/`fn test`/`hamming`/`quantize` in `theodb_rs/src/sbq.rs`; Grep the L2 distance call site (`l2_dist_from_bytes`) in `theodb_rs/src/am/scan.rs` | Read the SBQ tests + the `theodb_rs/src/am/scan.rs` distance call site where a PQ ADC path would branch | Test-pattern description + the exact `theodb_rs/src/am/scan.rs` integration point (`file:line`) for a PQ distance branch |
| Q5 | Can PQ be implemented std-only in Rust (like our `theodb_rs/src/sbq.rs`) or does it need a k-means crate — and what is the license posture (Apache/MIT/BSD gate, D1)? | deps | `theodb_rs/src/sbq.rs`; `.claude/knowledge-base/references/vectorchord/Cargo.toml`; `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs` | Grep `k-means`/`kmeans`/`Lloyd`/`rand`/`ndarray` in `vectorchord/Cargo.toml` + pgvectorscale sbq; read `theodb_rs/src/sbq.rs` `train()` to see our std-only precedent | Read each dep match in context; capture whether k-means is hand-rolled (std) or a crate + its license | Dep decision: std-only feasible? + any candidate crate + license (Apache/MIT/BSD gate, D1) |
| Q6 | How do these projects benchmark quantization recall×QPS reproducibly — the harness PQ's benefit must be proven against? | tools | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/distance.rs`; `docs/benchmarks/m38-copy-free-scan.md` | Glob `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/*.rs`; Grep `recall`/`qps`/`Criterion` in benches; read our M38 harness (`docs/benchmarks/m38-copy-free-scan.md`) | Read the bench harness setup (dataset, N, warm/cold, metric) | Reproducible-harness description + how our `benchmarks/` would measure PQ recall×QPS vs SBQ |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q4 | Covered |
| Dependencies | Q5 | Covered |
| Tools | Q6 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

**Coverage: 4/4 corners covered (100%)**

Question budget: 6 questions total (phd-rigor window 6–14 ✅); techniques corner has 3 (≥2 required ✅, ≤3 checker max ✅); every corner ≥1 ✅. (Consolidated from an initial 8 — the ScaNN SOTA + tradeoff table folded into Q2, and both permissive peers' integration shapes folded into Q3 — to respect the `check_plan_completeness.py` per-corner ceiling while keeping every claim.)

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | Each `.claude/knowledge-base/references/{project}/{path}` declared in Fase A exists | Mark Qx BLOCKED "path not found", continue |
| Before answering Q1/Q2 (literature) | WebFetch target host ∈ `discover-web-allowlist.txt` | Mark BLOCKED "source outside allowlist"; do not cite |
| Per-question Fase A budget | Fase A returned ≥1 hotspot OR 3 retries attempted | After 3 retries, mark BLOCKED "Fase A exhausted" |
| After answering Qx | Blueprint section under Qx has ≥1 citation (path or allowlisted URL) | Re-iterate Qx (1 retry max) |
| Perf-claim discipline (D3) | Every recall/QPS number has methodology+source OR `UNBENCHMARKED` | Add the marker; never assert a bare number |
| Per-project time budget | Budget not exhausted | When exhausted, BLOCK remaining Qx for that project; advance |
| Before promising complete | All 4 coverage corners have populated sections + ≥1 ADR | Refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All 6 research questions answered OR explicitly BLOCKED with reason
- [ ] Every citation resolves on disk (a real path under `theodb_rs/src/` or the cloned references) or is an allowlisted primary-source URL
- [ ] All 4 coverage corners populated in the blueprint
- [ ] Techniques claims (Q1 PQ, Q2 ScaNN, Q3 peers) each carry ≥2 primary sources (phd-rigor R2)
- [ ] Every performance number carries methodology+source or `UNBENCHMARKED` (phd-rigor R3)
- [ ] Comparison table (Q2) populated with per-cell citation or `UNBENCHMARKED`
- [ ] ≥1 ADR in the blueprint proposing go/no-go on PQ with the benchmark trigger

## Global Definition of Done

- `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS per `.claude/rules/discover-blueprint-golden-rule.md`
- No fabricated citation (hard cap `fabricated_citation`), no empty coverage corner (hard caps `empty_corner_*`)
- phd-rigor R1–R6 honored (`.claude/rules/discover-phd-rigor.md`)
