# Deps Audit: m167-projection-topk

**Date:** 2026-07-28
**Mode:** plan-bound:`m167-projection-topk`
**Verdict:** `INVALID_PLAN_DEPS`
**Hard caps triggered:** `plan_dependencies_section_missing`

## Summary

- **Ecosystems detected:** Rust (`theodb_rs`, `theodb_rs/lexical_core`, `benchmarks/m107_graph_spike`), Python (`benchmarks`)
- **Audit surface for this plan:** `theodb_rs` (the shipped extension — T2.1/T2.2 touch it) + `benchmarks` (T1.1/T2.1 touch it).
  `benchmarks/m107_graph_spike` and the two `.claude/skills/*/pyproject.toml` are outside the plan's blast radius and were
  not audited.
- **Vulnerabilities found:** 0 CRITICAL, 0 HIGH, **1 MEDIUM** (transitive), 0 LOW
- **Unmaintained-crate warnings:** 2 (no fix available)
- **Outdated:** not enumerated — no dependency change is proposed by this plan (see § Plan validation)
- **Allowlist hits:** 0 active, 0 expired (`.claude/rules/deps-audit-allowlist.txt` is empty)
- **Auditor coverage:** `cargo audit`: ran · `osv-scanner` (Cargo.lock): ran · `pip-audit` (requirements.txt): ran ·
  `npm audit`: n/a (no npm surface) · `govulncheck`: n/a (no Go surface)

**Cross-check earned its keep (Rule 9 / anti-pattern 6).** `cargo audit` reported `vulnerabilities.found = false, count = 0`
— it consults the RUSTSEC database only, and the thrift advisory below has no RUSTSEC entry. `osv-scanner` (GHSA-backed)
found it. Either auditor alone would have produced a false clean bill.

## Vulnerabilities (sorted by severity)

### GHSA-2f9f-gq7v-9h6m — MEDIUM (rust: `thrift@0.17.0`)
- **Summary:** Apache Thrift — Memory Allocation with Excessive Size Value.
- **Fixed in:** `thrift 0.23.0`
- **Path:** `theodb_rs` → `datafusion v54.0.0` → `parquet v58.3.0` → `thrift v0.17.0` (transitive; `thrift` is **not**
  declared in `theodb_rs/Cargo.toml`)
- **Reachability — stated honestly, not waved away.** `parquet` is **not** dead weight in this tree: `Cargo.toml:49`
  declares `datafusion = { …, features = ["parquet"] }`, and `theodb_rs/src/parquet.rs` implements the M143 own-code
  `theodb.read_parquet` reader. Parquet file **metadata is thrift-encoded**, so a hostile or corrupt Parquet file handed to
  `theodb.read_parquet` is an untrusted-input boundary that reaches this code. This is a genuine repo-level finding, not a
  bookkeeping artifact.
- **Relevance to M167 — none.** The path this plan makes default (`run_columnar_topk` → `decode_to_batch` → DataFusion
  `filter → sort → limit` over an **in-memory** `RecordBatch`) never parses Parquet: `df_executor.rs` contains no `parquet`
  reference. **M167 adds no exposure to this CVE and is not gated by it.**
- **Diff suggestion:** none applicable. `thrift` is transitive; the bump has to arrive via `parquet` / `datafusion`, which
  is a dependency-upgrade slice of its own (`datafusion 54` is deliberately pinned — `Cargo.toml:44` notes the pgrx-0.19
  coexistence is proven at this set, so an upgrade is not a one-line change).
- **Recommendation:** track as a separate slice (bump the `datafusion`/`parquet` set, re-prove pgrx coexistence), or add an
  allowlist entry with rationale + sunset ≤ 90 days. Do **not** attach it to M167 — unrelated scope.

## Unmaintained crates (informational — no fix available)

| Advisory | Crate | Path | Status |
|---|---|---|---|
| RUSTSEC-2024-0436 | `paste@1.0.15` | via `pgrx@0.19.0` | unmaintained; no patched version exists |
| RUSTSEC-2021-0127 | `serde_cbor@0.11.2` | via `pgrx@0.19.0` | unmaintained; no patched version exists |

Both arrive through `pgrx`, the extension framework — not directly actionable from this repo, and neither is a CVE.
Recorded for completeness (anti-pattern 3: never silently drop a finding).

## Python surface

