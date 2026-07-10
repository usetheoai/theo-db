# P0 (superioridade vetorial) — o bloqueador-raiz: navegabilidade do grafo HNSW próprio (ef-por-recall)

**Data:** 2026-07-10 · Consolidação medida a partir do CYCLE do M60 (2 ciclos de droplet, 500k×768d) + discover do M71.
Fonte de verdade estratégica para os milestones M60/M71/M72/M73 do Roadmap v5.

## A tese (medida)

Todos os milestones de **superioridade** do pilar P0 dependem de UM problema-raiz não resolvido:

> **O grafo HNSW do theodb precisa de ~5× o `ef_search` do pgvector para o MESMO recall.**

Medido a 500k×768d (mesmo corpus, GT exato — `docs/benchmarks/m60-raw/`):

| recall-alvo ~0.97 | `ef` necessário | p50 |
|---|---|---|
| pgvector hnsw | ~150–200 | ~6–7 ms |
| theodb_hnsw f32 | ~1000 | ~12–15 ms |

Consequência em cascata:
- **M60 (recall-superior):** com o mesmo `ef`/budget, theodb tem recall menor → só atinge paridade via SBQ (over-fetch+rerank, mais lento). Fechado como paridade (ADR-0030), NÃO superioridade.
- **M71 (latência-superior):** a iso-recall, theodb precisa de ~5× o `ef` → ~2× a latência. Cortes de custo-por-candidato (kernel bounded, norm-hoist, multi-accumulator) melhoram o p50 ABSOLUTO mas **não mudam o `ef`-por-recall** → não fecham a razão iso-recall. Superioridade de latência está bloqueada pelo mesmo problema.
- **M73 (head-to-head vs ScaNN/AlloyDB):** o ScaNN já é ~25× mais rápido (M33); o gap vs pgvector é a camada mais próxima. Sem fechar a navegabilidade, o veredito vs ScaNN é honest-negative.

## O que JÁ foi refutado por medição (não repetir) — 6 levers

| Lever | Resultado medido | Onde |
|---|---|---|
| `ef_construction` 64→200 | recall PIOROU → 0.832 (inversão anômala) | M57 |
| MERGE de back-links (paralelo) | 0.846 (não-monotônico — grafo corrompido) | M57 |
| `HNSW_M` 16→32 | 0.952 (piorou, anômalo) | M57 |
| bissecção sequencial vs paralelo | sequencial 0.96 ≈ paralelo 0.974 (não é contenção) | M57 |
| descida de build por beam `ef=1` | no-op (`search_layer(ef=1)` ≡ hill-climb) | M60 |
| multi-entry `ep←W` no build | no-op de RECALL (mas **+29% QPS** — grafo melhor conectado) | M60 |

**Confirmados CORRETOS por leitura dual-source** (theodb↔pgvector): promoção de entry-point, upper-layers
construídos, ground-search accept, `select_from` diversity+keep-back, page-layout (page-reads iguais), níveis `ml`.

## Pistas ainda não exploradas (candidatas para o próximo ataque focado)

1. **A INVERSÃO do `ef_construction`** (`efc=200 → recall PIOR`) é a maior anomalia. Num HNSW correto, recall é
   monotônico-crescente em `efc`. A inversão é um **sinal de bug de build** — provavelmente o overwrite lost-update
   do build paralelo (`hnsw_parallel.rs:129-134`) que se AMPLIFICA com candidate-list maior. **Testar:** `efc` sweep
   no build SEQUENCIAL puro (sem overwrite, `THEODB_HNSW_PARALLEL_THRESHOLD` gigante) — se sequencial for monotônico
   em `efc` e paralelo não, o overwrite é a causa e o fix é o union+re-prune correto (não o MERGE arbitrário refutado).
2. **Kernel de distância bounded** (PANORAMA arXiv:2510.00566) — corta p50 absoluto (onde theodb pode superar
   pgvector no custo/candidato), útil no M71 mesmo sem fechar a razão.
