# Opportunity: `--sweep` findings have no registration path in the corner checker

**Item:** B-004
**Repo:** squad
**Mode:** review
**Slug:** `sweep-registration-gap`
**Source plan:** `knowledge-base/discoveries/plans/sweep-registration-gap-plan.md`
**Generated:** 2026-08-05 via `/discover-execute`
**Confidence verdict:** PROVISIONAL (awaiting `/discover-confidence`)

## Context

`cycle-discover.md` declares that a `--sweep` run registers its findings directly into
`BACKLOG.md` with `source: discover-{mode}`, and lists "sweeping without registering" as
an anti-pattern. The deterministic checkers verify the opportunity document, but nothing
verifies that a sweep actually wrote its findings to the registry. The contract exists in
prose and has no enforcement behind it.

This is a `review`-mode finding: it is visible statically, in our own rules and scripts,
without running anything.

## Corner 1 — Evidence

The registration obligation is stated at `rules/cycle-discover.md:20` and repeated as an
anti-pattern near the end of the same rule. The output contract naming
`source: discover-{mode}` is at `rules/cycle-discover.md:150`.

The checker that scores a finished opportunity is
`skills/discover-confidence/scripts/check_corner_coverage.py:78` — it reads the
opportunity file and nothing else. `skills/discover-confidence/scripts/check_evidence_pointers.py:95`
resolves pointers against the project root, again touching only the document.

Neither checker opens `BACKLOG.md`, so a sweep that produces four well-formed
opportunities and registers none of them scores exactly as well as one that registers all
four. The gate that the anti-pattern implies does not exist.

## Corner 2 — Constraint Relation

<!-- UNKNOWN: rules/current-constraint.md is status=undeclared, and we do not instrument
flow across the ecosystem. Whether unregistered findings are a throughput limit or a
bookkeeping annoyance is not something we can currently measure. -->

## Corner 3 — Blast Radius

Repo-local. The change is confined to the `squad` tooling: one new deterministic check in
the `discover-confidence` slice plus its tests. No governed repo consumes these checkers
directly — they run against artifacts, not against product code.

No published contract changes, so no consumer needs to migrate and no ADR is warranted.

## Corner 4 — Verification

Tied to the item's DoD:

1. A sweep run producing N opportunities leaves N corresponding blocks in `BACKLOG.md`,
   each with `source: discover-{mode}` and a resolved `evidence` field.
2. A regression test constructs a sweep output whose findings were NOT registered and
   asserts the checker reports the gap — the test fails against today's code.

Where the limit plausibly moves next: once registration is enforced, `BACKLOG.md` becomes
the contended artifact. Two concurrent sweeps appending to it will collide, and id
allocation (`B-NNN` monotonic, never reused) becomes the next thing to protect.

## Recommendation

Add a `check_sweep_registration` deterministic check to the `discover-confidence` slice,
run only when the opportunity's provenance is a sweep. Write the failing test first, per
the RED-first discipline the implement cycle enforces.

Scope it to registration presence only. Verifying that the registered `evidence` field
matches the opportunity's Evidence corner is a second, larger check and belongs to its own
item.
