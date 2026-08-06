# Measurement Plan: hook coverage of the forbidden-git-command list

**Item:** B-007
**Repo:** squad
**Mode:** review
**Slug:** `git-hook-coverage`
**Owner:** test-author
**Created:** 2026-08-05

> Positive fixture for `/discover-plan-confidence` unit tests. Four corners covered, every
> path target resolves, question budget 4 (within 3-10), each question names a Tool and a
> Target.

## Context

`rules/git-safety.md` enumerates the forbidden git commands and states that
`hooks/validate-command.sh` enforces "the mechanizable subset". The rule names which subset
it expects the hook to cover. Nothing verifies that the hook and the rule still agree, and
the two are edited independently.

## Hypothesis

At least one command the rule declares mechanizable is absent from the hook, so the
document promises an enforcement that does not run.

## Falsification

Every forbidden command that `rules/git-safety.md` marks mechanizable has a matching branch
in `hooks/validate-command.sh`, and a crafted invocation of each is refused with exit code
2. If all of them are refused, the hypothesis is dead and item B-007 is killed with that
result as its `kill_reason`.

A partial result does not rescue the hypothesis: the claim is about the mechanizable subset
as a set, not about finding one interesting gap.

## Measurement Questions

| # | Question | Corner | Tool | Target | Expected answer shape |
|---|---|---|---|---|---|
| Q1 | Which commands does the rule declare mechanizable? | evidence | Read | `rules/git-safety.md` | Enumerated list with the line of each claim |
| Q2 | Which of those does the hook actually branch on? | evidence | Grep | `hooks/validate-command.sh` | Command → matching branch line, or absent |

<!-- DEFER-CORNER: blast_radius | not scoped in this pass -->
<!-- DEFER-CORNER: verification | not scoped in this pass -->
<!-- DEFER-CORNER: constraint | rules/current-constraint.md is status=undeclared and flow is not instrumented; the constraint relation is recorded as unknown in the opportunity rather than guessed here. -->

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | Every cited path opens | Mark Qx BLOCKED with the path that failed |
| After answering Qx | The answer carries a `file:line` pointer | Re-iterate Qx, one retry |
| Before Q4 | Q1 and Q2 both answered | Hold Q4 — verification without the gap list is theatre |
| Before promising complete | All four corners covered or explicitly deferred | Refuse the promise, keep iterating |

## Acceptance Criteria

- [ ] Every question is answered or BLOCKED with a stated reason
- [ ] Each answer carries a pointer that resolves, line included
- [ ] The falsification criterion is evaluated explicitly, with its result recorded
- [ ] `/discover-confidence` on the resulting opportunity reaches SHIPPABLE_WITH_CAVEATS or higher
