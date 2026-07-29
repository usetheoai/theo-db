# Review: m167-projection-topk (re-review, supersedes 2026-07-28)

**Date:** 2026-07-29
**Head reviewed:** `30e2077` (branch `develop`)
**Supersedes:** `m167-projection-topk-review-2026-07-28.md` (`NEEDS_FIXES`)
**Verdict:** READY_TO_MERGE

## Why a second report exists

The 2026-07-28 report closed `NEEDS_FIXES` with two open blockers. Both were worked, and the work was then
re-reviewed **six times** by independent specialist agents. This file records the verdicts those agents emitted;
it does not restate their reasoning, which lives in the passes themselves.

The author did not edit the prior report's verdict line. A `NEEDS_FIXES` that the author overwrites is not a gate,
and `/release` reading the newest report is the mechanism by which a genuine re-review supersedes an old one.

## Verdicts by reviewer

| Reviewer | Scope | Verdict | Head |
|---|---|---|---|
| `council-rust-pgrx` | unsafe / FFI / panic-across-C / pgrx idiom on the M167 diff | **READY_TO_MERGE** | `bf809e7` |
| `council-benchmark` pass 2 | measurement validity, artifacts | `NEEDS_FIXES` (1 BLOCKER, 1 HIGH, 1 MEDIUM, 2 LOW) | `bf809e7` |
| `council-benchmark` pass 3 | ” | `NEEDS_FIXES` (2 HIGH, 2 MEDIUM, 4 LOW) | `3effab0` |
| `council-benchmark` pass 4 | ” | `NEEDS_FIXES` (3 MEDIUM, 1 LOW/MED, 4 LOW, 3 INFO) | `1ea7e0b` |
| `council-benchmark` pass 5 | ” | `NEEDS_FIXES` (2 MEDIUM, 5 LOW, 2 INFO) | `259c290` |
| `council-benchmark` pass 6 | ” | **READY_TO_MERGE** (0 BLOCKER, 0 HIGH, 2 MEDIUM, 8 LOW, 1 INFO) | `dad1516` |

The pgrx verdict still holds at `30e2077`: `git diff bf809e7..30e2077 -- theodb_rs/` is a doc-comment move and one
`#[inline]` relocation (`ad132ab`), with no change to routing behaviour.

Pass 6's two MEDIUMs were fixed anyway, at the reviewer's explicit request not to let one of them become
precedent — see `30e2077`. Its eight LOWs were also closed.

## What the sequence produced

Findings that came from review rather than from the author, and would otherwise have shipped:

| Class | Example |
|---|---|
| Wrong results reachable | the ICU-provider hole (`datcollate = C` while ordering by ICU); the collation guard admitted a text key whose DataFusion byte order disagrees with PostgreSQL |
| Guard that did not guard | the decode bound was inert on any table PostgreSQL never `ANALYZE`s — which is every columnar table |
| Test that proved nothing | `M167-D4` claimed to exercise `TOPK_MAX_SORT_KEYS = 8` but PostgreSQL deduplicated its repeated pathkeys down to four; the committed log contained the refutation |
| Evidence that could not fail | ten mismatch counters printed for a human to read; `rc=0` certified the queries ran, never that the zeros were zero |
| Claim the repository contradicted | a `CHANGELOG` entry describing a `plan_rows`-based guard the code no longer had; a provenance table falsified by `git log` |

Three of those were introduced by fixes to earlier findings. That is the argument for the sequence rather than
for a single deeper pass.

## Quality gates

| Gate | Result |
|---|---|
| `/code-quality` | `PASS_WITH_CAVEATS` — `HARD: 0` |
| Unit suites (`test_columnar_type_ab`, `test_m164_harness_guards`) | 33 passed, 5 skipped |
| Type-coverage routing gate (`rules/testing.md` § 5.1) | 35/35, positive control `diverged = 2` |
| 1M top-k oracle | `rc=0` — H0 `9/9`, FINAL GATE ok (15 assertions) |
| Fixture oracle | `rc=0` — EC FINAL GATE ok (20 assertions) |
| Gate positive controls | 4 logs, 4 deliberate failures (`rc=3` ×3, `rc=1`) |
| § 5 guard proofs | `rc=0`, both proofs self-asserting with a positive anchor |
| 43-query suite, final binary | 36/43 routed, `diverged = 0`, 42 pass / 1 errored (q28, pre-existing) |
| Provenance | 8 artifacts, identical `so_md5=9b9342f7e925d37ce0e1cf2ce1c356e0` |

## Definition of done

Three of four bullets met with committed evidence; **bullet 2b is PARTIAL and is not ticked** — the bounded heap is
O(k) but the decode feeding it is O(N), mitigated by a size guard rather than eliminated. Tracked in
[issue #215](https://github.com/usetheodev/theo-db/issues/215). The ROADMAP checkbox records that the milestone
shipped, not that this clause was satisfied. Full mapping: `docs/benchmarks/m167-projection-topk-verdict.md` § 5.5.

## Handoff

`READY_TO_MERGE` → `/release`.
