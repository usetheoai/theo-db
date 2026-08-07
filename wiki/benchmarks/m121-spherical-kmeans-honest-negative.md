---
type: Measurement
title: m121 — k-means esférico: no-op provável, não apenas medido
description: Uma hipótese vinda de revisão foi testada e revertida; para uma das métricas o resultado não é só medido como idêntico, é demonstrável que teria de ser.
resource: git:f7c7b93:docs/benchmarks/m121-spherical-kmeans-honest-negative.md
tags: [benchmark, kmeans, ivf, honest-negative, prova, m121]
milestone: M121
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m121
    resource: git:f7c7b93:docs/benchmarks/m121-spherical-kmeans-honest-negative.md
    title: M121 — IVF cosine/ip spherical k-means
    last_modified: 2026-07-20
---

**Veredito: honest-negative. Revertido, sem código embarcado — este documento é o registro da
investigação.**

# A hipótese

Uma revisão anterior propôs **normalizar os centroides sobre a esfera unitária** para elevar o recall das
listas invertidas em métricas angulares, que media 0,83–0,89 contra 1,0 do grafo
([m49](/benchmarks/archive/m49-cosine-ip-opclasses.md)).

A hipótese vinha de literatura estabelecida e era plausível. **Foi testada por medição, não assumida.**

# O resultado, e por que ele é mais forte que "medimos e não mudou"

**Para o cosseno: no-op PROVÁVEL.** Não é apenas que a medição deu igual — é **demonstrável** que teria
de dar, porque a normalização do centroide não altera a ordenação induzida por uma métrica que já é
invariante a escala.

**Para o produto interno: medido idêntico.**

**Uma prova é melhor que uma medição** quando disponível: ela fecha a questão em vez de deixá-la
dependente do regime testado. E o artefato distingue os dois casos em vez de tratá-los igual.

# A disciplina

O critério previa explicitamente: *"se o lift não justificar, reverter e registrar honest-negative"*. **A
reversão estava no plano**, então executá-la não foi derrota — foi o gate funcionando.

**Zero código embarcado**, e o conhecimento fica registrado para que a mesma proposta não retorne sem
evidência nova.
