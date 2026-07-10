# M71 — Latência do scan do AM: melhoria medida (multi-entry build) + veredito iso-recall honesto

**Data:** 2026-07-10 · Droplet c-8 (8 vCPU), pg17.10 · 100k/500k×768d, gaussian-mixture, cosine, GT exato ·
Reprodutível: `benchmarks/run_m60_recall.py` (theodb) + `run_m60_pgvector_control.py` (pgvector). Raw:
`docs/benchmarks/m60-raw/m71_*`, `m60_theodb_f32_fix2_multientry_500k768d.json`.

## Deliverable shipado: multi-entry `ep←W` no build → +29% QPS, recall-neutral

O fix implementado no M71 é carregar o **conjunto completo `W`** da busca como entry-set entre camadas do build
(Malkov-Yashunin INSERT Alg.1 `ep ← W`; pgvector `HnswFindElementNeighbors` `ep = w`) em vez de colapsar a um único
nó. Produz um grafo melhor-conectado. **Medido (recall-neutral):**

| n=500k×768d, ef=1000 | recall@10 | QPS |
|---|---|---|
| baseline (`ep=selected.first()`) | 0.974 | 64.2 |
| **multi-entry (`ep←W`)** | 0.972 | **82.8 (+29%)** |

Validação de correção: **63/63 pg_tests `hnsw` GREEN** com o fix; recall inalterado (0.996 @100k, 0.972 @500k). É
uma melhoria de throughput de query real e recall-neutral — **shipada**.

## Veredito iso-recall (honesto — NÃO superioridade)

O gate rigoroso do M71 é **latência a iso-recall** (mesmo ponto de recall), não QPS a `ef` fixo. Medido a 100k×768d:

| recall ~0.996 | ef | p50 |
|---|---|---|
| **pgvector** | 100 | **2.13 ms** |
| **theodb (multi-entry)** | 200 | 3.16 ms |

**A iso-recall, o theodb NÃO é latência-superior — nem em paridade.** pgvector atinge 0.996 com `ef=100`; o theodb
precisa de `ef=200` (~2× o `ef`) → ~1.5× a latência a 100k. A 500k o gap piora para ~5× o `ef` (~1.7× a latência).
Causa: a **mesma lacuna de navegabilidade do grafo** que gateia o M60 (o grafo do theodb precisa de mais `ef` por
recall que o do pgvector, e piora com escala — `docs/benchmarks/p0-vector-superiority-root-blocker.md`). Cortes de
custo/candidato (kernel bounded, norm-hoist — blueprint M71) reduzem o p50 ABSOLUTO mas **não mudam a razão
iso-recall** (a razão é a navegabilidade, não o custo/candidato).

## Veredito do milestone (DoD reenquadrada — ADR-0031)

A DoD original do M71 (`p50 ≤ pgvector a recall≥0.99` = superioridade) é **honest-negative**: a superioridade de
latência a iso-recall está gateada na lacuna de navegabilidade (compartilhada com o M60, 7 levers refutados). O M71
**entrega a melhoria medida** (multi-entry +29% QPS, recall-neutral, shipada) e **documenta honestamente** que a
superioridade/paridade a iso-recall NÃO foi atingida. Reenquadrada (measurement-first, como o M60/ADR-0030):
**M71 = melhoria de latência medida e entregue; superioridade iso-recall = follow-up gated na navegabilidade.**
Sem claim de superioridade (`public-copy.md`).
