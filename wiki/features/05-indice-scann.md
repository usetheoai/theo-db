---
type: Feature
title: Índice ScaNN — o que é entregue e o que é API-alvo
description: Não existe access method theodb_scann; a técnica ScaNN-inspired entregue é IVF com Asymmetric Hashing sobre o theodb_ivfflat, e a superioridade de QPS foi medida como inalcançável.
resource: git:f7c7b93:docs/features/05-indice-scann.md
tags: [feature, indice, scann, ivf-aq, asymmetric-hashing, honestidade]
feature_status: parcial — IVF-AQ entregue, superfície scann não entregue
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat05
    resource: git:f7c7b93:docs/features/05-indice-scann.md
    title: Criar um índice ScaNN
---

Esta página é o exemplo mais claro da disciplina de honestidade do projeto: ela documenta uma
superfície **que não existe** e diz por quê.

# Não existe `USING scann`

**Não há** access method `theodb_scann` nem extensão com esse nome. A decisão de **não construí-lo** é
o [ADR 0004](/decisions/0004-scann-fork-decision.md), tomada por gate de benchmark. A superfície
`USING scann (…)` que aparece em material de roadmap é **API-alvo condicional**, não entrega.

# O que é entregue: IVF + Asymmetric Hashing em código próprio

A técnica inspirada no [ScaNN](/technologies/scann.md) que existe hoje é IVF com quantização e
Asymmetric Hashing, exposta pelo [índice IVFFlat](/features/03-indice-ivfflat.md):

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE INDEX products_scann_like_idx
ON products
USING theodb_ivfflat (description_embedding theodb_ivfflat_l2_ops)
WITH (lists = 1000, pq_subspaces = 16, pq_bits = 4, separate_storage = 1);

SELECT * FROM products
ORDER BY description_embedding <=> theodb.embed('wireless headphones')
LIMIT 10;
```

Isso combina listas invertidas com quantização e scan em lote sobre LUT SIMD, atingindo **paridade de
recall** classe-pgvector.

# O veredito medido — e por que não é um gap a fechar

**Superar o ScaNN e o [AlloyDB](/technologies/alloydb.md) em QPS vetorial é estruturalmente
NÃO-ALCANÇÁVEL** por uma extensão PostgreSQL permissiva. O gap de ~25 a 44× a recall 0,99 é de
**paradigma**, não de tuning, e tem duas componentes:

1. o **AH-LUT anisotrópico** do ScaNN, com anos de tuning; e
2. o fato de o ScaNN **não pagar o imposto de MVCC, WAL e heap** que qualquer extensão paga.

Isso está registrado nos vereditos [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md),
[ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md) e
[ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md), e levou ao reposicionamento formal do
north star no [ADR 0033](/decisions/0033-north-star-reposition-proposal.md).

**É um limite de paradigma documentado, não uma pendência.** A entrega é paridade de recall mais
eficiência de memória — nunca "mais rápido que o ScaNN".

# Por que o índice ainda vale a pena

O caminho IVF-AQ **é lossless**: o recall é byte-idêntico ao IVF exato nos ajustes medidos, com o
pruning por AH servindo como filtro de candidatos. E ele agrega **compressão de memória** — 16 bytes
por vetor contra 512 em precisão plena, isto é 32× nos códigos — sem custo de recall. Esse é o
benefício real: **footprint, não velocidade**.

# Histórico útil

A capacidade "qualidade-ScaNN" foi originalmente entregue por
[DiskANN](/technologies/diskann.md) via [pgvectorscale](/technologies/pgvectorscale.md), conforme o
ADR 0004. Ambos foram **removidos** no [ADR 0029](/decisions/0029-m70-drop-pgvector.md) — qualquer
instrução para usar `USING diskann` hoje é histórica e não se aplica.
