---
type: Measurement
title: m14 — DiskANN contra a barra de qualidade-ScaNN (avaliação do gatilho de fork)
description: Mede se o substituto permissivo já atinge a barra de recall, o que decide se um fork é autorizado; atravessa a barra, mas com ressalva de dataset que mantém a decisão provisional.
resource: git:f7c7b93:docs/benchmarks/m14-scann-fork-decision.md
tags: [benchmark, diskann, scann, fork-gate, recall, m14]
milestone: M14
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m14
    resource: git:f7c7b93:docs/benchmarks/m14-scann-fork-decision.md
    title: M14 — ScaNN-quality / fork-trigger evaluation
    last_modified: 2026-06-28
---

**A pergunta é de política, não de curiosidade.** A política de fork autoriza construir um access
method nativo **apenas** quando um benchmark reproduzível mostra o substituto permissivo insuficiente.
Este benchmark é essa medição.

**A barra:** `recall@10 ≥ 0,90` a QPS usável — a faixa que o [ScaNN](/technologies/scann.md) e o
[DiskANN](/technologies/diskann.md) ocupam nas suítes públicas. **É uma barra de recall**; as features de
memória e compressão do ScaNN são eixo separado.

# Medido

n=5000, dim=32, cosseno, k=10, 3 runs, seed fixa, gaussiano sintético:

| Índice | Params | recall@10 | QPS | build |
|---|---|---|---|---|
| DiskANN | sls=500 | **0,9340** | 229,9 | 3025 ms |
| DiskANN | sls=1000 | **0,9780** | 154,8 | 3025 ms |
| HNSW | ef=40 | 0,9740 | 3878,6 | 735 ms |
| HNSW | ef=100 | 0,9990 | 2019,9 | 735 ms |
| IVFFlat | probes=5 | 1,0000 | 1278,5 | 33 ms |

**O DiskANN atravessa a barra de qualidade-ScaNN** — 0,934 e 0,978. Logo o substituto permissivo basta, e
o fork **não** é autorizado.

# Por que a decisão ficou provisional

Duas ressalvas registradas explicitamente, e são elas que sustentam o status:

- **Os números são gaussianos sintéticos em dim=32**, abaixo da dimensionalidade real de embeddings.
  Gaussiano é **desfavorável** ao DiskANN — o que torna a travessia da barra **conservadora** —, mas não
  é representativo.
- **"Qualidade-ScaNN" aqui está escopado a recall.** O eixo de memória e compressão **não** é
  reivindicado.

E o alvo de referência é **citado, não reproduzido** no repositório — a distinção entre número medido e
número citado é mantida.

# O que aconteceu depois

A decisão correspondente é o [ADR 0004](/decisions/0004-scann-fork-decision.md), que foi **reaberta** pelo
[ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) quando o mandato mudou para código
próprio. O projeto acabou construindo os seus próprios access methods, e o veredito final do eixo está no
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md).

Note que já aqui o HNSW aparece dominando o DiskANN em QPS a recall comparável — sinal consistente com a
[decisão de índice default](/decisions/m2-index-decision.md).