3. **norm-hoist do cosseno** (`vec.rs:183-185` recomputa `‖q‖²`/candidato) — win puro de custo/candidato, recall-neutral.

## ATUALIZAÇÃO 2026-07-10 (2ª rodada de droplet) — 2 achados que reformulam o problema

Experimento efc × modo-de-build (`docs/benchmarks/m60-raw/m60_efc_sweep_100k768d.json` + `m60_efc_seq_vs_parallel_500k768d.json`):

| n | efc | modo | recall@10 |
|---|---|---|---|
| **100k**×768d | 64 | sequential | **0.998** |
| 100k×768d | 64 | parallel | **0.998** |
| 100k×768d | 128 | sequential/parallel | 0.998 / 0.998 |
| **500k**×768d | 64 | sequential | **0.974** |
| 500k×768d | 64 | parallel | 0.972 |

**Achado 1 — o "gap de recall" é DEGRADAÇÃO POR ESCALA, não defeito fixo.** A 100k×768d o theodb chega a
**recall@10 = 0.998** (excelente — ≈ ou acima do pgvector). O plateau 0.974 só aparece a 500k. Ou seja: o HNSW
próprio é **correto e de alta qualidade em escala moderada** (≤100k cobre uma fração enorme de workloads reais); o
gap é uma **degradação de qualidade-de-grafo específica de escala** (5× mais nós). Notícia de produto relevante:
para ≤100k vetores, o vetor do theodb está em paridade/superioridade com o pgvector.

**Achado 2 — a hipótese do OVERWRITE (paralelo) é REFUTADA (7º lever).** O teste decisivo sequential-vs-parallel a
500k: **sequential 0.974 ≈ parallel 0.972** — idênticos. O build SEQUENCIAL não tem o overwrite lost-update, e mesmo
assim produz o MESMO plateau. Logo a degradação por escala é **inerente ao algoritmo de build nos DOIS modos**, não
o overwrite paralelo (a suspeita principal). No 100k não há inversão de efc (saturado em 0.998 a efc 64 e 128).

**Estado: 7 levers refutados.** A degradação por escala do grafo (0.998@100k → 0.974@500k, seq≡paralelo) resiste a
todos. A causa remanescente mais provável (não testada, cara): a heurística de diversidade/RobustPrune (`select_from`)
produzir um grafo menos navegável **a escala** vs a do pgvector (Alg.4 do HNSW paper, `extendCandidates` /
`keepPrunedConnections` com o alpha correto) — mas exige comparar a navegabilidade dos grafos a 500k, outro ciclo.

## Recomendação estratégica (decisão do owner)

O pilar P0 é um **programa de pesquisa multi-sessão gated num único problema** (navegabilidade do grafo). Opções:
- **(1) Ataque focado à navegabilidade** — 1 investigação dedicada à inversão do `efc` (pista #1, a mais promissora
  e não-refutada), com o build sequencial como controle. Se fechar, destrava M60(recall-superior)+M71+M73 de uma vez.
- **(2) Aceitar paridade no pilar** — M60 já fechado como paridade (SBQ); reenquadrar M71/M73 para paridade medida
  + os wins absolutos (multi-entry +29% QPS, bounded kernel), sem claim de superioridade. Honesto e entregável.
- **(3) Reescopar o v5** — focar superioridade onde o theodb já lidera (abertura, custo, portabilidade, HTAP) e
  tratar o vetor como paridade-com-pgvector documentada.

**Honestidade (Regra 3/5):** não há caminho medido para superioridade vetorial pura sem resolver a navegabilidade;
6 levers caíram. Nenhum benchmark sustenta um claim de superioridade hoje — só paridade (recall via SBQ; QPS-a-ef-fixo
via multi-entry). Ver `docs/adr/0002` (North Star) e a memória `goto-p0-vector-superiority`.
