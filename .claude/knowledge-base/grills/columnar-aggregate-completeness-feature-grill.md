---
generated_by: roadmap-feature
slug: columnar-aggregate-completeness
date: 2026-07-19
status: completed
new_milestones: [M114, M115]
---

# Feature grill — columnar analytical follow-ups (M114 + M115)

Two milestones amending ROADMAP v2 (last was M113). Requirements derived with 95%+ confidence from the just-shipped
ad-hoc columnar slices (zone-map int/temporal + GROUP BY pushdown) and the owner's explicit scope; the split was
confirmed via AskUserQuestion (breadth vs planner-fix).

## Q1 — What & why now

Three capabilities surfaced as "next" in the GROUP BY pushdown verdict caveats, split into two milestones:

- **M114 (breadth):** GROUP BY + WHERE combined + `avg`/`sum(int)` — broaden the admitted columnar aggregate SHAPES.
- **M115 (composability):** resolve the pre-existing M100 limitation where a columnar-aggregate output VALUE consumed
  inside an enclosing expression (subquery/join/aggregate-ORDER-BY) fails (`cache lookup failed for attribute N of
  relation 0`).

**Why now:** the ad-hoc columnar slices (zone-map skip, temporal, GROUP BY) shipped measured wins but left two honest
gaps — the aggregate surface is narrow (float8-only, no combined skip×group) and the output is not composable. Both
are prerequisites for real analytical/RAG queries that compose aggregates. Owner declared them as the next roadmap work.

## Q2 — Dependencies

Both gate on **M100** (the DataFusion CustomScan they extend/fix). The ad-hoc GROUP BY/zone-map slices (off-roadmap,
no milestone_id) are prior art, not gates. M114 and M115 are independent of each other (either order); M115 enhances
both the scalar and M114 output.

## Q3 — Definition of done

**M114:** admit accepts `groupClause` + `baserestrictinfo` together (skip + Filter + group in one plan); `avg` +
`sum(int2/4/8)` admitted with the exact PG output-type semantics (or decline); byte-identical in-PG A/B per new shape;
CHANGELOG + verdict.

**M115:** the columnar-aggregate output (scalar AND grouped) is byte-identical and usable in a subquery, a join, and
an aggregate `ORDER BY` over the agg value, CustomScan engaged; no regression to the top-level path; A/B/tests + verdict.

## Q4 — Top 2 new risks each

**M114:** (a) integer-sum overflow / numeric semantics — PG `sum(int4)`→int8, `sum(int8)`→numeric, `avg`→numeric;
must match exactly or decline (fail-safe). (b) skip×group interaction — a skipped chunk group must never drop a group
(the DataFusion Filter is the authority — reuse the D3 admission-filter invariant; A/B with partial-overlap groups).

**M115:** (a) deep `setrefs`/SubqueryScan-removal interaction — the naive `INDEX_VAR`-in-`plan.targetlist` attempt
already broke the top-level path (setrefs wants the real exprs and builds INDEX_VAR itself); discover-first the correct
`scanrelid=0` grouped pattern. (b) the fix may require intercepting at a different planner stage → scope could grow;
measurement/spike-first gate before committing the approach.
