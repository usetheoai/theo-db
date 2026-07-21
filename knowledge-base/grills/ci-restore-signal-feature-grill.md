---
slug: ci-restore-signal
milestone_id: M133
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Feature grill — M133 fix #140 (restore CI signal: every Actions job fails pre-step)

Answers synthesized from issue #140 (filed 2026-07-21 while checking release pre-conditions for PR #139), per the
grill protocol's "explore first" rule. User intent was explicit: create milestones for #132, #140, #117.

## Q1 — What is this feature and why NOW?

Fix **#140**: **every** GitHub Actions job on `develop` has failed for **30+ consecutive runs**. Each job dies in
**2–3 seconds with zero steps executed** (`"steps": []`), and log retrieval returns `BlobNotFound` — no log was ever
produced. Affected: `pg-regression`, `ai-sql`, `columnar-measure`, `hybrid-search`, `harness-unit`, `image-and-bench`,
`bm25-measure`, `migration-smoke` — i.e. all of them.

**Why now:** CI provides **zero signal today**. Releases v0.113.0–v0.117.0 were all merged red, and verification for
the whole M127–M131 program came from measured droplet runs recorded under `docs/benchmarks/`, not from CI. Any real
regression is currently invisible. This is the safety net under every other milestone — including M132 and M134 —
so restoring it early makes the rest verifiable rather than hand-checked.

## Q2 — Dependencies (which milestones must be [x])

- **M131** `[x]` — most recent completed milestone.

No code dependency: `.github/workflows/ci.yml` has not changed since before M127 (last touch `1b83632`), so this is
not a regression introduced by recent work.

## Q3 — Definition of Done (verifiable)

1. Root cause identified **with evidence** and recorded in #140 — distinguishing an account/org-level Actions
   condition (exhausted minutes / spending limit / Actions disabled) from a workflow-level defect. The pre-step
   failure on plain `runs-on: ubuntu-latest` points at the former, but the milestone must confirm, not assume.
2. At least one full CI run on `develop` where **steps actually execute** (`gh api .../jobs/<id>` returns a non-empty
   `steps` array and logs are retrievable) — proving signal is restored, regardless of pass/fail.
3. The resulting conclusion is triaged honestly: a green run closes it; a red run **for genuine code reasons** is
   recorded and its failures filed as their own issues (see risk (b)).
4. A failure notification (e.g. a `workflow_run` hook) so a dead CI surfaces immediately instead of after 30 silent
   runs.
5. #140 closed with the evidence comment.

## Q4 — Top 2 NEW risks

1. **The root cause may sit outside the repository** (org billing / Actions enablement for `usetheodev`). If so, the
   fix requires an **owner action in GitHub settings** that no code change can substitute. Honest boundary: the
   milestone may legitimately end **BLOCKED-on-owner**, and that must be reported as BLOCKED rather than papered
   over — per Rule 3, an honest BLOCKED beats a false PASS.
2. **Restoring CI may reveal accumulated real failures** from 30+ unverified runs — the suite has not gated anything
   for the whole M127–M131 program. Scope could grow from "restore signal" to "fix N latent breakages". Mitigation:
   scope this milestone to **restoring signal + triaging**; file each genuine failure as its own issue instead of
   absorbing them here.

## Prior art

- Issue #140 (evidence: `"steps": []`, `BlobNotFound`, 30-run history, `ci.yml` untouched since `1b83632`).
- `rules/cycle-release.md` — green CI is an explicitly **optional (warn-not-block)** release pre-condition, which is
  why releases legitimately proceeded red; this milestone restores the net, it does not change the gate.

## SOTA delta

None — CI/infrastructure repair; no reference peers needed.
