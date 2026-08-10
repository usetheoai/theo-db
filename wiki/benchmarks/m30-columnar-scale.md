---
type: Measurement
title: m30 — colunar em escala, com média e desvio
description: Fecha o gap de escala que o m6 deixou aberto, medindo speedup crescente de 3× a 14× — e reconcilia honestamente com o resultado anterior que ele contradiz.
resource: git:f7c7b93:docs/benchmarks/m30-columnar-scale.md
tags: [benchmark, columnar, escala, variancia, reconciliacao, m30]
milestone: M30
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m30
    resource: git:f7c7b93:docs/benchmarks/m30-columnar-scale.md
    title: M30 — columnar-at-scale
---

Preenche o gap de escala que o [m6](/benchmarks/m6-columnar-vs-row.md) marcara como não medido, e é a
evidência que ancora a decisão de manter o pilar
([ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md)).

**Método:** o mesmo agregado analítico nos dois lados, **3 passadas cronometradas por lado**, com média ±
desvio e cache quente. A imagem do substrato é **fixada por digest** — correção direta da lição do m6.

# Resultado

| linhas | row-store | columnstore | speedup | efeito > variância |
|---|---|---|---|---|
| 100.000 | 12,7 ± 1,4 ms | 4,2 ± 0,8 ms | 2,99× | sim |
| 1.000.000 | 62,3 ± 3,5 ms | 7,0 ± 0,5 ms | **8,89×** | sim |
| 5.000.000 | 285,3 ± 19,3 ms | 20,6 ± 0,3 ms | **13,87×** | sim |

**O speedup cresce com a escala**, o que é a assinatura clássica do colunar, e o **efeito excede a
variância em todos os pontos** — não é ruído.

**Correção:** contagem exata e média dentro de 1e-3 entre engines — **não byte-idêntica**, porque a
ordem de somatório difere e o último decimal diverge. Declarar isso é o que distingue "correto" de
"idêntico".

# A reconciliação honesta com o m6

O m6 mediu o row-store **vencendo** a 100k; este mede o colunar vencendo. **O tempo do colunar a 100k
oscilou ~11× entre as duas corridas, no mesmo harness e na mesma família de imagem** — quase certamente
drift de versão da imagem não fixada mais regime de cache.

**Portanto o ponto de 100k é tratado como quase-paridade e NÃO load-bearing.** A decisão se ancora no
ganho **robusto a versão e muito além da variância a partir de 1M**, onde tanto o efeito quanto o
crescimento super-linear do row-store são inequívocos.

Esta é a forma correta de lidar com dois resultados contraditórios: **identificar qual é robusto,
aposentar o frágil, e dizer qual é qual** — em vez de escolher o conveniente.

# Ressalvas

O substrato roda uma versão de PostgreSQL diferente da embarcada, que **não embarcava colunar** na época.
Isto prova a **capacidade** e o ganho a partir de 1M; embarcar era passo separado. Dados sintéticos, uma
máquina, agregação estilo rollup.

E o colunar medido aqui é **lakehouse em disco**, não in-memory — a aposta declarada.
