# Review — remove control-plane/platform (repo is the database only)

**Date:** 2026-07-03
**Slug:** platform-removal
**Verdict:** READY_TO_MERGE
**Type:** chore/removal — verification review (NOT a full 5-7 agent `/review`; there is ZERO production-code change, so a full review is YAGNI). This record satisfies the `cycle-release` traceability gate honestly.
**Commit:** `1b83632`

## Change

Per owner directive ("remove everything platform; the project is only the database"), removed the entire Go control-plane and HA layer from this repository: `operator/` (Go k8s operator + CLI + gateway + MCP server + read-pool + observability) and `ha/` (Patroni + pgBackRest + failover/PITR smokes), plus orphaned platform docs, the CI `ha-smoke` job, README promises, and CLAUDE.md's platform-integration section. ROADMAP milestones M23/M24/M27/M28/M29 marked REMOVIDO.

## Verification performed

| Check | Result |
|---|---|
| Database engine untouched | PASS — `theodb_rs/` (the pgrx extension) not modified; `git show --stat 1b83632` touches only operator/, ha/, docs, CI, README, ROADMAP, CLAUDE.md, CHANGELOG. |
| No production-code deletion | PASS — the deletions are the Go control-plane (a separate concern) + docs + CI job. The DB engine, `sql/`, `packaging/`, `benchmarks/` are intact. |
| No dangling references in active build/config | PASS — `git grep` for `operator/` / `ha/*` in Makefile / `.github` / Dockerfile / packaging returns nothing (remaining hits are released-CHANGELOG history + intentional ROADMAP cancellation notes). |
| CI still valid | PASS — `ci.yml` parses as valid YAML after the `ha-smoke` job removal; no job `needs:` the removed job. |
| Makefile / Dockerfile still build the engine | PASS — neither referenced operator/ha; the engine build path is unchanged. |
| CHANGELOG discipline (Rule 6) | PASS — removal documented under `[Unreleased] § Removed`; released entries (M23/M4) left untouched. |
| LOCKED ADR integrity | PASS — ADR 0006 (Rust/Go strategy) left intact; its Go clause flagged for a formal superseding ADR (owner sign-off) rather than edited unilaterally. |
| Honesty (Rule 3) | PASS — gitignored `references/` left (not part of repo); the descoping is recorded, not spun. |

## Hard gates

- No failing tests introduced (no test/production code changed — only deletions of a separate Go module + docs). No secrets. On `develop`. No `Co-Authored-By`. CHANGELOG updated.

## Verdict rationale

Zero BLOCKER. A pure, owner-directed removal of a separate concern (the Go control-plane) with complete reference cleanup (no broken build), the DB engine fully intact, and honest documentation of what was descoped and what was deliberately preserved (LOCKED ADR, released CHANGELOG, gitignored references). **READY_TO_MERGE.**
