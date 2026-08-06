# Discover-Opportunity Golden Rule

Locked unbreakable contract that `/discover-confidence` reads to score opportunities and decide verdicts. This file is the per-project Source of Truth promoted from `skills/discover-confidence/templates/discover-opportunity-golden-rule.example.md`.

Without this file, `/discover-confidence` falls back to the example template in the skill's `templates/` directory — usable, but not project-locked.

## § 1 — The unbreakable rule (LOCKED)

An opportunity is `INVALID` and **cannot** produce a `SHIPPABLE` or `SHIPPABLE_WITH_CAVEATS` verdict when either holds:

1. **Empty corner** — at least one of `## Corner 1 — Evidence`, `## Corner 2 — Constraint Relation`, `## Corner 3 — Blast Radius`, `## Corner 4 — Verification` is missing or has zero non-placeholder content.
2. **Fabricated evidence** — at least one code pointer `path/to/file.ext:LINE` cited in the opportunity fails to resolve: the file does not exist, the line is outside the file, or the file cannot be read.

This is NOT a guideline. It is a constraint enforced by the skill, which SHALL fail-closed when it is violated.

### § 1.1 — `unknown` is scoped, and the scoping is part of the rule

`<!-- UNKNOWN: reason -->` populates `## Corner 2 — Constraint Relation` and **no other corner**. The constraint is a lens rather than a gate (`current-constraint.md`): we do not instrument flow across the ecosystem, so demanding a constraint claim would be answered by assertion.

Honouring the marker anywhere else would make it the cheapest way to pass every other gate — an opportunity whose Evidence corner is `unknown` has measured nothing, which is the one thing this cycle exists to require. The scoping is load-bearing and SHALL NOT be widened.

The ancestor's `<!-- DEFERRED: ... -->` marker is **not honoured anywhere**. Deferring prior-art research was reasonable; deferring the measurement, the blast radius or the verification of a change to a running system is not.

## § 2 — What the rule requires

| Requirement | Enforcement |
|---|---|
| **Score capping** — final score capped at 49 when any unbreakable rule is violated, regardless of `weighted_avg` | `run_opportunity_score.py` |
| **Mandatory verdict** — returned verdict is `INVALID`, not any soft band | `run_opportunity_score.py` |
| **Vocabulary lock** — the word "shippable" SHALL NOT appear unqualified in a report whose score is capped | Rendering invariant |
| **Hard cap audit** — JSON output MUST list every triggered cap in `hard_caps_triggered` with stable identifiers (`empty_corner_evidence`, `empty_corner_constraint`, `empty_corner_blast_radius`, `empty_corner_verification`, `fabricated_evidence`, `no_adr_on_cross_repo_change`) | JSON schema invariant |
| **Visual rendering** — INVALID band appears in red when the score is capped | Renderer |

## § 3 — Rules that cannot be bent (LOCKED)

| Rule | Enforcement script |
|---|---|
| All 4 corners populated | `skills/discover-confidence/scripts/check_corner_coverage.py` |
| `unknown` honoured for Constraint Relation only | `skills/discover-confidence/scripts/check_corner_coverage.py` (`UNKNOWN_CORNERS`) |
| Every code pointer resolves, line included | `skills/discover-confidence/scripts/check_evidence_pointers.py` |
| Runtime observations counted, never reported as verified | `skills/discover-confidence/scripts/check_evidence_pointers.py` |
| Mandatory opportunity sections present | `skills/discover-confidence/scripts/check_opportunity_completeness.py` (cap 70) |
| ADR present when the blast radius reaches beyond the own repo | `skills/discover-confidence/scripts/check_opportunity_completeness.py` (cap 70) |
| `--skip-checks` flag does not exist and SHALL NOT be added | Constructor invariant in `run_opportunity_score.py` |
| Score-capped reports MUST mark the cap explicitly | Rendering invariant |
| `hard_caps_triggered` MUST be non-empty when `verdict == INVALID` | JSON schema invariant |

## § 4 — Why the rule exists

An opportunity missing a corner did not do the work it claims. One with unresolvable pointers is unsafe to plan against: everything downstream treats a cited `file:line` as measured fact and builds on it.

The lesson mirrors `plan-confidence`'s — **tests passing ≠ system works; a filled template ≠ something was measured.** One empty corner or one fabricated pointer produces wrong decisions downstream even when every other check is green.

Two clauses are worth their own justification, because both loosen the ancestor:

- **Runtime observations are never "verified".** An HTTP observation against a dev environment is transient; re-running it later can legitimately differ. A checker that re-tested and reported "verified" would be asserting, not measuring. Counting them separately is the honest option, and it is why an opportunity with zero code pointers is not automatically penalised.
- **The ADR requirement is conditional.** The ancestor demanded at least one ADR from every blueprint. For maintenance work that is ceremony: most items touch one repo and decide nothing architectural. But a change whose blast radius reaches other repos decides something for *their* maintainers, and shipping it unrecorded is how a breaking change arrives unannounced.

## § 5 — Verdict tokens (LOCKED)

`/discover-confidence` MUST emit one of these (matching `cycle-rule-schema.md`):

| Verdict | Score cap | Meaning | Downstream action |
|---|---|---|---|
| `SHIPPABLE` | 100 | Passes all gates with high confidence | Feed `/to-plan` |
| `SHIPPABLE_WITH_CAVEATS` | 89 | Passes hard caps; soft caps flagged | `/to-plan`, caveats carried into the plan |
| `NEEDS_REVISION` | 70 | Structurally OK; soft caps fire | Loop to `/discover-improve` |
| `INVALID` | 49 (capped) | Hard cap triggered | Back to `/discover-plan` — rewrite, not improve |

`ITEM_KILLED` is **not** scored by this skill. A killed item produces no opportunity; the outcome lives on the `B-NNN` block with its `kill_reason`.

## § 6 — When this rule may change

Per `cycle-rule-schema.md § Golden Rule Change Protocol` (ADR signed by the project owner). Rule-specific deviations:

- Document the change in `## § 3 — Rules that cannot be bent`.
- Bump the `rules/discover-opportunity-thresholds.txt` reference to the new ADR.

## § 7 — Failure modes the rule guards against

- Measurement theatre — claiming an investigation that was not run.
- Fabricated pointers slipping through manual review.
- Stale pointers surviving because only file existence was checked, never the line.
- A transient runtime observation being reported as a durable verified fact.
- A cross-repo decision shipping with no record for the repos it binds.
- `unknown` spreading from the constraint corner into the corners that carry the substance.
- `--skip-checks` flags being added to bypass the gate.
- Soft caps masking hard structural failure.

## Cross-references

- Schema: `cycle-rule-schema.md`
- Cycle rule: `cycle-discover.md`
- Intake that supplies the item: `cycle-backlog.md`
- Constraint lens: `current-constraint.md`
- Skill: `skills/discover-confidence/SKILL.md`
- Skill template (the seed for this file): `skills/discover-confidence/templates/discover-opportunity-golden-rule.example.md`
- Thresholds template: `skills/discover-confidence/templates/discover-opportunity-thresholds.example.txt`
- Rubric: `skills/discover-confidence/templates/rubric-opportunity.md`
- Defaults (fallback): `skills/discover-confidence/defaults/`