`pip-audit --requirement benchmarks/requirements.txt` — **40 packages audited, 0 vulnerabilities.** The declared set
(`numpy`, `psycopg2-binary`, `pytest`, `h5py`, `fastembed`, `scann`) is dev/CI-only and is not shipped in the theo-db
image, as `requirements.txt` itself documents. T1.1 and T2.1 add test/benchmark cases only — **no new Python dependency**
(the top-k A/B uses `psycopg2` + stdlib, both already present; parsimony rung 4).

## Plan validation (Mode 2)

The plan at `.claude/knowledge-base/plans/m167-projection-topk-plan.md` has **no `## Dependencies` section**.

Per `deps-audit-golden-rule.md` § 2–3 (hard cap #4, stable id `plan_dependencies_section_missing`), that is
`INVALID_PLAN_DEPS` (cap 49) — regardless of the fact that the plan happens to introduce nothing.

**Why the cap is correct here rather than pedantic.** A plan with no Dependencies section is indistinguishable from a plan
whose author never asked the question. In this specific case the question had a real answer: the plan's whole purpose is to
route more traffic through DataFusion by default, and DataFusion's own subtree carries the MEDIUM advisory above. The
correct answer is "M167 does not touch the affected path" — but that is a conclusion the plan must *state*, not one a
reader should have to re-derive.

| Plan dep | Section | Manifest match | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| — | **section absent** | — | — | — | `plan_dependencies_section_missing` |

Expected content once added: zero rows under *New*, zero under *Removed*, and under *Existing — use as-is* the deps the
plan actually leans on (`pgrx`, `datafusion` + `arrow`), each with its version and the note that no version changes.

## Recommended next steps

1. Add a `## Dependencies` section to the plan declaring **no new dependencies**, listing the existing ones the changed code
   relies on, and recording the thrift advisory as *known, out of scope, not on the M167 path*.
2. Re-run `/deps-audit m167-projection-topk` to confirm the verdict moves to `PASS_WITH_CAVEATS`
   (the transitive MEDIUM keeps the soft cap at 89 per Workflow § Step 4.4 — transitive CVE rooted at a declared dep is a
   soft warning, not a `FAIL_MEDIUM`, which is reserved for a **declared** dep).
3. Re-run `/plan-confidence m167-projection-topk` (the plan text changed).
4. Separately: open a slice for the `datafusion`/`parquet` bump that carries `thrift ≥ 0.23.0`, **or** an allowlist entry
   with sunset. This is repo-level debt with a real untrusted-input boundary (`theodb.read_parquet`) — it should not be left
   implicit just because M167 does not touch it.

## Re-run after remediation (same day)

The `## Dependencies` section was added to the plan (no new deps; existing deps declared with the versions read from
`theodb_rs/Cargo.toml` / `benchmarks/requirements.txt`; the thrift advisory recorded as known-and-out-of-scope with its
reachability argument). Re-validated:

| Check | Result |
|---|---|
| `## Dependencies` present | yes |
| `Existing` / `New` / `Removed` subsections present | yes / yes / yes |
| Declared deps with a pinned version | 5 / 5 (none unset) |
| `New` + `Removed` explicitly `(none)` | yes — so Rule 9 evaluation is not applicable (no new dep to justify) |
| Unallowlisted CRITICAL/HIGH on a declared dep | none |

**Verdict after remediation:** `PASS_WITH_CAVEATS` — hard cap `plan_dependencies_section_missing` cleared; the transitive
MEDIUM (`GHSA-2f9f-gq7v-9h6m`) remains as the standing caveat at cap 89 per Workflow § Step 4.4 (transitive CVE rooted at a
declared dep is a soft warning; `FAIL_MEDIUM` is reserved for a **declared** dep).

Side-effect check: `/plan-confidence` re-scored after the plan edit — 93.6 / `SHIPPABLE` plan-only, 0 caps, 2 citations
resolved / 0 unresolved. The new section introduced no spec smells and no fabricated citation.

**Standing debt (not closed by this milestone):** the `datafusion`/`parquet` bump carrying `thrift ≥ 0.23.0`, or an
allowlist entry with sunset. `theodb.read_parquet` accepts user-supplied files whose metadata is thrift-encoded, so this is
a real untrusted-input boundary that outlives M167.

## Reproduction

```bash
cd theodb_rs && cargo audit --json
osv-scanner --lockfile=theodb_rs/Cargo.lock --json
pip-audit --requirement benchmarks/requirements.txt --format json
cargo tree --invert thrift --edges normal   # confirms the parquet → datafusion → theodb_rs chain
```
