---
id: 0001-discover-phd-rigor
status: accepted
date: 2026-06-26
owner: usetheo
---

# 0001 — Discover PhD-rigor profile for TheoDB (frontier)

## Context

TheoDB is a frontier, open-source PostgreSQL-compatible database that mirrors the
AlloyDB SOTA using only permissive OSS pieces (PRD, CLAUDE.md). Its highest-stakes
decisions — e.g. the `pgvector`/`pgvectorscale` fork trigger (PRD D3) — must be made
on **reproducible benchmark evidence anchored on the field's state of the art**, not on
a shallow read of a couple of repositories.

The installed `cycle-discover` shipped with generic defaults that are too weak for this:

- `rules/discover-web-allowlist.txt` was empty → WebFetch disabled → discovery was blind
  to arXiv, AlloyDB docs, and the maintained repos of the permissive pieces.
- Question budget was the generic 5–10 total / ≤ 3 per corner, squeezing the
  algorithm/SOTA axis (the `techniques` corner) into the same shallow window as tooling.
- Verdict bands were the generic 90/70 defaults — not calibrated for a frontier bar.
- No requirement to anchor a technique on SOTA, to cite ≥ 2 primary sources, or to carry
  benchmark evidence for performance claims.

## Decision

Adopt a **per-project PhD-rigor profile** for discovery, recorded in
`rules/discover-phd-rigor.md`, and wire it through the cycle:

1. **Populate the WebFetch allowlist** with authoritative SOTA domains only
   (peer-reviewed venues + stable resolvers, the AlloyDB/ScaNN official pages, the
   permissive dependencies' official docs/repos). `rules/discover-web-allowlist.txt`.
2. **Widen the question budget (frontier profile):** total **6–14**, **≤ 5 per corner**,
   `techniques` corner **≥ 2**. Enforced structurally by raising the constants in
   `skills/discover-plan-confidence/scripts/check_plan_completeness.py`
   (`MIN_QUESTIONS=6`, `MAX_QUESTIONS=14`, `MAX_PER_CORNER=5`). Stays **inside** the
   LOCKED hard cap `question count ≤ 15` — that number is unchanged.
3. **Add R1/R2/R3 rigor on the techniques corner** (SOTA-anchoring; ≥ 2 primary sources;
   benchmark methodology+numbers+source OR the literal marker `UNBENCHMARKED`). These are
   **review-enforced** today (read by `/discover-plan-confidence`, `/discover-confidence`,
   `/discover-edge-cases`) — honest debt, not faked with a phantom script.
4. **Tighten verdict bands** to SHIPPABLE 92 / SHIPPABLE_WITH_CAVEATS 75 in
   `discover-plan-thresholds.txt` and `discover-blueprint-thresholds.txt`.
5. **Record the profile in the two LOCKED discover golden rules** under a new
   `§ 3.1 — Project rigor profile` subsection, and cross-reference it from `cycle-discover.md`.

## Alternatives considered

- **Add a literal 5th coverage corner ("SOTA").** Rejected (KISS): the corner list is
  hardcoded in two checker scripts + the rubric + the template; adding a corner ripples
  through all of them. Strengthening the existing `techniques` corner achieves the same
  outcome with no structural ripple.
- **Implement R1/R2/R3 as deterministic checkers now.** Deferred: a trustworthy checker
  needs fixtures + integration into the score orchestrator + its own ADR. Faking a script
  reference would itself violate the fabricated-citation hard cap. Review-enforcement now,
  promotion later via a follow-up ADR.
- **Raise the budget hard cap above 15.** Rejected: 14 already gives ample frontier depth
  and keeps the scoping discipline the LOCKED cap protects.
- **Leave bands at 90/70.** Rejected: a frontier blueprint should clear a higher bar before
  it is declared "ready for use".

## Consequences

- Discovery can now reach and cite the field (allowlist), and is required to anchor on SOTA,
  back claims with ≥ 2 primary sources, and benchmark (or honestly flag) every perf claim.
- Frontier plans may go deeper on the algorithm axis without tripping the budget hard cap.
- A blueprint must score higher (≥ 92) to be SHIPPABLE.
- Honest debt: R1/R2/R3 are review-enforced until a fixture-backed checker is shipped (a
  future ADR). `discover-phd-rigor.md § 3` records this.
- No LOCKED hard cap was weakened; the ≤ 15 budget cap and all fabricated-citation /
  empty-corner caps are unchanged.

## Compliance

- CHANGELOG `[Unreleased]` updated (Unbreakable Rule 6).
- `scripts/check_xrefs.py` and `scripts/test_e2e_smoke.py` MUST pass after this change
  (per each golden rule's `§ When this rule may change`).
- `check_plan_completeness.py` re-validated against its fixtures (good passes, under-budget
  still fails at the new floor of 6).
