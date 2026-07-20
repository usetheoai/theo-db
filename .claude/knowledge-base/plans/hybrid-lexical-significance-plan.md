---
slug: hybrid-lexical-significance
milestone_id: M125
created_at: 2026-07-20
goal: Measure hybrid vs vector significance on a lexical-favoring BEIR set and isolate the ts_rank leg's contribution to resolve H6
---

# Plan — M125 Hybrid significance on a lexical-heavy set (resolve H6)

## Goal

Run the M123 paired-significance test on a lexical-favoring BEIR dataset (NFCorpus) and add the fts-leg's own
per-query significance so the report separates *fusion value* from *ts_rank-leg quality* — converting the AI-native
hybrid claim from AT_RISK into a measured verdict (significant lift OR honest parity with the confound named).

**Single metric:** `run_m53_hybrid_beir.py --dataset nfcorpus` emits a `significance` block for hybrid-vs-vector AND
hybrid-vs-fts (nDCG@10, permutation p + bootstrap CI + wins/losses/ties), validated by a deterministic unit test.

## Context

Consumes `.claude/knowledge-base/discoveries/blueprints/hybrid-lexical-significance-blueprint.md`. M123 measured
PARITY on SciFact (dense-strong). The blueprint (BEIR Table 3) shows the hybrid value-prop only shows on
lexical-favoring sets, and warns of a confound: TheoDB's shipped lexical leg is `ts_rank_cd` (`hybrid.rs:42`), not
BM25 — so a null result is ambiguous unless the fts leg's own contribution is measured. The harness already runs a
`fts` retriever (`hybrid.py` `_RETRIEVERS`) and emits its per-query array — M125 just adds the fts-vs comparisons.

## Baseline Context

Repo state: git sha `02a2f8b`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `benchmarks/run_m53_hybrid_beir.py` | 205 | `_paired_sig` computes hybrid-vs-vector only | Add hybrid-vs-fts + fts-vs-vector significance blocks. |
| `benchmarks/theodb_bench/test_significance.py` | 79 | 8 tests | Add a test for the multi-comparison `_paired_sig`. |
| `docs/benchmarks/m125-hybrid-lexical.md` | 0 | (NEW) | The measured NFCorpus report + the ts_rank confound + honest verdict. |

### Current callers / dependents (verified `file:line`)

- `benchmarks/run_m53_hybrid_beir.py:76` `_paired_sig(per_query)` — currently reads `per_query["hybrid"]` +
  `per_query["vector"]`; extend to also read `per_query["fts"]`.
- `benchmarks/theodb_bench/hybrid.py` `_RETRIEVERS = ("vector", "fts", "hybrid")` — the fts per-query nDCG@10 array
  already flows via `return_per_query=True`.
- `benchmarks/theodb_bench/significance.py::paired_significance` — reused unchanged.
- `benchmarks/theodb_bench/beir.py:91` downloads any BEIR `{name}.zip` — `nfcorpus` needs no code change.

### Domain glossary

- **fts leg** — TheoDB's lexical retriever using Postgres `ts_rank_cd` (not BM25); one of the three retrievers.
- **fusion value vs FTS-leg quality** — whether a hybrid parity means fusion doesn't help, or the ts_rank leg is
  too weak; measured by comparing hybrid-vs-fts and fts-vs-vector.
- **NFCorpus** — a small (3.6K docs / 323 queries) medical BEIR set where BM25 ≈ dense (a fusion-synergy probe).

### Architecture boundaries affected

`benchmarks/` is a dev/CI harness (not shipped). The significance module is pure computation over arrays (no
DB/network), unit-testable offline; the numbers come from the harness's existing DB+embedding boundary. No new dep.

## Prior Art & Related Work

- Blueprint (web-evidenced): BEIR (Thakur 2021) per-dataset BM25-vs-dense; RRF (Cormack 2009, k=60); hybrid
  complementarity (Bruch 2023). `docs/benchmarks/m123-hybrid-significance.md` (the SciFact PARITY this extends).

## ADRs

### ADR M125-1 — measure the fts leg's contribution, not just hybrid-vs-vector

