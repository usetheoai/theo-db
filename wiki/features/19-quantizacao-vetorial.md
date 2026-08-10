---
type: Feature
title: Quantização vetorial (compressão dos índices ANN)
description: Reloptions compartilhados pelos três access methods, com kernels próprios de SBQ, AQ anisotrópico, Asymmetric Hashing e RaBitQ — o ganho é memória, e isso é dito explicitamente.
resource: git:f7c7b93:docs/features/19-quantizacao-vetorial.md
tags: [feature, quantizacao, sbq, rabitq, asymmetric-hashing, memoria, reloptions]
feature_status: entregue
milestone: M22+M51+M59+M83+M85+M86
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat19
    resource: git:f7c7b93:docs/features/19-quantizacao-vetorial.md
    title: Quantização vetorial
---

**Status: entregue.** A quantização é configurada por **reloptions** compartilhados pelos três access
methods próprios — [HNSW](/features/02-indice-hnsw.md), [IVFFlat](/features/03-indice-ivfflat.md) e
[SymQG](/features/17-indice-symqg.md) —, todos registrados num único tipo de opção, com cada AM lendo
apenas o que implementa.

Os kernels são **próprios**, sem dependências novas: SBQ, AQ anisotrópico, Asymmetric Hashing com LUT16
e [RaBitQ](/technologies/rabitq.md) em variante sem precisão plena.

# O que a quantização compra — e o que não compra

**Ela compra memória. Ela não compra QPS.** Essa frase é o resumo de uma linha inteira de investigação
medida:

| Tentativa | Veredito |
|---|---|
| SBQ inline no carrier HNSW | **refutado** — 0,35–0,77× do QPS do f32 ([ADR 0018](/decisions/0018-m57-sbq-inline-not-superior.md)) |
| AQ anisotrópico + AH no carrier HNSW | **paridade**, mesmo com o layout corrigido ([ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md)) |
| RaBitQ, o melhor quantizador permissivo | **viável**, ganho é **memória**, não QPS ([ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md)) |
| IVF-AQ+AH como access method completo | **lossless e correto**, sem ganho de QPS medível ([ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md)) |

Os ganhos de **tamanho**, esses sim, são consistentes e medidos: RaBitQ **3,28× menor** a paridade de
recall ([verdict](/benchmarks/e1-rabitq-inpg-verdict.md)); SQ8-refine **3,5× menor**
([m85](/benchmarks/m85-sq8-refine.md)); e paridade de SBQ contra a referência permissiva
([m22](/benchmarks/m22-sbq-parity.md)).

# A lição de layout que vale para qualquer quantização

**Co-localizar os códigos com os vetores de precisão plena anula o ganho.** Se o código de 4 bytes vive
no mesmo tuple que o vetor de 3 KB, ler o código **pagina o vetor inteiro** — e o working set não
encolhe.

Por isso existe a reloption `separate_storage`, e por isso ela importa mais que o número de bits.

# Reloptions

```sql
CREATE INDEX itens_q
ON itens
USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)
WITH (
  lists = 1000,
  pq_subspaces = 16,
  pq_bits = 4,          -- só aceita 4
  separate_storage = 1, -- o que faz o ganho materializar
  refine = ...          -- rerank exato sobre o top-k
);
```

Outras opções disponíveis: `sbq_bits` e `aq_threshold`. Sem nenhuma delas, o índice guarda os vetores
em precisão plena.

# Posicionamento

O que pode ser dito: **paridade de recall e memória para escala grande**. O que **não** pode: "mais
rápido que o ScaNN". O teto é de paradigma e está medido nos vereditos
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md) e
[ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md), e formalizado no reposicionamento
do [ADR 0033](/decisions/0033-north-star-reposition-proposal.md).

# Licença

Todos os algoritmos são **reimplementações próprias** de trabalhos permissivos. O core vendorizado de
RaBitQ e seu destino estão registrados nos ADRs
[0032](/decisions/0032-vendor-rabitq-rs-core.md) e [0046](/decisions/0046-rabitq-vendor-tree-deleted.md).
