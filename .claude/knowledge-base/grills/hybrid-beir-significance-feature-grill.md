---
slug: hybrid-beir-significance
generated_by: roadmap-feature
milestone_id: M123
date: 2026-07-20
status: completed
---
# Grill — hybrid-beir-significance (M123)

Derived from verified codebase context (harness `benchmarks/run_m53_hybrid_beir.py` + `docs/benchmarks/m53-hybrid-beir.md` already exist).

- **Q1 (what/why now):** O benchmark hybrid-BEIR existe mas reporta médias sem significância — não prova que o ganho hybrid vs vector-only é real (não ruído). Why now: "performance é claim, não opinião" (regra TheoDB 5) + lente council-ai-in-db.
- **Q2 (deps):** M53 (hybrid/RRF) — `[x]`.
- **Q3 (DoD):** per-query nDCG@10/recall@k hybrid E vector-only sobre dataset BEIR permissivo; teste pareado (bootstrap/Wilcoxon) com p-value+efeito+IC; honestidade anti cherry-pick (todas as queries, fração de perdas, honest-negative se não-signif.); artefato reproduzível.
- **Q4 (novos riscos):** (1) dataset BEIR grande/licença → subset permissivo pequeno (SciFact/NFCorpus); (2) resultado não-significativo → honest-negative sem ajustar dataset/k p/ "dar significativo".