**Decision:** report three paired comparisons — hybrid-vs-vector (primary), hybrid-vs-fts, fts-vs-vector — so a
parity result can be attributed (fusion adds nothing vs the ts_rank leg is too weak).

**Rationale (cites blueprint + Rule 3 honesty):** the blueprint names the ts_rank≠BM25 confound as load-bearing; a
single hybrid-vs-vector number repeats M123's ambiguity. The fts leg's nDCG + hybrid-vs-fts disambiguate without
needing the heavy `pg_textsearch` BM25 extension.

**Alternatives rejected:**
- **Install pg_textsearch (true BM25 leg) and A/B ts_rank vs bm25** — REJECTED for this milestone: requires a
  separate throwaway BM25 PG image (heavy build). Deferred as a follow-up if the ts_rank leg is proven the
  bottleneck (honest, named in the report), rather than silently shipping an ambiguous null.
- **Run Touché-2020 (strongest lexical set)** — REJECTED for this milestone: its 382K-doc corpus OpenAI embed is a
  long/costly run impractical for a CI benchmark; NFCorpus (3.6K) is the feasible measurable. Touché flagged as the
  follow-up with a dedicated embed budget.

### ADR M125-2 — pre-declared endpoint, honest-negative accepted

**Decision:** primary endpoint nDCG@10 on NFCorpus, pre-declared; a lift is claimed only when p<0.05 AND the CI
excludes 0; otherwise report parity + the confound. Same anti-p-hack contract as M123 (ADR M123-2).

**Alternatives rejected:** dataset-shopping for a significant result — REJECTED (blueprint anti-p-hack).

## Dependencies

No new dependency (numpy present; scipy optional as in M123). `## Dependencies`: **none added** — verified against
`benchmarks/requirements.txt`.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| hybrid-vs-vector + hybrid-vs-fts + fts-vs-vector significance on NFCorpus | T1 (multi-comparison `_paired_sig`) |
| deterministic correctness of the multi-comparison helper | T2 (unit test) |
| honest measured verdict + ts_rank confound named | T3 (real run + report) |

## Phase 1 — multi-comparison significance

### T1.1 — extend `_paired_sig` to report the three comparisons

#### Why this step
A parity result must be attributable. Reasoning: reusing `paired_significance` unchanged, compute hybrid-vs-vector
(primary), hybrid-vs-fts, and fts-vs-vector from the per-query arrays the harness already produces, so the report
can say whether fusion helps or the ts_rank leg is the bottleneck.

#### Files to edit
- `benchmarks/run_m53_hybrid_beir.py` — `_paired_sig` returns `{hybrid_vs_vector, hybrid_vs_fts, fts_vs_vector}`
  each = `paired_significance(a, b)` over qid-aligned nDCG@10; the main printout shows all three + the fts leg's
  mean nDCG@10 (the "is the lexical leg even alive on this set" signal).

#### TDD
- RED: `test_paired_sig_three_comparisons` — a stubbed `per_query` with hybrid/vector/fts arrays yields a dict with
  all three comparison keys, each carrying `p_permutation`/`ci95_low`/`wins`; asserts qid-alignment across all three.
- GREEN: implement the three-way comparison.
- REFACTOR: one alignment helper shared by the three.

#### Concurrency tests
(none — single-threaded) — pure array computation, no shared state, no threads.

#### Acceptance criteria
- `_paired_sig` output has `hybrid_vs_vector`, `hybrid_vs_fts`, `fts_vs_vector`, each a full significance dict.
- `None` when per-query data absent (older path).

#### DoD
- Unit test green.

## Phase 2 — deterministic correctness

### T2.1 — unit test for the multi-comparison helper

#### Why this step
Prove the three-way helper is correct before the real run. Reasoning: synthetic per_query arrays with known
relations (hybrid>vector, fts<vector) must produce the expected signs deterministically.

#### Files to edit
- `benchmarks/theodb_bench/test_significance.py` (extend).

#### TDD
- RED then GREEN: `test_three_comparisons_signs` — hybrid = vector + 0.1, fts = vector − 0.2 → hybrid_vs_vector
  mean>0/all-wins, fts_vs_vector mean<0/all-losses, hybrid_vs_fts mean>0. Deterministic (fixed seed).
