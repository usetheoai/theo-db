# ADR-0045: Reconcile the LOCKED North Star (ADR-0002) with the measured vector verdict (ADR-0035/0036)

- **Status:** proposed (awaiting owner sign-off)
- **Date:** 2026-07-16
- **Deciders:** project owner / CTO (this ADR cannot be self-adopted — ADR-0002 is LOCKED)
- **Supersedes / relates:** ADR-0002 (LOCKED North Star), ADR-0033 (reposition proposal, PROPOSED since 2026-07-10), ADR-0035 (M73 vector verdict), ADR-0036 (M74 RaBitQ lever verdict)
- **Tags:** governance, north-star, no-code-change

> Drafted by the system-design audit (Phase 6). This is a **governance** ADR: it recommends a
> decision-record reconciliation, not a code change. The engineering discipline behind the
> contradiction is exemplary — the gap is purely that the mandate-of-record lags the evidence.

## Context and problem statement

`ADR-0002` ("North Star — equal or superior to AlloyDB", **LOCKED**) mandates, among its
pillars, pursuit of **vector-query QPS superiority** over AlloyDB/ScaNN. The team then
**measured** that goal and recorded the result:

- `ADR-0035` (M73): vector QPS superiority over ScaNN/AlloyDB is **NOT achievable** as a
  permissive PostgreSQL extension (the ~25–44× gap at recall 0.99 is a paradigm gap —
  AH-LUT anisotropic + no MVCC/WAL tax — not a tuning gap).
- `ADR-0036` (M74): RaBitQ, the best permissive lever, buys **memory, not QPS**; the team
  deliberately chose **not** to build the full AM (anti-sunk-cost / D3).

A repositioning record, `ADR-0033`, was drafted to update the mandate — but it has sat at
`Status: PROPOSED` since **2026-07-10 (6 days as of this audit)** while milestones M74, M102,
M103 (ADR-0036/0043/0044) shipped **citing 0033's positioning as if adopted**.

**The mandate-of-record and the measured reality are in a documented but unresolved
contradiction.** The single `rationale_valid = 0` trade-off in the audit. A Staff engineer
reading the repo cannot today tell whether "beat AlloyDB on vector QPS" is still a live goal.

## Decision drivers

- The LOCKED-golden-rule change protocol requires **owner sign-off** to alter ADR-0002; the
  audit (an analyst) must not unilaterally flip a LOCKED mandate (honesty over convenience).
- Downstream ADRs already assume the repositioned framing → the record is *de facto* adopted
  but *de jure* unsigned. This drift compounds each milestone.
- Public-copy discipline (`public-copy.md`) forbids the "faster than AlloyDB on vector" claim;
  the LOCKED doc still implies it is the goal.

## Considered options

1. **Owner signs ADR-0033** (adopt the reposition: parity-recall + memory/billion-scale +
   AI-native/HTAP/open, drop the "vector QPS superiority" pillar).
2. **Add an explicit supersede note to ADR-0002** pointing at ADR-0035/0036 as the measured
   invalidation of the QPS-superiority pillar, leaving the rest of 0002 intact.
3. **Do nothing** — leave the contradiction; keep citing 0033 informally. (Rejected.)

## Decision outcome

**Chosen: Option 1 (sign ADR-0033), with Option 2 as the minimal fallback.** Either closes
the governance debt; Option 1 is preferred because downstream ADRs already reference 0033.
**No code changes** are entailed — the engine already reflects the measured reality.

### Consequences

- **Good:** the mandate-of-record matches the measured evidence and the shipped positioning;
  new contributors read one consistent North Star; public-copy risk removed.
- **Good:** preserves the exemplary anti-sunk-cost discipline as the *documented* posture
  rather than an implicit one.
- **Bad / cost:** requires an owner decision cycle; formally narrows the North Star's boastable
  scope (which is the honest outcome — the ambition was measured and bounded).
- **Neutral:** ADR-0006 (own-code index) is **unaffected** — its rationale (recall parity +
  own vector type + memory-scale moat) never rested on QPS superiority, so 0035's negative
  verdict does not invalidate it.

## Validation

- ADR-0033 flips to `Status: ACCEPTED` with a signer + date, **or** ADR-0002 gains a
  `Superseded-in-part-by: 0035, 0036` header note.
- No open ADR cites 0033's positioning while 0033 remains `PROPOSED`.
