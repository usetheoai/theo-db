# Review — M37 `ai.summarize` (doc-drift correction)

**Date:** 2026-07-03
**Slug:** m37-ai-summarize
**Milestone:** M37
**Verdict:** READY_TO_MERGE
**Scope:** doc-only (2 feature docs + validation artifact + CHANGELOG/ROADMAP/plan/blueprint). **Zero production code changed.**

## Why this review is doc-scoped

The M37 CYCLE discovery falsified its own premise: `ai.summarize` / `ai.agg_summarize` were already
delivered (M10) and tested. Writing Rust for them would be a DUPLICATE (install conflict) — the
measurement-first grounding prevented it (the redundant Rust additions were reverted via `git restore`).
M37 therefore delivered a doc-drift correction, not new code. The review validates the correction against
reality, not new architecture/wiring.

## Findings by dimension

| Dimension | Result | Evidence |
|---|---|---|
| **Cross-validation (docs ↔ code ↔ tests)** | PASS | `docs/features/11` cites `sql/50-theodb-ai.sql:32` (`ai.summarize`) + `:82` (`ai.agg_summarize`) + 6 real test names; `docs/features/08` cites `ai.generate_batch`/`ai.if` + 9 tests. Both pass `deep-research/validate_citations.py` (0 fabricated). |
| **Functional evidence** | PASS | Live: 33 offline contract tests PASS + 3 real-OpenAI (gpt-4o-mini) PASS, incl. `test_real_openai_agg_summarize_shape`. Scalar summarize live example captured. See `docs/benchmarks/m37-ai-summarize-validation.md`. |
| **Security** | PASS | `ai.summarize`/`ai.agg_summarize` `proacl={postgres=X/postgres}` → REVOKE FROM PUBLIC (least-privilege, ADR 0007). Proven by `test_ai_functions_not_executable_by_public`. |
| **Code drift** | PASS | `git status` clean before commit; `theodb_rs/src/{chat,api}.rs` byte-identical to committed state (no duplicate `ai.summarize`). |
| **Honesty (public-copy / Rule 3)** | PASS | No throughput claim (would be a measure of the LLM provider, not the engine). Docs mark the AlloyDB-style preview surface as target-API, not delivered. |
| **CHANGELOG discipline (Rule 6)** | PASS | `[Unreleased] § Changed` documents the correction + root cause (Rust-only audit was incomplete). |

## Hard gates (`cycle-review` BLOCKER-level)

- Failing tests on branch → **none** (33 passed + 3 live passed).
- New secrets committed → **none** (OPENAI key only from `.env`, never in tree).
- Direct commit to `main` → **no** (work on `develop`).
- Co-Authored-By trailer → **none**.
- CHANGELOG not updated despite source changes → **N/A** (no source changed) + CHANGELOG updated anyway.

## Benchmark requirement (standing directive)

The directive demands "DADOS E VALIDAÇÕES EM BENCHMARK". For a pre-existing AI surface with no new code,
the honest validation IS the contract test suite against the real container + real LLM (documented in
`docs/benchmarks/m37-ai-summarize-validation.md`). A synthetic throughput number for LLM summarization
would measure the provider, not TheoDB — publishing it as a TheoDB claim would violate `public-copy.md`
§4/§5. The validation artifact records this explicitly.

## Verdict rationale

No BLOCKER, no HIGH findings. Docs now match delivered reality; every claim is citation-validated and
backed by live functional evidence. **READY_TO_MERGE.**

## Release recommendation

Doc-only change → recommend folding into the next real release (no dedicated version cut for docs, per the
M38 precedent). Human decides.