- REFACTOR: parametrize.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria / DoD
- `pytest benchmarks/theodb_bench/test_significance.py` green.

## Phase 3 — real measurement + report

### T3.1 — run NFCorpus + write the honest report

#### Why this step
The DoD's measured verdict. Reasoning: run the harness on NFCorpus (OPENAI_API_KEY from `.env`), capture the three
comparisons, and report honestly — significant hybrid lift OR parity with the ts_rank confound named + the
follow-up (true BM25 leg / Touché).

#### Files to edit
- `docs/benchmarks/m125-hybrid-lexical.md` (NEW) — measured NFCorpus report + the ts_rank≠BM25 confound + the
  NFCorpus license flag (CI-internal only) + the honest verdict + the deferred follow-ups (bm25 leg, Touché).

#### TDD
- RED: the report must cite real NFCorpus numbers (n, three Δ̄/p/CI, fts mean nDCG@10) — a placeholder-only report
  FAILs the honesty gate.
- GREEN: run `python3 benchmarks/run_m53_hybrid_beir.py --dataset nfcorpus` with the key set; capture the block.

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios (external I/O — OpenAI API, PG, BEIR download)

- OpenAI key absent or embeddings endpoint 5xx/timeout/429 — the harness emits `status=UNBENCHMARKED` or retries
  429 with backoff (`openai_embed.py`); the report is flagged, never fabricated.
- NFCorpus download fails on the network — the loader raises a typed error and the run aborts; no partial result.
- PostgreSQL / theodb unavailable — `VectorDB.connect()` raises and the run aborts loudly; no silent empty result.

#### Acceptance criteria
- The report has real n, the three Δ̄/95%CI/permutation-p/wins-losses-ties, and the fts leg's mean nDCG@10; the
  verdict is honest (significant lift or parity); the confound + license + follow-ups are named.

#### DoD
- `docs/benchmarks/m125-hybrid-lexical.md` present with measured numbers + reproduction command.

## Failure scenarios

External I/O = OpenAI embeddings + PostgreSQL + the BEIR dataset download (all in T3; the significance code in T1
touches no I/O):

- OpenAI API key absent or the embeddings endpoint returns 5xx/timeout/429 — the harness emits `status=UNBENCHMARKED` or retries 429 with backoff (`openai_embed.py`); the report is flagged, never fabricated numbers.
- NFCorpus download fails on the network — the loader raises a typed error and the run aborts; no partial or fabricated result is written.
- PostgreSQL / theodb is unavailable — `VectorDB.connect()` raises and the run aborts loudly; no silent empty result reaches the report.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| NFCorpus (BM25≈dense synergy set) may show parity — a weaker test than Touché | MEDIUM | Honest-negative accepted; the fts-leg contribution disambiguates; Touché flagged as the strong-lexical follow-up (embed-budget) | implementer |
| ts_rank≠BM25 confound: a null could be a weak leg, not "hybrid fails" | MEDIUM | Measure hybrid-vs-fts + fts mean nDCG@10 to attribute; recommend the true-BM25-leg follow-up if the leg is the bottleneck | implementer |
| NFCorpus license unverified (HF cc-by-sa uploader tag; authors report none) | LOW | CI-internal use only (the permissive-licence gate covers distributed deps); flag in the report; do not redistribute the corpus | implementer |

## Unresolved Questions

- Should M125 install pg_textsearch for a true-BM25 A/B? Resolved at plan time: **deferred** (ADR M125-1) — heavy
  throwaway image; run it only if the ts_rank leg is proven the bottleneck, named as a follow-up.
- (none other — every decision is resolved at plan time.)

## Global DoD

- `_paired_sig` reports hybrid-vs-vector + hybrid-vs-fts + fts-vs-vector; unit-tested deterministically.
- NFCorpus measured honestly in `docs/benchmarks/m125-hybrid-lexical.md` (significant OR parity), with the three
  comparisons + fts nDCG@10 + the ts_rank confound + license flag + follow-ups — or UNBENCHMARKED with the reason.
- No production-code change; no new dependency; `pytest` green.
