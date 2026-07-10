# Gap 1 — extendCandidates no build: recall 0.974→0.990 (fecha a degradação por escala) — honesto sobre o frontier

**Data:** 2026-07-10 · Droplet c-8, pg17.10 · 500k×768d, gaussian-mixture (256 clusters), cosine, GT exato ·
Reprodutível: `benchmarks/run_m60_recall.py` (theodb) + `run_m60_pgvector_control.py` (pgvector). Raw:
`docs/benchmarks/gap1-extend-candidates/*.json`. Método: **white-box** (anatomia do grafo) → localização → fix.

## Como chegamos aqui (o método deep-research que funcionou)

7 levers black-box (rebuild+benchmark) refutados sem apontar a causa. Trocamos para **white-box**: um analisador
que replica o build do theodb e mede a ESTRUTURA do grafo por escala (local, sem droplet). Resultado decisivo:

| escala | out-degree L0 | classe da miss (ef=64) | meanHop entry→trueNN |
|---|---|---|---|
| 10k | 32/32 (cheio) | **100% ROTEAMENTO** | 5.0 |
| 30k | 32/32 | 100% roteamento | 7.3 |
| 80k | 32/32 | 100% roteamento | **9.6** |

→ **Conectividade perfeita; o defeito é ROTEAMENTO** (grafo conexo mas mal-navegável; hop-distance cresce com a
escala). Causa paper-grounded: faltava **`extendCandidates`** (Malkov-Yashunin — recomendado para dados
**extremamente clusterizados**, exatamente nosso regime de 256 clusters). Fix: estender o pool de candidatos com
os vizinhos-dos-vizinhos antes do `select_from`.

## Resultado MEDIDO a 500k×768d (o gate real, não o modelo)

| ef | theodb baseline | **theodb +extendCandidates** | pgvector |
|---|---|---|---|
| 200 | 0.898 | **0.952** | 0.988 |
| 400 | — | 0.972 | 0.992 |
| 1000 | 0.974 | **0.990** | 0.994 |
| **f32 best** | 0.974 | **0.990** | 0.994 |
| **SBQ best** | 0.986 | **0.994** | 0.994 |

## Veredito HONESTO (sem inflar — o white-box foi otimista)

- **✅ GANHO REAL — a degradação de recall por escala FECHOU:** a curva de recall inteira subiu ~5pt; o f32 vai de
  0.974 → **0.990** e o SBQ de 0.986 → **0.994** (= paridade de VALOR de recall com pgvector 0.994). Antes, o f32
  platôava em 0.974 e **não alcançava 0.99 a 500k** — agora alcança. Correctness limpa (63/63 pg_tests GREEN).
- **❌ NÃO é paridade de FRONTIER:** a pgvector ainda tem recall MAIOR no mesmo `ef`/latência (ef=200: 0.988 vs
  0.952). A iso-recall 0.988, o theodb precisa de ~5× o `ef` → **~1.8× mais lento**. O `extendCandidates` subiu o
  teto, não igualou a eficiência recall-por-ef.
- **⚠️ Custo:** o build ficou ~2-3× mais lento (pool de candidatos maior por insert) — erode a vantagem de
  build-speed que tínhamos. Trade-off: **recall-quality priorizado sobre build-speed** (recall era o eixo do North
  Star onde estávamos atrás).

**Honestidade (Regra 3):** o white-box a 80k saturava (recall→1.000), então o fix parecia perfeito; a 500k a
verdade apareceu — teto fechado, frontier ainda da pgvector. É um ganho de recall real e shipável, **não** paridade
de latência.

## Follow-up (a caça ao frontier)

O resíduo (per-ef ainda atrás) aponta que a heurística `select_from` ainda difere da `SelectNeighbors` exata do
pgvector (`keep_pruned` + a ordem exata da diversidade — Qdrant `graph_layers_builder.rs` / hnsw_rs). Próxima
iteração white-box: refinar a diversidade (não o extend) pra buscar a paridade de frontier. Tunar `extendCandidates`
como reloption (default off = build rápido; on = +recall p/ dados clusterizados) também é follow-up.
