---
type: Measurement
title: m6 — colunar contra row-store, a 100k
description: A primeira medição do pilar colunar; o row-store venceu nesta escala, e esse resultado foi depois marcado como superado e não load-bearing.
resource: git:f7c7b93:docs/benchmarks/m6-columnar-vs-row.md
tags: [benchmark, columnar, historico, superado, m6]
milestone: M6
status: deprecated
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m6
    resource: git:f7c7b93:docs/benchmarks/m6-columnar-vs-row.md
    title: M6 — Columnar vs row-store, measured
    last_modified: 2026-06-28
---

**A primeira medição do pilar colunar**, e o gate measurement-first que informaria se a peça deveria
entrar na imagem.

# O que foi medido

Uma tabela de 100.000 linhas com 5 categorias, com o **mesmo agregado analítico** rodado nos dois lados:

```sql
SELECT category, count(*), round(avg(amount)::numeric,4)
FROM <tabela> GROUP BY category ORDER BY category;
```

| Caminho | Plano | Latência |
|---|---|---|
| row-store | `Sort → HashAggregate → Seq Scan` | **10,9 ms** |
| columnstore | **custom scan vetorizado** | 44,3 ms |

**O row-store venceu**, e a explicação oferecida na época foi que o overhead de setup domina nessa
escala.

**Correção verificada:** o agregado do columnstore **iguala** o do row-store, grupo a grupo.

# O substrato, declarado

A medição roda sobre a **distribuição canônica** da extensão, numa imagem descartável — que traz uma
versão de PostgreSQL diferente da embarcada. Um build a partir do código-fonte para a versão embarcada
**foi tentado e falhou** num descompasso de toolchain — problema **resolúvel**, não lacuna de capacidade.

Logo: a capacidade fica provada, e **embarcar é o passo gated**.

# Por que este número saiu de circulação

O [m30](/benchmarks/m30-columnar-scale.md) mediu o **oposto** a 100k, com o colunar vencendo — um swing
de ~11× no **mesmo harness e na mesma família de imagem**, atribuído a drift de versão e regime de cache.

Por isso o [ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md) trata o ponto de 100k como
**quase-paridade e explicitamente NÃO load-bearing**, ancorando a decisão no ganho robusto a partir de 1M.

**Este resultado é registrado como superado e incerto, e não é citado como evidência** — o que é
diferente de apagá-lo. A trajetória do pilar terminou no
[colunar próprio](/features/14-analitico-colunar.md).
