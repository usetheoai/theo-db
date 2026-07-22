---
slug: vectorizer-worker-embed-fix
milestone_id: M132
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Feature grill — M132 fix #132 (vectorizer bgworker dead-letters every embed job on self-host)

Answers synthesized from issue #132 + the dogfood evidence, per the grill protocol's "explore first, ask the user
only for intent/preference" rule. User intent was explicit: create milestones for #132, #140, #117.

## Q1 — What is this feature and why NOW?

Fix **#132**: on a self-hosted TheoDB, the async vectorizer background worker **dead-letters every embed job**
(`state = 'failed'`, `attempts = 5`, `last_error = 'embed/upsert failed'`); the embedding column stays NULL. The
queue, trigger and worker state machine all work (5 pending → 5 failed) — only the worker's embed step fails. The
discriminator is decisive: with the **same** instance GUCs, `theodb.embed(...)` and `theodb.embed_batch(...)` succeed
in a normal session and fail every time inside the bgworker. So the delta is the **bgworker execution context**, not
the request.

**Why now:** this is the single highest-leverage item on the maturity gap. The dogfood anchor is stuck at status
`wired` (the only status that supports a production-ready claim is `running`), and its own evidence file records
`outcome: partial` **because of this defect**. It breaks the *freshness* half of the flagship AI-native promise —
"the vectorizer keeps embeddings fresh" — on the target deployment. Every benchmark we have proves algorithms; this
is what converts them into product evidence.

## Q2 — Dependencies (which milestones must be [x])

- **M131** `[x]` — most recent completed milestone.
- **M122** `[x]` — the fully-async embed split in the vectorizer, the code area this touches.
- **M124** `[x]` — the dogfood anchor + `benchmarks/dogfood_anchor_smoke.sh`, the repro harness.

All satisfied.

## Q3 — Definition of Done (verifiable)

1. `last_error` records the **underlying** cause (HTTP status, or "embedding GUC not visible in worker") instead of
   the generic `embed/upsert failed` wrapper — the current message hides the root cause and forces a debugger.
2. Worker startup logs the presence of endpoint/model and the **length** of the api-key (never the value) so a
   misconfigured worker is diagnosable from the log alone.
3. Root cause identified and fixed: `benchmarks/dogfood_anchor_smoke.sh` on a self-hosted instance ends with
   `SELECT state, count(*) FROM theodb.vectorizer_queue GROUP BY 1` showing **0 rows in `failed`** and the embedding
   column **non-NULL for every inserted row**.
4. A regression test covering the worker path (not only the session path) — the session path already passes today,
   so a session-only test would not have caught this.
5. A dogfood evidence file recording the passing anchor run, moving the anchor off `outcome: partial`.

## Q4 — Top 2 NEW risks

1. **The root cause may be placeholder-GUC visibility inside `BackgroundWorker::transaction`.** If so, the fix means
   registering `theodb.embedding_*` as real custom GUCs — an operator-visible configuration change that must be
   documented (and may alter how existing deployments set them). Mitigation: confirm the cause from the improved
   `last_error`/startup log BEFORE choosing between "register real GUCs" and "document where the worker reads them".
2. **A worker-side HTTP/TLS init difference could interact with the M122 async split.** Changing worker-side HTTP
   setup risks re-opening the `backend_xmin` pin that M122 closed. Mitigation: re-run the M122 xmin proof after the
   fix; treat any regression there as blocking.

## Prior art

- Issue #132 (repro, discriminator, suggested fix), `knowledge-base/dogfood/evidence/2026-07-20-anchor-failure-modes.md`.
- `benchmarks/dogfood_anchor_smoke.sh` (the repro harness), M122 async-embed work, `rules/dogfood-golden-rule.md § 2`
  (the `wired` → `running` status contract).

## SOTA delta

None — this is a defect in our own bgworker path; no new reference peers needed.
