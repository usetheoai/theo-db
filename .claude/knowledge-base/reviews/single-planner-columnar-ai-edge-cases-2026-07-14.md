# Discover Edge Case Review — single-planner-columnar-ai

Date: 2026-07-14
Discovery plan analyzed: `knowledge-base/discoveries/plans/single-planner-columnar-ai-plan.md`
Research questions analyzed: 9
Edge cases found: 4 (MUST FIX: 0, SHOULD TEST: 2, DOCUMENT: 2)

All 26 reference citations were re-verified to exist (`discover-plan-confidence` already confirmed 0 fabricated). No
MUST-FIX: no path is missing, no corner is empty, the budget is respected. The edges below are execution-robustness
(halt-loop budget) + honesty framing (GO/NO-GO), not plan-breakers.

## MUST FIX

(none)

## SHOULD TEST

### EC-1: Q2's Hydra columnar C files are huge — whole-file reads will exhaust the budget
- **Affected question:** Q2 (techniques — columnar TAM storage + MVCC)
- **Family:** Method / Scope
- **Scenario:** `columnar_tableam.c` is **4030 lines**, `columnar_reader.c` **2271**. During `/discover-execute`, a naive "Read the file" on these blows the 6h Hydra budget on one question and buries the visibility/vacuum callbacks in noise.
- **Impact:** budget exhausted before Q3-Q5; blueprint thin on the later techniques.
- **Suggested fix:** add a halt-loop checkpoint — for Q2, Fase A `grep -n` the specific callback names (`columnar_scan_getnextslot`, `columnar_tuple_satisfies_snapshot`, `columnar_relation_nontransactional_truncate`, `RowMask`, `stripe`), then Fase B Reads ONLY those line-ranges, never the whole 4030-line file.

### EC-2: Q6 coexistence (pgrx 0.16.1 + datafusion 54 + arrow 58 in one crate) is NOT provable by reading Cargo.toml
- **Affected question:** Q6 (deps — version compatibility)
- **Family:** Method / Interpretation
- **Scenario:** the plan reads pg_search's pins (`datafusion="54"`, `arrow-*="58.1.0"`, `pgrx.workspace`) — but a version PIN is not proof of coexistence: duplicate-arrow-version resolution, `arrow` symbol/ABI collisions with anything pgrx pulls, and feature-flag conflicts only surface at `cargo build`. `/discover-execute` is read-only research and cannot build.
- **Impact:** the blueprint could assert "compatible" from pins alone — a false-confidence claim (Rule 5).
- **Suggested fix:** Q6's expected answer must be a **version MATRIX + an explicit "coexistence is UNPROVEN until a `cargo tree`/build spike"** flag — the blueprint records it as a downstream feasibility gate for the roadmap's β milestone, NOT as a confirmed compatibility.

## DOCUMENT

### EC-3: Q6/Q7 are the GO/NO-GO feasibility gates — a NO does not waste Q1-Q5
- **Affected question:** Q6, Q7 (deps)
- **Accepted risk:** if datafusion↔pgrx coexistence (Q6) or the `TableAmRoutine` FFI (Q7) comes back NO, the vectorized-executor (β) and native-TAM (α) milestones are blocked — but the techniques findings (Q1 CustomScan seam, Q2 columnar design, Q3 DataFusion model, Q4 Lance, Q5 semantic operators) remain valuable design study, and the roadmap simply pivots (e.g., keep the shipped pg_duckdb + build only the Arrow-cache/AI-operator rungs that don't need a native TAM). The discovery delivers KNOWLEDGE regardless of the feasibility verdict — this is why it precedes any code.

### EC-4: Rust-version fear RESOLVED; use UPSTREAM datafusion, not paradedb's fork
- **Affected question:** Q6 (deps)
- **Accepted risk / correction:** datafusion 54 requires Rust 1.88; **TheoDB's `rust-toolchain.toml` pins 1.91.0 → the Rust-version gate is already satisfied** (record this as a resolved risk, not an open one). Separately, pg_search pulls `datafusion-distributed` from a paradedb git fork (verified **Apache-2.0** — shippable, but a third-party fork); the blueprint must reference **upstream `apache/datafusion`** as the dependency, noting the fork only as pg_search's own choice — TheoDB does not adopt a fork it doesn't control (Rule 9 / supply-chain hygiene).

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 0 | 0 | 0 | 0 |
| Q2 | 1 | 0 | 1 (EC-1) | 0 |
| Q3 | 0 | 0 | 0 | 0 |
| Q4 | 0 | 0 | 0 | 0 |
| Q5 | 0 | 0 | 0 | 0 |
| Q6 | 3 | 0 | 1 (EC-2) | 2 (EC-3, EC-4) |
| Q7 | 1 | 0 | 0 | 1 (EC-3 shared) |
| Q8 | 0 | 0 | 0 | 0 |
| Q9 | 0 | 0 | 0 | 0 |

**Verdict:** DISCOVERY PLAN OK (SHOULD-TEST checkpoints + DOCUMENT ADRs absorbed into v1.1; no MUST-FIX — proceed to `/discover-execute`).
