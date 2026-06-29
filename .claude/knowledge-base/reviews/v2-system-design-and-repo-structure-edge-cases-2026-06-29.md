# Discover Edge Case Review — v2-system-design-and-repo-structure

Date: 2026-06-29
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/v2-system-design-and-repo-structure-plan.md
Research questions analyzed: 7
Edge cases found: 3 (MUST FIX: 1, SHOULD TEST: 1, DOCUMENT: 1)

Paths verified: all 33 cited `references/` paths exist (0 fabricated). Licenses verified from each LICENSE file.

## MUST FIX

### EC-1: paradedb (the PRIMARY ref) AND citus are AGPL-3.0 — the plan only flagged vectorchord (D1)
- **Affected:** In-Scope table, Out-of-Scope table, ADR D-licensing; Q1/Q2/Q4/Q5/Q6/Q7 (all cite paradedb).
- **Family:** License / D1 distribution gate.
- **Scenario:** verified licenses — **AGPL-3.0:** `paradedb/LICENSE` ("GNU AFFERO GENERAL PUBLIC LICENSE", `paradedb/Cargo.toml` `license = "AGPL-3.0"`), `citus/LICENSE` (AGPLv3), `vectorchord/LICENSE` (AGPLv3). **Permissive (D1-OK):** cloudnative-pg (Apache), duckdb (MIT), supabase-postgres (PostgreSQL License), pgvectorscale (PostgreSQL License), pg_mooncake (MIT), hydra (Apache). The plan marks ONLY vectorchord as "AGPL — pattern only", but paradedb (the **primary** reference) and citus are equally AGPL. CLAUDE.md D1 bars AGPL from the distribution; copying/deriving their **code** into TheoDB's Apache distribution is forbidden.
- **Impact:** if the blueprint lifted code (not just structure) from paradedb/citus, it would inject AGPL into an Apache distribution — a release-blocking D1 violation. Even for a structure discovery, the boundary must be explicit so `/discover-execute` does not copy code bodies.
- **Why it's still safe to use them:** this discovery studies **organizational structure** (folder layout, crate/workspace boundaries, layering taxonomy) — a non-copyrightable *method/idea*, observed clean-room — NOT code expression. Studying "they split `src/` into catalog/execution/storage" or "they use a cargo workspace with member crates" and applying the GENERAL pattern is legitimate regardless of license. The risk is only if execute copies AGPL *source*.
- **Suggested fix (≤1 sentence):** amend the plan to mark **paradedb + citus + vectorchord as AGPL → STRUCTURE/PATTERN-observation-only (no source copied/derived; observe folder layout + layering ideas, never code bodies)**, and note the permissive refs (cloudnative-pg/duckdb/supabase-postgres/pgvectorscale/pg_mooncake/hydra) may inform code patterns too — add an explicit ADR D4 "License-aware investigation (D1)".

## SHOULD TEST

### EC-2: over-structuring risk (YAGNI) — don't impose paradedb's 6-crate workspace at M17's 1-crate scale
- **Affected:** Q2 (workspace layout), the blueprint's "proposed target tree" + Recommendations.
- **Family:** Premature abstraction / scale mismatch.
- **Suggested checkpoint:** the blueprint MUST make the proposed structure **scale-appropriate + milestone-keyed** — TheoDB has ONE Rust crate today (`theodb_rs`); paradedb's 6-member workspace (`pg_search, tests, tokenizers, benchmarks, macros, stressgres`) is its END state. The Recommendations must say WHEN to introduce a workspace (e.g., at the 2nd crate, M18/M20) and WHEN to split, citing the small-extension contrast (pgvectorscale/pg_mooncake/hydra are single-crate) — not cargo-cult the full workspace now. Assert: the proposal includes an incremental ordering tied to M17→M24, not a big-bang FAANG tree imposed immediately.

## DOCUMENT

### EC-3: the proposed target tree is a RECOMMENDATION, not a mandate (already D3 — reinforce)
- **Family:** Scope / decision authority.
- **Accepted risk:** restructuring touches the whole repo; the blueprint's target tree + migration ordering are inputs to a later `/to-plan` + a binding ADR, applied incrementally (never big-bang). D3 already states this; the blueprint's Recommendations section must restate it so a reader does not treat the proposal as an approved migration.

## Summary

| Question | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|
| Q2 | (EC-1) | EC-2 | EC-3 |
| Q1/Q4/Q5/Q6/Q7 (cite paradedb/citus) | EC-1 | 0 | 0 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT — 1 MUST FIX (D1: paradedb+citus are AGPL → structure-only, add ADR D4); 1 SHOULD-TEST (scale-appropriate/incremental structure, anti-YAGNI); 1 DOCUMENT (proposal-not-mandate, reinforces D3).
