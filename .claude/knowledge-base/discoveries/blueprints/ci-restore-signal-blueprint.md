# Blueprint — #140: CI failure is upstream of the repository (M133 → BLOCKED-on-owner)

> Discover executed 2026-07-21 by a decisive minimal-workflow experiment. Feeds M133.
> **Verdict: the repository cannot fix this.** Reported as BLOCKED rather than worked around (Rule 3).

## Context

Every GitHub Actions job on `develop` has failed for 30+ consecutive runs, each dying in 2–3 seconds with **zero
steps executed**. Releases v0.113.0–v0.118.0 were all merged red; verification for the whole M127–M132 program came
from measured droplet runs under `docs/benchmarks/`, not from CI.

## The decisive experiment

Hypotheses to separate: (A) our workflows are broken, (B) Actions cannot run for this account at all.

A **canary workflow** (`.github/workflows/ci-canary.yml`) was added with no dependencies, no secrets, no services
and a single `echo` step — the smallest thing that can possibly run.

```
job: canary   started 15:50:42Z   completed 15:50:44Z   conclusion: failure
steps: 0                       # no step ever started
logs: 22-byte (empty) zip      # the runner produced no output at all
```

**Hypothesis A is falsified.** A single-`echo` job on plain `runs-on: ubuntu-latest` cannot fail for a repository
reason.

## What else was ruled out (not assumed)

| Candidate | Check | Result |
|---|---|---|
| Workflow code regression | `git log -- .github/workflows/ci.yml` | untouched since `1b83632`, before M127 |
| Actions disabled for the repo | `GET /repos/…/actions/permissions` | `{"enabled": true, "allowed_actions": "all"}` |
| Repo archived/disabled | `GET /repos/…` | `archived: false, disabled: false` |
| A specific job/toolchain | all 8 jobs across every workflow | identical 0-step failure, now including a toolchain-free job |

## Remaining cause and why it is not fixable here

The runner is **never provisioned**. The repository is **private**, so Actions minutes are metered. This is the
signature of exhausted included minutes, a $0 spending limit, or a missing/expired payment method.
`usetheodev` is a **User** account (`GET /users/usetheodev → "type": "User"`), so the setting lives under the
personal account's Billing and plans.

Confirming *which* of those requires the billing API, which needs the `user` OAuth scope; the available token
carries only `gist, read:org, repo, workflow` → HTTP 404. **This is an honest limit of what could be established,
not a guess presented as fact.**

## What M133 can and cannot deliver

| DoD item | Status |
|---|---|
| Root cause identified with evidence, distinguishing account-level from workflow-level | **Met** — the canary falsifies the workflow-level hypothesis; account-level is the remaining cause, escalated with the exact settings path |
| A run where steps actually execute | **BLOCKED** — requires the owner to raise the spending limit / add payment (or make the repo public) |
| Triage the resulting conclusion | **BLOCKED** — depends on the above |
| A failure notification so a dead CI surfaces immediately | **Partially met** — the canary IS the liveness signal: a canary dying pre-step means the red is infrastructural, not a code regression. A `workflow_run` notifier is only meaningful once runs execute |
| #140 closed with evidence | **Not yet** — kept open with the escalation comment; closing it while CI is still dead would be a false PASS |

## ADR-1 — report BLOCKED; do not simulate CI to make the milestone look complete

**Decision:** M133 stays unchecked. The canary ships; the restoration waits on the owner.

**Rationale (Rule 3, and the milestone's own declared risk (a)):** the DoD requires steps to execute. Substituting a
self-hosted runner, disabling the failing jobs, or marking the milestone done on the strength of the diagnosis alone
would each produce a green board over a dead safety net — precisely the failure mode that let 30+ red runs pass
unnoticed.

**Alternatives rejected:** (i) add a self-hosted runner on the droplet — REJECTED: changes what CI *is* to dodge a
billing problem, and puts the test suite on the same box that produces the benchmark evidence (no independence).
(ii) Close #140 as external — REJECTED: the safety net is still absent; the issue is the tracking artifact for that.

## Verdict

**BLOCKED-on-owner**, with the cause narrowed to a single actionable setting and the diagnosis permanently
instrumented by the canary. Owner action: personal account → Settings → Billing and plans → spending limit /
Actions minutes; then re-run the canary via `workflow_dispatch`.
