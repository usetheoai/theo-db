---
slug: m105-docs-features-reconciliation
milestone_id: M105
date: 2026-07-16
cycle: review
---

# /review — M105 docs/features reconciliation

**Verdict:** READY_TO_MERGE (after fixes)

Independent review (council-ai-in-db) adversarially verified the reconciliation for over-labeling, signature accuracy, new fabrications, and ScaNN honesty.

## Verified honest
- **No over-labeling:** every `🎯 API-alvo` section was confirmed genuinely unshipped in code (Proxy Model, `theodb_ai_nl.*`, `USING scann`/`theodb_scann`, `USING ivf`, unhonored hybrid keys). No shipped capability was buried to cheat the gate.
- **Signatures accurate:** sampled runnable examples match real code (`theodb.embed(content,model)`, `ai.rerank(query,documents[],model,top_n)→TABLE(idx,score)` idx 0-based, `ai.rank(prompt,model)→real`, AMs + 6 opclasses).
- **ScaNN (05) honest:** documents the shipped IVF-AQ+AH path; states ScaNN-QPS-superiority is measured-negative (ADR-0035/0036), not a gap.

## Findings — all FIXED
- **[MEDIUM] 09:29,39** `CREATE EXTENSION IF NOT EXISTS theodb_ml` / `extname='theodb_ml'` — `theodb_ml` is a schema, not an extension → fixed to `CREATE EXTENSION theodb` + schema note. (The gate missed it due to the `IF NOT EXISTS` variant — gate hardened.)
- **[MEDIUM] 06:188** reversed `theodb.embed('model','content')` → fixed to `theodb.embed(content, model)`.
- **[LOW] 06:158** `lexical_engine` default documented `postgres` → fixed to `ts_rank_cd` (+ typed-error note).

## GATE (hardened, re-run)
Deterministic scan over all 12 files: no fabricated symbol, no extension-vs-schema error, no embed arg-order bug, no wrong `lexical_engine` default in any shipped code block. **12/12 PASS.**

## Hard gates
✅ no BLOCKER · ✅ no secrets · ✅ no main commit · ✅ commit-trailer policy honored · ✅ CHANGELOG updated · docs-only (zero code, zero test regression).

**READY_TO_MERGE.**
