---
slug: m60-hnsw-recall-quality
milestone: M60
date: 2026-07-10
verdict: SHIPPABLE
cycle: discover
---

# Blueprint M60 — Fechar o gap de recall do HNSW próprio (recall≥0.99 a escala)

Diagnóstico linha-a-linha (same-graph) do teto de recall do `theodb_hnsw` (~0.96–0.974 a
100k–500k×768d, `ef`-insensível) vs pgvector (0.978–0.992 no mesmo dado). Deep-research
com evidência de código dos DOIS lados + SOTA web (R0). Refuta 3 hipóteses e crava a
causa-raiz na **qualidade das arestas do build**, não na busca nem no entry-point.

## Objetivo

Isolar a origem do gap e definir o fix mínimo, **provável localmente** (same-graph
micro-bench a 20k–50k, sem droplet), que leve recall@10 a ≥0.99 sem regressão inaceitável
de QPS. Confirmação a 500k×768d é o único passo droplet-gated.

## Evidência que fixa a CLASSE do bug (antes do ranking)

O ef-sweep do `theodb_hnsw_f32` a 500k×768d (`docs/benchmarks/m57-raw/m57p_ef1000.json`) é
a prova:

| ef_search | 200 | 400 | 800 | 1000 |
|---|---|---|---|---|
| theodb f32 recall@10 | 0.898 | 0.956 | 0.958 | **0.974 (teto)** |
| pgvector (clustered) | 0.974 | 0.99 | 0.992 | 0.992 |

Recall sobe com `ef` e **satura ~0.974 mesmo a ef=1000** (pool de 1000 sobre 500k). Essa é
a assinatura definicional de um **teto de conectividade/alcançabilidade do grafo**: para
~2.6% das queries o k-NN verdadeiro está numa região que o beam **nunca alcança** pelas
arestas construídas — alargar o beam (mais `ef`) não compra nada. pgvector na MESMA
distribuição passa de 0.99 → o dado não é o problema e a maquinaria da ground-search não é
o problema; **o grafo que o build produz está sub-conectado**. A bissecção do M57
(sequencial 0.96 ≈ paralelo 0.974) já eliminou contenção de lock e localizou no algoritmo.

## Causa-raiz (ranqueada — file:line dos dois lados)

### #1 (MAIS PROVÁVEL) — descida upper-layer do BUILD é `greedy_descend` (hill-climb sem backtracking), não um beam

- **theodb:** `theodb_rs/src/ann/hnsw_parallel.rs:108-116` (paralelo, o caminho de escala) e
  `theodb_rs/src/ann/hnsw.rs:155-163` (sequencial). A descida usa `greedy_descend`
  (`hnsw_parallel.rs:168-196`, `hnsw.rs:192-211`) — hill-climb de UM só `ep`, move ao vizinho
  mais próximo até não melhorar. **Sem beam `ef` na descida, sem backtracking.**
- **pgvector diverge em** `hnswutils.c:1304-1308` (`HnswFindElementNeighbors`, Alg. 1): a
  descida `ef=1` ainda é `HnswSearchLayer` (`hnswutils.c:822-985`) — um best-first **beam** com
  heap de candidatos `C` e de resultado `W`, que **retrocede** quando um nó mais próximo
  aparece atrás de um ridge local.
- **Por que gera plateau `ef`-insensível:** em 768d os upper-layers são esparsos e com
  ridges; a descida greedy do theodb pousa no basin errado para uma fração dos inserts;
  esses nós selecionam vizinhos de layer-0 do bairro ERRADO → seus verdadeiros vizinhos
  nunca os linkam → inalcançáveis em query-time. O defeito está nas **arestas**, não na
  largura da busca; mais `ef` expande mais arestas erradas, nunca a que falta.
- **Fix mínimo:** trocar `greedy_descend` no caminho de BUILD por um `search_layer` de
  largura 1 (o beam que já existe), espelhando `HnswSearchLayer(..., ef=1, ...)`. Sites:
  `hnsw_parallel.rs:108-111` + `hnsw.rs:156-159`. A descida de QUERY-time
  (`hnsw_page.rs:1565-1581`) tem a mesma forma greedy e deve receber o mesmo tratamento —
  mas o build vem primeiro (um beam de query não alcança arestas que nunca foram construídas).

### #2 (PROVÁVEL, compõe com #1) — `select_from` descarta arestas podadas; pgvector as MANTÉM

