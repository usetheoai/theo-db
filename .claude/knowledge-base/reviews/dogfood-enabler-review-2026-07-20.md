# Review — M124 dogfood enabler

**Date:** 2026-07-20 · **Slug:** dogfood-enabler · **Milestone:** M124
**Verdict:** READY_TO_MERGE (after fixing the HIGH)

## Scope

Adversarial review (council-ai-in-db, retrieval-honesty lens) of the M124 enabler: the self-host quickstart, the
anchor smoke, the two evidence files, the manifest `planned → wired` flip, and the honest-scope framing.

## Consolidated findings

| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | HIGH | The one recorded "fused FTS+vector" run was actually **vector-leg-only**: the query "how does the index keep vector search fast" matched no doc under `plainto_tsquery` AND-semantics, so the FTS leg was empty. The scores (1/61…1/65) were the single-leg arithmetic signature, yet the evidence + CHANGELOG called it a two-leg fusion. The smoke gate (`RES>=1`) accepted either leg alone. | **FIXED** — query changed to `'vector search'` (matches the FTS AND-filter on the HNSW doc); the smoke now asserts **both legs non-empty** AND **max_score > 1/61** (proving a doc is scored by both legs, RRF-summed). Re-run MEASURED: doc 2 = `0.032787 = 1/61+1/61` (rank-1 in both legs), others single-leg → `two_leg_fusion=yes`, `SMOKE_RESULT: PASS`. Evidence + CHANGELOG prose corrected to the honest two-leg result (with the earlier vector-only run disclosed). |
| 2 | (process) | Two smoke bugs surfaced while fixing #1: the async-worker failure and the SSRF assertion were gating the whole smoke. | **FIXED** — the async-worker outcome is a recorded diagnostic (not the gate; the gate is the fused query path); the SSRF assertion was masked by `set -o pipefail` (a firing guard exits psql non-zero, which the piped grep's success couldn't override) — rewritten to capture-then-grep. Re-run: SSRF `PASS`, fusion `PASS`. |

No BLOCKER. The reviewer's LOW/informational points (only 1 of the 2 anchor halves works on self-host; `wired` rests on the query half) are disclosed everywhere, not concealed — accepted.

## What the review confirmed sound (verified, unchanged)

- **Real embeddings, real vector leg** — embedded via live OpenAI `text-embedding-3-small` (`query_vector` NULL path), not faked.
- **`wired` flip justified, `running` correctly NOT claimed** — `manifest.md` status `wired` (golden-rule § 2 bar: invoked in a manual smoke); the honest-scope rationale (ADR M124-1) is explicit; no `production-ready`/`running` overclaim anywhere.
- **Failure story real + traceable** — issue #132 exists and is OPEN with a matching title + a genuine discriminator (session embed works, worker fails); the backfill gap honestly downgraded to a docs note.
- **Frontmatter § 5 complete** on both evidence files; `scenario:` matches the anchor slug.
- **SSRF/GUC guidance matches code** (`embed.rs:173-194` resolve_cfg, http(s)-only guard).
- **No secrets committed** — the key is read from `$OPENAI_API_KEY`; scan of all M124 files clean.

## Post-fix measured result

`SMOKE_RESULT: PASS (fused_rows=5, max_score=0.032787, two_leg_fusion=yes, fts_hits=1, embedded=5/5, async_worker_embedded=no)` — SSRF fail-closed PASS; genuine two-leg RRF fusion proven; the async-worker failure recorded as the mandatory failure story (#132).

## Verdict

**READY_TO_MERGE.** The HIGH (fusion-honesty gap) is fixed with a stronger gate that a single-leg result would now
FAIL, and the prose matches the measured two-leg fusion. The `wired` claim is sound and honest; `running` stays
correctly unmade.
