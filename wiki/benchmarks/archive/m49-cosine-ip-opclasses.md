---
type: Measurement
title: m49 — opclasses de cosseno e produto interno: recall e crash-safety
description: Usa o scan exato da MESMA métrica como oráculo, que é gate mais forte que comparar dois aproximados entre si.
resource: git:f7c7b93:docs/benchmarks/archive/m49-cosine-ip-opclasses.md
tags: [benchmark, opclass, cosseno, crash-safety, oraculo, m49]
milestone: M49
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m49
    resource: git:f7c7b93:docs/benchmarks/archive/m49-cosine-ip-opclasses.md
    title: M49 — cosine + inner-product opclasses
---

**A escolha de oráculo é o ponto metodológico.** O recall é medido contra o **scan exato da mesma
métrica** — e o documento justifica: **um oráculo exato é gate mais forte que comparar aproximado com
aproximado**. Um head-to-head contra a referência fica como follow-up.

# Recall contra o oráculo exato

| Access method / métrica | recall@10 |
|---|---|
| HNSW / cosseno | **1,0** |
| HNSW / produto interno | **1,0** |
| IVFFlat / cosseno | 0,89 |
| IVFFlat / produto interno | 0,83 |

Todos acima do gate de 0,80, com o grafo perfeito e as listas invertidas na faixa esperada para a
configuração medida.

# Crash-safety, e por que ela vem de graça

Provado por teste versionado: matar o processo com sinal e reiniciar preserva o **top-5 idêntico** sob o
operador de cosseno — **a métrica é preservada, não corrompida para L2**.

E há um argumento estrutural: **cosseno e produto interno usam formato de página IDÊNTICO ao de L2**,
guardando f32 bruto. Logo **a maquinaria de crash-safety do fold cobre as novas opclasses por
construção** — e o teste prova isso ponta a ponta para uma delas.

**Herdar uma garantia por identidade de formato, e verificar que a herança vale**, é mais barato e mais
sólido que reimplementar a garantia.

Estas opclasses fecham o follow-up que o [ADR 0010](/decisions/0010-m26-index-am-scope.md) deixara em
aberto — o escopo l2-primeiro.