- **theodb:** `theodb_rs/src/ann/hnsw.rs:257-283` (dup em `hnsw_parallel.rs:255-279`): aplica a
  heurística de diversidade (Alg. 4) e só faz top-up `if kept.len() < m` — candidatos que
  falham a heurística e não são necessários são **descartados**.
- **pgvector diverge em** `hnswutils.c:1149-1151` (`SelectNeighbors`): `/* Keep pruned
  connections */` — recolhe os rejeitados em `wd[]` e **reenche `r`** até `lm`. Um nó pgvector
  sempre preenche todos os `m`/`m0` slots quando há candidatos — aresta "redundante" ainda é
  aresta de **alcançabilidade**. theodb pode deixar nós com **menos de `m`** vizinhos.
- **Fix mínimo:** após a passada da heurística, reenche `kept` a partir dos candidatos
  REJEITADOS (não um novo scan nearest) até `kept.len() == m`. ~5 linhas. Dedup a duplicata
  entre os dois arquivos (DRY).

### #3 (PLAUSÍVEL, amplificador só-paralelo) — overwrite lost-update no build paralelo

- **theodb:** `theodb_rs/src/ann/hnsw_parallel.rs:129-134`: `nn[layer] = selected.clone()`
  sobrescreve, clobberando back-links que outras threads empurraram.
- **pgvector:** `hnswbuild.c:382-408` (`UpdateNeighborsInMemory`→`HnswUpdateConnection`,
  `hnswutils.c:1181-1229`) nunca sobrescreve; update bidirecional simétrico sob lock,
  re-rodando `SelectNeighbors` para encolher. **Nunca perde back-link.**
- **Nuance honesta:** a bissecção do M57 mostra sequencial (sem overwrite) TAMBÉM em 0.96 →
  isto é **amplificador**, não a raiz. O MERGE tentado no M57 (`m57_recallfix.json`→0.846)
  falhou porque mesclou back-links ARBITRÁRIOS **sem re-podar**; o merge correto é
  union+`select_from` (re-prune), como o pgvector. Reabordar só DEPOIS de #1+#2.

### #4 (REFUTADOS — honest-negative, NÃO perseguir)

- **Promoção do entry-point está CORRETA** nos dois builds: paralelo `hnsw_parallel.rs:159-164`,
  sequencial `hnsw.rs:186-189`; pack em `hnsw_page.rs:789-793` grava `entry_blkno/offno/level`
  = semântica `HNSW_UPDATE_ENTRY_ALWAYS` (`hnswbuild.c:243-244`, `hnswutils.c:359-364`). O nó
  de maior nível É o entry persistido. **Hipótese-topo do brief refutada.**
- **Upper-layers SÃO construídos** (não é grafo flat layer-0): níveis `-ln(rng)/ln(m)`
  (`hnsw.rs:77-79`), o loop linka todo layer `lc` de `level.min(max_level)` a 0
  (`hnsw_parallel.rs:113-156`); codec empacota slots por-layer (`hnsw_page.rs:605-617`,
  `decode_neighbors:563`, mesma matemática `(level-lc)*m` do pgvector `hnswutils.c:784`).
- **Beam accept/terminate da ground-layer está CORRETO:** `scan_core.rs:138-162` usa o break
  `cd > worst && result.len() >= ef` e accept `nd < worst || result.len() < ef` —
  byte-equivalente ao pgvector `hnswutils.c:892,911,934`. Teste D3
  `ground_search_matches_brute_exact_knn` (`scan_core.rs:277-291`) prova kNN exato a ef alto
  **num grafo bom**. A busca NÃO é o teto.

## Coverage corners

### Corner 1 — Integration tests (como provar o fix)

Estender o harness `MemNeighborSource` (`theodb_rs/src/ann/scan_core.rs`) com um teste
same-graph a 20k×128d, 200 queries, GT brute-force:
- **`reachable_fraction`**: para cada query, o top-1 verdadeiro é alcançável por BFS sobre a
  adjacência layer-0 construída, a partir do entry? Hipótese #1 prevê greedy < 1.0, beam →1.0.
- **`recall@10` a ef=500**: A/B build-greedy-descent vs build-ef1-beam-descent nos MESMOS
  levels/seed. Prevê greedy plateau <1.0, beam →~1.0.
