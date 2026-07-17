---
slug: m110-graph-surface
milestone_id: M110
date: 2026-07-16
cycle: review
---

# /review — M110 in-DB graph extraction surface

**Verdict:** READY_TO_MERGE

Two independent adversarial reviewers: **council-security** (untrusted text + LLM path = Risk a) +
**council-ai-in-db** (extraction port fidelity — the recall gate).

## council-security — M110's own code is defensively SOUND

- **SQL injection (Q1): SOUND** — every untrusted value in `_graph_upsert` is bound `$1..$7` via
  `Spi::*_with_args`; no `format!` splices untrusted text into a production query (ADR-3 holds).
- **Prompt injection (Q2): LOW** — text newline-collapsed into the user role, protocol in system role, reply
  PARSED never executed, blast radius = the caller's own workspace (no escalation/cross-workspace write).
- **Least privilege (Q3): SOUND** — all three wrappers REVOKE'd from PUBLIC; internals gated by schema-USAGE.
- **Resource/DoS (Q6): LOW** — entities capped 64, edges window-bounded, one bounded LLM round-trip.
- **Error handling (Q7): SOUND** — typed `ereport`, no panic across C, no swallowed errors.

### Findings — pre-existing platform gaps (NOT introduced by M110; filed as issues, tracked)
- **[MEDIUM #117] SSRF** — `theodb.llm_endpoint` is a caller-settable GUC with no internal-address block. This
  affects ALL `ai.*` LLM functions since M0, not M110 specifically. M110 adds a new reachable caller. Filed
  `#117`. Follow-up: register the GUC `Suset` + private-IP denylist. Out of M110 scope.
- **[MEDIUM #118] tenant-blind `graph_build`/`graph_expand`** — folds the whole shared edge table with no
  workspace predicate (pre-existing M108 design). No cross-tenant *leakage* (node-ids disjoint) but not
  partitioned. M110's *tables* are correctly ws-scoped (`m110_upsert_tenant_isolation` proves it). Filed `#118`.
  Follow-up: per-workspace CSR view. Already flagged in the impl summary + benchmark md.

## council-ai-in-db — the recall gate HOLDS (byte-identical on English/ASCII); 2 MEDIUM FIXED

- **Port fidelity (Q1): PASS on realistic English** — tokenization, cap-run, separator flush, stopword trim,
  <3-char drop, dedup/cap-64/first-appearance, windowed co-occurrence + canonical src≤dst all byte-identical.
- **Normalization (Q2): PASS** — Turkish-I not a divergence (locale-independent `to_lowercase`); exotic
  whitespace unreachable (spans are alnum tokens joined by ASCII space).
- **Upsert semantics (Q4): mention_count/weight sum, source_chunk_ids union, orphan drop, idempotency all PASS.**
- **E2E (Q5): PASS** — extract→upsert→build→expand reconstructs the undirected co-occurrence graph.

### Findings — FIXED
- **[MEDIUM] `description` not updated on ON CONFLICT** → on heuristic-then-LLM re-ingest of the same edge, the
  LLM description was lost (theo-rag keeps first-non-empty). **FIXED:** added
  `description = COALESCE(NULLIF(theodb.graph_edges.description,''), EXCLUDED.description)`. Proven by
  `m110_upsert_description_upgrades_from_empty`.
- **[MEDIUM] "100% parity coverage" over-claim** (3 ASCII fixtures ≠ input space) → **FIXED:** wording corrected
  to "byte-identical on ASCII/English fixtures; non-ASCII edge cases documented, not tested" (bench json + md).
- **[LOW] LLM edges not deduped / fallback not canonicalized** → **FIXED:** `parse_llm_extraction` now dedups +
  canonicalizes edges via the same accumulator as the heuristic path.

### Findings — accepted (documented LOW)
- **[LOW]** Unicode parity divergences (Rust `is_uppercase`/`is_alphanumeric`/`chars().count()`/codepoint-order
  vs JS `\p{Lu}`/`\p{L}\p{N}`/UTF-16-length/UTF-16-order) — real but confined to non-ASCII (Roman numerals,
  combining marks, astral chars) absent from English RAG corpora, and benign for the undirected CSR. Documented
  in the benchmark md + module scope note.

## Hard gates
✅ no BLOCKER · ✅ no secrets · ✅ no main commit · ✅ commit-trailer policy honored · ✅ CHANGELOG updated ·
✅ benchmark artifact (throughput + parity) · ✅ security posture (parameterized-data-only, REVOKE, newline-
collapse) · 347 pg_tests GREEN (+11 M110, 0 regression).

## DoD (ROADMAP M110)
(1) `graph_expand` pg_extern+REVOKE+wiring ✅ · (2) `ai.extract_graph/entities` reusing `ai.*` ✅ · (3) idempotent
upsert ✅ · (4) theo-rag integration proof (E2E) ✅ · (5) extraction-quality baseline measured (100% parity +
throughput) ✅ · security (Risk a) ✅ (M110 code sound; 2 pre-existing platform gaps filed #117/#118).
