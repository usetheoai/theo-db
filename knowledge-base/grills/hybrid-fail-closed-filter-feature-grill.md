---
slug: hybrid-fail-closed-filter
milestone_id: M120
generated_by: roadmap-feature
mode: batch-evidence-derived
status: completed
date: 2026-07-20
---

# Feature grill — M120 Filtro estruturado fail-closed p/ ai.hybrid_search_rrf

> Batch/evidence-derived. Answers grounded in `.claude/knowledge-base/backlog.md` + shipped code (verified
> against the current tree post-M118, not a stale audit). Out-of-scope cross-check: ROADMAP.md has no
> `### Explicitly out of scope` section → vacuously satisfied.

## Q1 — What / why now
M53 (hybrid/RRF) [x]

## Q2 — Dependencies (must be [x])
BLOCKER latente de segurança: filter_sql cru sob INVOKER vira escalonamento sob DEFINER/tenant isolado; fail-closed estruturado é a única defesa real (council-security F1). Now: segurança/AI-native é eixo diferenciador pós-ADR-0033.

## Q3 — Definition of done
(1) filtro estruturado (col/op/valor allowlist, quote_ident+bind, sem SQL cru); (2) fail-closed erro tipado + teste negativo; (3) payload (SELECT count) rejeitado; (4) filter_sql cru = opt-in caller-privilege doc.

## Q4 — Top 2 new risks
(1) expressividade limitada vs SQL cru (KISS, filter_sql opt-in cobre resto); (2) mudança de assinatura quebra callers (manter opt-in retrocompat).
