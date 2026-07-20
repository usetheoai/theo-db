# Licence Compliance Audit — TheoDB

**Date:** 2026-07-19 · **Mode:** full · **Auditor:** loop-check-licence
**Project licence:** Apache-2.0 · **Evidence DB:** `.claude/check-licence-evidence.db`

## Verdict: **CLEAN** (licence-compliance axis)

No licence-compliance violation found. Every substantive overlap with a reference
project is (a) against a **permissive** source (PostgreSQL License, Apache-2.0-compatible)
and (b) **explicitly attributed** in-file. No copyleft (GPL/AGPL) contamination. No
missing-attribution obligation. Git provenance shows incremental own-code development, not
bulk paste. TheoDB's CLAUDE.md D3 "study don't copy" policy **held robustly**.

The only `high`-tagged rows are the 9 mechanical *mention-surface* README hits — compliant
positioning, not code copying (see §3.5). On the licence axis the realized maximum severity
is **low**.

## Severity matrix (all axes)

| Severity | Similarity | Git prov. | Findings | Mentions | Total | Licence-relevant? |
|---|---|---|---|---|---|---|
| critical | 0 | 0 | 0 | 0 | 0 | — |
| high | 0 | 0 | 0 | 9 | 9 | No — README positioning (mechanical over-count) |
| medium | 0 | 0 | 0 | 652 | 652 | No — source/commit mentions of prior-art |
| low | 10 | 0 | 2 | 4286 | 4298 | attributed structural overlaps + internal-doc mentions |
| info | 7 | 0 | 2 | 0 | 9 | wire-format/boilerplate + clean systemic findings |

Coverage: **74/74 production files scanned (100%)** vs 77 reference files across 2 projects.

## Phase 1 — Discovery
- 2 reference projects: **pgvector** (C, id=1), **pgvectorscale** (Rust, id=2).
- 74 production files registered: 62 Rust (`theodb_rs/`, ~31.6k LOC) + 12 SQL (`sql/`).
- Excluded (vendored/generated): all `*.c`/`*.h` outside references (vendored PostgreSQL / pgrx build artifacts), `target/`, benchmark/spike harnesses, auto-generated version-delta SQL.

## Phase 2 — Licence Mapping
| Reference | SPDX | Compatible w/ Apache-2.0 | Risk | Obligation |
|---|---|---|---|---|
| pgvector | PostgreSQL | ✅ | low | retain copyright + permission notice (attribution) |
| pgvectorscale | PostgreSQL | ✅ | low | attribution |

No copyleft in the reference set → theoretical severity ceiling = **medium** (copy without attribution). Never critical/high on the licence axis.

## Phase 3 — Code Similarity Analysis
- 17 similarity findings: structural_clone ×11, textual_reference ×3, import_pattern ×2, near_copy ×1. All **info/low**. By project: pgvector ×10, pgvectorscale ×7.
- **Attribution gaps: NONE.** Every overlapping file names the exact reference `file:line` + PostgreSQL License in-header. The single `near_copy` (0.82, `am/page/mod.rs` `page_get_*` macros) is a reimplementation of **PostgreSQL's own public C macros**, attributed.
- Representative attribution present: `dtype.rs`, `vec.rs` (`*_matches_pgvector_oracle` parity tests), `sbq.rs`, `am/hnsw_page.rs`, `ann/hnsw.rs`, `am/{mod,guc,options}.rs` — all cite reference file:line + Unbreakable Rule 9.
- Systemic (clean): (1) SQL operator/type surface mirrors pgvector conventions — **mandatory** for a pgvector drop-in (100% wire-compat is a product gate), own-code DDL with functions renamed `theodb_vector_*`; (2) distinctive ANN files (`aq.rs`=ScaNN, `ah.rs`=FAISS, `rabitq.rs`=Gao&Long paper, `symqg.rs`=arXiv:2411.12229) derive from **papers/FAISS, not the references** — zero pgvector/pgvectorscale DNA.

## Phase 3.5 — Mention Scan
- 4,947 mentions / 24 terms. Surface: internal_doc 3,267 (low, expected prior-art), source_code 497, commit_history 155, **shipped 9**.
- The 9 shipped hits are all `README.md`: 7× pgvector (factual "pgvector customizado" + benchmark **parity-not-superiority**), 1× langchain + 1× llamaindex (integration roadmap). Compliant with `public-copy.md`; no banned framing ("pilar killer" = killer-feature idiom, not "<competitor> killer").
- **No AGPL project (VectorChord / pgvecto.rs) on any shipped or source_code surface — CLEAN.** This is the single genuinely high-risk vector and it is absent.
- Caveat: mechanical `shipped`/high is the known `npm pack`/root-README workspace over-count.

## Phase 4 — Git Provenance
- Priority vector files (hnsw/hnsw_page/ivf/sbq/pq/aq/scan/build) each introduced in a milestone+task-tagged feature commit, evolved over 7–50 commits (TDD).
- Max lines added in any single commit = 162–634 (feature-sized, not a reference-file dump).
- Commit messages assert own code ("own SBQ scalar quantizer T1.1/T2.1", "own Product Quantization — measured NOT a QPS win").
- Grep for paste-origin phrasing (ported/copied/from/based-on pgvector[scale]) → **zero** paste-origin commits. `git_history_contamination` = none.
- (The `git_commits_analyzed: 0` metadata counter reflects that no *contamination* rows were added — a clean result, not an unrun phase.)

## Remediation plan
No mandatory remediation. Hygiene:
1. **(low) — RESOLVED 2026-07-19.** Top-level `NOTICE` created, aggregating the PostgreSQL-License
   attributions (pgvector, pgvectorscale, PostgreSQL/plpython3u) for the components **redistributed
   in the distribution image** (confirmed: `packaging/Dockerfile.m51-test` builds pinned pgvector;
   pgvectorscale is statically linked into `vectorscale.so`). Reproduces each licence's copyright +
   permission notice verbatim to satisfy the "appear in all copies" obligation at the distribution
   root; cross-references `docs/packaging/license-audit.md` (AGPL due-diligence) rather than
   duplicating it. CHANGELOG `[Unreleased] § Added` updated.
2. **(info)** The `scan_mentions.py` `shipped`/high README over-count is a known tool caveat for non-npm repos; no action needed, documented here so the 9 hits are not mistaken for exposure.

## Reproduction
```
DB=.claude/check-licence-evidence.db
S=/home/paulo/Projetos/plugins/loop-check-licence/scripts
python3 $S/scan_mentions.py --db-path $DB --target . --references-dir knowledge-base/references --json-summary
python3 $S/check_licence_database.py --db-path $DB risk-summary
python3 $S/check_licence_database.py --db-path $DB coverage-stats
```
