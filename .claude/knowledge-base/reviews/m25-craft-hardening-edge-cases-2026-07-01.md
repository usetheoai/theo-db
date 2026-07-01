# Edge Case Review — m25-craft-hardening

Date: 2026-07-01
Tasks analyzed: 4 (T1.1 DRY+consts, T2.1 decompose+tests, T3.1 lib.rs split, T4.1 validation)
Cases found: 3 (NEGATIVE: 2, EDGE: 1 | MUST FIX: 2 already-in-plan, DOCUMENT: 1)

## MUST FIX (both already absorbed into the plan — no rewrite needed)

### EC-1: nl_to_sql extraction must not weaken SQL-injection defense
- **Affected task:** T2.1 · **Kind:** NEGATIVE (security boundary)
- **Scenario:** decomposing the L2/L4 validation could accidentally drop a guard (multi-statement, disallowed relation, banned token).
- **Impact:** injection bypass — the worst possible regression for this milestone.
- **Status:** COVERED — plan T2.1 TDD writes the L2/L4 rejection tests FIRST (RED before extract); Drawbacks lists it HIGH; existing nl behavior tests + Python oracle stay green.

### EC-2: lib.rs split must preserve extension_sql `requires` ordering + every REVOKE
- **Affected task:** T3.1 · **Kind:** NEGATIVE (silent behavior drift)
- **Scenario:** moving `extension_sql!` blocks could reorder the install script or drop a `REVOKE ... FROM PUBLIC`, silently widening privileges.
- **Impact:** broken install ordering OR a security REVOKE lost.
- **Status:** COVERED — plan T3.1 moves blocks verbatim with `requires`/`name`; AC asserts every function+REVOKE identical; full pytest suite is the net.

## DOCUMENT

### EC-3: Docker-only build slows iteration; a stale clippy #[allow] could slip
- **Kind:** EDGE (tooling constraint) · **Accepted risk:** plan gates `cargo clippy` in Docker after every task; DoD forbids new `#[allow]`. No code change.

## Summary
| Task | NEGATIVE | EDGE | MUST FIX | DOCUMENT |
|------|----------|------|----------|----------|
| T2.1 | 1 | 0 | 1 (covered) | 0 |
| T3.1 | 1 | 0 | 1 (covered) | 0 |
| T4.1 | 0 | 1 | 0 | 1 |

**Verdict:** PLAN OK — both MUST-FIX are already in the plan's TDD + Drawbacks; no plan revision needed.
