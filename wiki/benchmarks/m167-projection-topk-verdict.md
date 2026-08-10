---
type: Measurement
title: m167 — top-k de projeção: veredito, terceira revisão
description: Um artefato que supersede os próprios rascunhos e rastreia, artefato por artefato, qual binário produziu cada número — a proveniência levada ao limite.
resource: git:f7c7b93:docs/benchmarks/m167-projection-topk-verdict.md
tags: [benchmark, columnar, top-k, proveniencia, revisao, m167]
milestone: M167
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m167p
    resource: git:f7c7b93:docs/benchmarks/m167-projection-topk-verdict.md
    title: M167 — projection top-k verdict
    last_modified: 2026-07-28
---

**Revisão 3 — supersede os rascunhos 1 e 2**, com a razão da supersessão documentada no próprio artefato.

Um documento que **substitui as próprias versões anteriores e explica por quê** é mais confiável que um
que aparece pronto: ele mostra que os números foram revisados, e contra o quê.

# A proveniência levada ao limite

O artefato mantém uma **tabela de artefatos**, dizendo, **para cada arquivo bruto**, **o que ele
sustenta** e **qual instância do servidor o produziu** — inclusive marcando explicitamente quais foram
coletados **antes** de um commit específico.

Isso responde uma pergunta que quase nenhum benchmark responde: **este número específico veio do binário
que a conclusão descreve?**

Sem essa rastreabilidade, uma coleta feita antes de uma correção pode contaminar um veredito posterior —
e ninguém saberia.

# O gate de tipos anexo

O veredito exige, como artefato obrigatório, **a matriz de cobertura de tipos** — porque a mudança toca
caminhos de admissão de roteamento, e a
[lição do espaço cego](/benchmarks/m163-type-coverage-verdict.md) tornou esse gate obrigatório para essa
classe de mudança.

**Um gate que passa a ser exigido por classe de mudança, e não por decisão caso a caso**, é o que impede
que a mesma classe de bug volte.

# Base

O estado anterior está em [m167 baseline](/benchmarks/m167-baseline-and-routing-facts-2026-07-28.md); a
continuação, em [m168](/benchmarks/m168-streaming-topk-verdict.md).
