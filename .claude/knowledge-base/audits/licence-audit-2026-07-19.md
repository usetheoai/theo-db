# Licence Compliance Audit — TheoDB

**Date:** 2026-07-19
**Project licence:** Apache-2.0
**Mode:** full
**References audited:** pgvector, pgvectorscale (both PostgreSQL License)
**Evidence store:** `.claude/check-licence-evidence.db`
**Verdict:** **PASS — no licence violation. No copied code at problematic severity; all references permissive and Apache-2.0-compatible.**

## Severity matrix

| Severity | Count | Nature |
|---|---|---|
| critical | 0 | — |
| high | 9 | README.md mentions of pgvector (×7), langchain (×1), llamaindex (×1) — legitimate foundation/ecosystem positioning (see § Mentions) |
| medium | 652 | source-code + commit-message mentions of the permissive references (expected — TheoDB composes/forks them under D3) |
| low | ~4292 | internal-doc prior-art citations + low structural clones |
| info | 9 | import patterns / common idioms |

> **Count note (honesty):** the deterministic `scan_mentions.py` was run twice (initial audit + a refresh during Phase 3.5), and it appends rather than de-duplicates, so the raw DB totals are ~2× the true values (18 shipped, 9894 mentions on disk). The **true** figures are **9 shipped / 4947 total**; the matrix above uses the true values. Zero critical is unaffected by the double-count.

## Licence compatibility matrix

| Reference | Licence (SPDX) | Project licence | Compatible | Risk |
|---|---|---|---|---|
| pgvector | PostgreSQL | Apache-2.0 | YES | low |
| pgvectorscale | PostgreSQL | Apache-2.0 | YES | low |

The PostgreSQL License is a permissive licence compatible with Apache-2.0. No AGPL/GPL/BUSL reference is present (satisfies TheoDB D1 — Apache/MIT/BSD/PostgreSQL only, AGPL forbidden in the distribution).

## Code similarity (Phase 3) — all 74 production files scanned (100%)

17 findings, **all low/info**, ZERO critical/high:

- Highest: `dtype.rs` structural_clone 0.9 (info) vs pgvector — the vector data type implements the same PostgreSQL type interface, so structural similarity is an interface constraint, not a copy.
- Others: structural clones / textual references / import patterns in the vector-index files (`vec.rs`, `sbq.rs`, `ann/hnsw.rs`, `am/*`) vs pgvector/pgvectorscale — implementing the same well-known algorithms (HNSW, SBQ quantization) and the same PG index-AM interface.
- Consistent with **D3 fork policy**: reuse techniques from permissive references, write own code.
- **The v0.105.0 columnar min/max work (`columnar.rs`, `columnar_agg.rs`, `df_executor.rs`, `columnar_codec.rs`) produced ZERO similarity findings** — it is the columnar engine (own-code), a distinct domain from the references' vector indexes; its symbols (`directory_minmax`, `MinCol`, `fold_minmax_bits`) do not appear in any reference.

## Mentions (Phase 3.5)

The only `high` items are 9 SHIPPED mentions, all in `README.md`: **pgvector ×7, langchain ×1, llamaindex ×1**. Assessed **BENIGN**:

- pgvector is TheoDB's **declared permissive FOUNDATION** (CLAUDE.md D1/D3 — "compomos sobre PostgreSQL + extensões maduras… pgvector"). Naming it in the README is correct attribution/positioning, not a leak or a hidden copy.
- langchain / llamaindex are ecosystem-integration references.
- None is a competitor whose code is being concealed.

No remediation required. (Heuristic caveat: `scan_mentions.py` classifies the public README as a `shipped` surface via an npm-pack heuristic; TheoDB is a Rust project with no npm/`dist/` artifact, but the GitHub README is genuinely public, so the classification is technically correct — the *content* is benign positioning.)

## Git provenance (Phase 4)

**No bulk introduction of reference-derived code.** The structural-clone files were developed incrementally over many commits (`vec.rs` 16, `ann/hnsw.rs` 13, `sbq.rs` 7, `dtype.rs` 6), not a single copy commit. Direct evidence: `d45e628 feat(m69): tipo vetorial proprio theodb.vector own-code`. No commit message indicates copy/port/vendor from the references.

## Remediation plan

None required. The audit confirms:
1. All references are permissive and Apache-2.0-compatible (D1 satisfied).
2. No copied code at critical/high severity; structural similarity reflects shared interfaces/algorithms (D3 fork policy).
3. Mentions are legitimate foundation/ecosystem/prior-art references.
4. Git history shows own-code development, no bulk copy.

**Advisory (optional):** de-duplicate the `mention_findings` table before the next run (the scan appends), or add a `--replace` step, so raw counts match reality.
