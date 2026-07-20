---
slug: async-embed-vectorizer
generated_by: roadmap-feature
milestone_id: M122
date: 2026-07-20
status: completed
---
# Grill — async-embed-vectorizer (M122)

Derived from verified codebase context (not a live interview — the answers were established by reading the code).

- **Q1 (what/why now):** O worker do vectorizer roda o embed HTTP dentro da txn de processamento do job → um endpoint pendurado prende o xmin horizon até o timeout (~90s), atrasando o VACUUM. `vectorizer.rs:15-16` declara o "fully async embed" como follow-up rastreado. Why now: hardening pós-roadmap-completo, eixo operabilidade+AI-native.
- **Q2 (deps):** M54 (vectorizer/job-queue, ADR-0016) — `[x]`. O groundwork DRY (`resolve_cfg`, `run_batch` em embed.rs) já existe.
- **Q3 (DoD):** 3-fases (claim+commit lease → embed sem txn com cfg resolvido → write em txn nova); MEDIDO que o xmin não segura durante o HTTP; crash-safety B↔C idempotente; path síncrono `ai.embed` inalterado.
- **Q4 (novos riscos):** (1) config drift read↔write → passar cfg resolvido pela fase B; (2) crash pós-HTTP pré-write → reprocessa idempotente (custo = 1 HTTP).
