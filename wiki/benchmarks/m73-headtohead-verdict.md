---
type: Measurement
title: m73 — o veredito consolidado do pilar vetorial
description: Consolida fronteiras já medidas em vez de re-rodar o adversário, e justifica por que re-medir seria anti-sunk-cost — o adversário não mudou.
resource: git:f7c7b93:docs/benchmarks/m73-headtohead-verdict.md
tags: [benchmark, veredito, consolidacao, scann, north-star, m73]
dataset: SIFT1M
milestone: M73
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m73
    resource: git:f7c7b93:docs/benchmarks/m73-headtohead-verdict.md
    title: M73 — Head-to-head MEDIDO vs ScaNN/AlloyDB
    last_modified: 2026-07-10
---

A consolidação que produz o veredito rastreável do north star, formalizado no
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md).

# Por que consolidar em vez de re-rodar

O documento justifica a escolha metodológica:

As três melhorias anteriores — [recall](/benchmarks/m60-hnsw-recall.md),
[multi-entry](/benchmarks/m71-scan-latency.md) e [multi-cliente](/benchmarks/m72-qps-multiclient.md) —
foram **todas no carrier de precisão plena**. **Nenhuma tocou o paradigma de quantização**, que é a
vantagem do adversário.

Portanto o veredito é emitido a partir das **fronteiras já medidas em dataset real**, **sem re-rodar o
adversário** — porque **ele não mudou**, e re-medir apenas reconfirmaria um gap de paradigma que
**quatro medições independentes já estabelecem**.

**Isso é anti-sunk-cost aplicado à medição**, não à construção: gastar uma corrida cara para reconfirmar
o que já se sabe é o mesmo desperdício que construir o que já se sabe não pagar.

E o rigor que sustenta a escolha: **cada linha do veredito tem artefato**. A consolidação não inventa
números — ela referencia medições existentes, cada uma reproduzível.

# O veredito

Paridade own-code de recall **alcançada**; superioridade de QPS sobre o adversário **medida como
não-alcançável** por extensão permissiva, por gap de paradigma; e o trade-off documentado, com o
throughput multi-cliente competitivo-a-superior **num regime declarado**.

Os detalhes, o posicionamento permitido e o proibido estão no
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md).