- **histograma de out-degree** por layer: build atual left-skewed com massa em degree < m0; a
  variante keep-back (#2) preenche a m0. Correlacionar nós degree-deficientes com as vítimas
  de #1 — devem sobrepor.
Roda em segundos localmente (criterion/`cargo pgrx test`), isola o build de todo I/O.

### Corner 2 — Dependencies

Nenhuma nova dependência. Fix é reuso de `search_layer` já existente + ~5 linhas em
`select_from`. Parsimony ladder rung 4 (reusar o que já existe). pgvector permanece como
oráculo de controle no benchmark (removido da distribuição no M70, instalável só no bench).

### Corner 3 — Tools

- Micro-bench local: `cargo pgrx test pg17` + harness criterion same-graph (M46/M47 pattern).
- Confirmação a escala (droplet): `benchmarks/bench_ann_index.py` / `run_m57_pressure.py`
  (build+measure em duas fases, ≥3 runs, mean±std) a 500k×768d clustered casado com pgvector.

### Corner 4 — Techniques (SOTA / R0 web)

- Malkov & Yashunin 2016 (arXiv:1603.09320) — INSERT Alg. 1: a descida `l_c=L..l+1` roda
  SEARCH-LAYER (ef=1) e atualiza `ep`; é **beam**, não hill-climb puro. Confirma o fix #1.
- pgvector `HnswSearchLayer`/`SelectNeighbors` (github pgvector/src/hnswutils.c) — o keep-back
  `wd[]` e o `HNSW_UPDATE_ENTRY_ALWAYS`. Confirma #2 e refuta o suspeito de entry-point.
- "Down with the Hierarchy: the H in HNSW stands for Hubs" (arXiv:2412.01940) — a hierarquia
  contribui pouco a escala; o que importa é a **conectividade do layer-0** — reforça que a
  raiz é a qualidade das arestas do layer-0 (nosso #1/#2), não a estrutura de níveis.
- TopLoc (arXiv:2504.21507) / HM-ANN (NeurIPS'20) — entry-point query-adaptivo / promoção
  bottom-up: alternativas de teto, mas trade-off dataset-dependente; NÃO necessárias aqui
  (nosso entry-point já está correto — refutado).

## ADR do blueprint

**ADR M60-1 — Fix é qualidade de aresta no BUILD, não busca nem entry-point.**
- Decisão: implementar #1 (descida beam ef=1 no build) + #2 (keep-back no `select_from`),
  provar same-graph a 20k–50k, confirmar a 500k. Reabordar #3 (merge re-prune) só se residual.
- Alternativas rejeitadas: (a) entry-point query-adaptivo (TopLoc) — refutado, entry já
  correto; (b) mexer no beam de query/`ef_search` — refutado, ground-search é exato num bom
  grafo; (c) subir `m`/`ef_construction` — refutado por medição no M57 (piorou).

## Acceptance / halt

- Todo claim tem citação theodb file:line + pgvector file:line + (SOTA quando aplicável).
- Fix provável localmente (same-graph), confirmação a 500k é o único gate droplet.
- Honest-negative aceito: se #1+#2 não cruzarem 0.99 same-graph, reabrir para #3 / técnica nova.

## ADENDO — resultado MEDIDO (2026-07-10, droplet c-8, 500k×768d) — hipótese #1 REFUTADA

O discover acima (dual-source + SOTA) apontou #1 (descida de build por beam ef=1) como causa-raiz mais provável.
**Implementado e medido a 500k×768d: no-op.** Recall byte-idêntico ao pré-fix (ef=1000 → 0.974). Motivo:
`search_layer(ef=1)` só admite candidatos melhores que o melhor atual → **não retrocede** → é equivalente ao
hill-climb; e a descida do pgvector também é ef=1 → a descida **nunca foi** a diferença. Fix revertido (Regra 3).

**Controle pgvector no MESMO corpus (decisivo):** pgvector best recall@10 = **0.988** — *também abaixo de 0.99*.
Logo o gate 0.99 é **artefato do dado** (256 clusters gaussianos apertados em 768d), não um defeito theodb.
Gap real f32↔pgvector = **~1.4pt** (0.974 vs 0.988); **SBQ over_fetch=32 = 0.986 (≈ paridade)**. Relatório completo +
raw: `docs/benchmarks/m60-hnsw-recall.md`, `docs/benchmarks/m60-raw/`. **DoD do M60 deve virar paridade-pgvector**,
não 0.99 absoluto. Próximo lever (não perseguido): `select_from` keep-back ordering (pgvector `wd[]`
`hnswutils.c:1149-1151`). M60 **permanece aberto**.
