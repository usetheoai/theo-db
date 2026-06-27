# Blueprint: Vector Recall@k Benchmark Harness — FAANG-grade Methodology

> **Discovery verdict:** `SHIPPABLE_WITH_CAVEATS` (89/100, `/discover-confidence` M2 — 0 hard caps, 4/4 corners, soft cap `citation_density_low` advisory, 2026-06-27)
> **Slug:** `vector-recall-benchmark-harness` · **Created:** 2026-06-27 · **Owner:** paulohenriquevn (CTO)
> **Plan:** `.claude/knowledge-base/discoveries/plans/vector-recall-benchmark-harness-plan.md`
> **Method note (honesty):** deep-read dos análogos OSS (`pgvector`, `pgvectorscale`) + reconstrução do
> padrão SOTA **ANN-Benchmarks** (Aumüller et al.) de fontes primárias allowlisted (repo + paper arXiv:1807.05614).
> Toda afirmação de performance carrega método+fonte ou `UNBENCHMARKED` (R3); fontes inalcançáveis → `BLOCKED` (R6).

## Context

O ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (LOCKED) e o `ROADMAP.md` M2 elevaram o **harness de benchmark recall@k a 1º item / gate** do pilar killer — nenhuma decisão de índice (adotar pgvectorscale / forkar / ScaNN-as-PG-AM) nem claim de performance antes dele. A discovery `alloydb-vector-ai-implementation` provou que **nenhum harness de recall reproduzível existe** para herdar (tudo `UNBENCHMARKED`). `public-copy.md` + PRD D3 proíbem claim sem benchmark reproduzível. Esta discovery fecha o **como medir** em grau FAANG antes de escrever o harness.

## Objective

Habilitar a implementação de um **harness reproduzível** que meça **recall@k + latência (p50/p95/p99) + QPS + build-time + tamanho de índice** de um índice vetorial em PostgreSQL, com **ground-truth exato** e **rigor estatístico (best-of-N, seeds)**, de forma que TheoDB afirme paridade/superioridade vs ScaNN/AlloyDB **com evidência**. Sucesso: recall@k definido com semântica SOTA correta (distance-threshold), protocolo de latência/QPS ancorado, esqueleto executável citado, verdict `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS.

## Coverage Corner 1 — Integration Tests

Como os análogos validam recall contra um PostgreSQL real — o padrão de teste que o harness herda (`rules/testing.md` pirâmide: unit p/ a matemática + integration contra PG real).

- **Oráculo de ground-truth = exact = mesma query com índice desligado.** pgvector roda `SELECT i FROM tst ORDER BY v <op> '$q' LIMIT k` **antes** de criar o índice (seqscan = brute-force exato), guarda os ids como `@expected`; depois cria o índice, força `SET enable_seqscan=off`, e um `EXPLAIN ANALYZE` **assegura que o Index Scan é realmente usado** (guard contra medir exact de novo). Citações: `.claude/knowledge-base/references/pgvector/test/t/012_hnsw_vector_build_recall.pl`, `.claude/knowledge-base/references/pgvector/README.md` (§ "exact nearest neighbor … perfect recall"; § "Monitor recall by comparing approximate with exact").
- **Recall = sobreposição de conjuntos / k**, threshold de aceite **0.99** (`<->`/`<=>`/`<+>`) e **0.97** (`<#>` inner product), k=20. Citação: `.claude/knowledge-base/references/pgvector/test/t/012_hnsw_vector_build_recall.pl`.
- **Variante tie-aware (importante):** em vez de `LIMIT 20` cru, o oráculo é "toda linha com distância ≤ a 20ª menor" (CTE `top` com `enable_indexscan=off`), evitando penalizar o índice por empates. Citação: `.claude/knowledge-base/references/pgvector/test/t/044_hnsw_iterative_scan_recall.pl`. Testa também recall sob filtro (`WHERE i % c = 0`).
- **Padrão client-side (psycopg) contra PG real:** connect → `CREATE EXTENSION IF NOT EXISTS vector` → `CREATE TABLE (embedding vector(d))` → insert → `CREATE INDEX` → `ORDER BY embedding <=> %s::vector LIMIT k`. Citações: `.claude/knowledge-base/references/pgvectorscale/tests/test_basic_operations.py`, `.claude/knowledge-base/references/pgvectorscale/tests/conftest.py`. **Gap:** estes asseguram contagem/limites de distância, **nunca comparam com exact** → não medem recall.

## Coverage Corner 2 — Dependencies

Dependências necessárias ao harness (mínimas, permissivas — sujeitas a `/deps-audit` antes de entrar no pacote, D1).

| Dep | Versão (fonte) | Papel | Citação |
|---|---|---|---|
| `numpy` | ANN-Benchmarks pin `2.2.4`; pgvectorscale tests `>=1.20` | geração de vetores + **ground-truth brute-force (BLAS)** | `.claude/knowledge-base/references/pgvectorscale/tests/requirements.txt` |
| `psycopg2-binary` | `>=2.9` | driver Postgres sync (boundary SQL) | `.claude/knowledge-base/references/pgvectorscale/tests/requirements.txt` |
| `h5py` | `3.13` | I/O dos datasets HDF5 (train/test/neighbors/distances) | ANN-Benchmarks `requirements.txt` (github, allowlist) |
| `scikit-learn` | `1.6.1` | split seeded (`train_test_split`, `random_state`) | ANN-Benchmarks `requirements.txt` |
| `psutil` | `7.0.0` | medição de memória | ANN-Benchmarks `requirements.txt` |
| `pytest` | `>=7` | runner de testes (unit + integration) | `.claude/knowledge-base/references/pgvectorscale/tests/requirements.txt` |

**Gap honesto:** `run-python-tests.sh` usa `asyncpg` **não declarado** no requirements (dep oculta de runner). Sem `h5py` nos análogos → confirma que **não há carga de dataset HDF5** lá (datasets são synthetic numpy). `NEEDS-DEPS-AUDIT`: rodar `/deps-audit` (osv-scanner + pip-audit) antes de pinar. Citação: `.claude/knowledge-base/references/pgvectorscale/scripts/run-python-tests.sh`.

## Coverage Corner 3 — Tools

Aquisição reprodutível de datasets + gate de "PG vivo e capaz".

- **Datasets HDF5 reprodutíveis:** esquema `train (N,d)` / `test (n_q,d)` / `neighbors (n_q,k) int` (índices true-NN) / `distances (n_q,k) float`. Padrões: `glove-100-angular`, `sift-128-euclidean`, `gist-960-euclidean` (`{name}-{dim}-{metric}`). Split seeded `train_test_split(test_size=10000, random_state=1)` → **mesmo ground-truth para todos**. Default k=100. Fonte: ANN-Benchmarks `ann_benchmarks/datasets.py` (github, allowlist) + paper §3.2. **Flag supply-chain:** o host `ann-benchmarks.com/{name}.hdf5` está **fora da allowlist** → para reprodutibilidade + R5, **espelhar o HDF5 localmente** (ou gerar synthetic seeded), nunca depender do host vivo de terceiro.
- **Gate "PG vivo":** `pg_isready -h $H -p $P -U $U -d $DB` como pré-condição fail-fast + `CREATE EXTENSION IF NOT EXISTS` idempotente, params por env. Citação: `.claude/knowledge-base/references/pgvectorscale/scripts/run-python-tests.sh`.
- **Determinismo:** pgvector seeda o build com `SeedRandom(42)` (build determinístico). Citação: `.claude/knowledge-base/references/pgvector/src/hnsw.h` (mapeia `pg_prng`). **Faltando nos análogos:** seed de geração de dataset e de seleção de queries (pgvector usa `rand()` não-seedado) → o harness deve seedar explicitamente.

## Coverage Corner 4 — Techniques

### T1 — recall@k: definição SOTA (distance-thresholded, NÃO id-overlap)

O ANN-Benchmarks **rejeita** o "fração de ids coincidentes" (frágil sob empates) e define recall por **limiar de distância**: o k-ésimo vizinho verdadeiro define o limiar; qualquer retornado a distância ≤ limiar conta.

```
recall(π,π*) = |{ p ∈ π : dist(p,q) ≤ dist(p*_k, q) }| / k
recall_ε     = |{ p ∈ π : dist(p,q) ≤ (1+ε)·dist(p*_k, q) }| / k
```

Implementação (cross-validada): `knn_threshold(data,count,eps)= data[count-1]+eps`; recall computado dos **distances** (`dataset_distances[i]` = distâncias true ordenadas; `run_distances[i]` = distâncias retornadas), `mean(actual)/k`. `epsilon=1e-3` é só tolerância numérica de empate float. Fontes (allowlist): `github.com/erikbern/ann-benchmarks` `ann_benchmarks/plotting/metrics.py` + paper `arxiv.org/abs/1807.05614` §2.1. **R1/R2** ✓. **Insight load-bearing:** uma implementação id-overlap divergiria do padrão da área sob empates/duplicatas — o harness DEVE usar a semântica de limiar de distância.

### T2 — ground-truth exato (brute-force k-NN)

Brute-force exato via BLAS: angular → normaliza L2, `dists = -dot(index, v)`; euclidean → `lengths - 2·dot(index,v)`; seleção n-closest via `numpy.argpartition` + sort exato. Escrito como `neighbors`+`distances` no HDF5. Fontes: `ann_benchmarks/algorithms/bruteforce/module.py` (`BruteForceBLAS`) + `datasets.py` `write_output(...count=100)` + paper §3.2. **R1/R2** ✓. Equivalente ao oráculo "index desligado" do pgvector (Corner 1) — duas fontes independentes do mesmo ground-truth.

### T3 — protocolo de latência/QPS (rigor estatístico)

- **Build do índice 1×; reuso entre grupos de query.** `run_individual_query(...run_count, batch)` repete `run_count` vezes (CLI `--runs`, default **5** — *flag `UNVERIFIED-DEFAULT`: confirmar em `main.py` argparse antes de citar como fato*).
- **Single-query:** mede cada query com `time.time()`; `search_time = total/len(X_test)` (latência média por query); **best-of-N** = `min` entre runs (reduz jitter/scheduler, reporta throughput quase-pico).
- **QPS = 1 / best_search_time** (`queries_per_second`). **Percentis p50/p95/p99/p999** das latências por-query (millis).
- **Single-thread** para fairness (paper §3.5: container isolado + cpuset). **Batch** apresentado separadamente. Distâncias recomputadas após a query (não penaliza quem não retorna distância). **Sem warm-up dedicado** no runner (o min-of-N descarta runs frios implicitamente) — *fato de estrutura, não número*.
- **Reporting padrão:** fronteira de Pareto **Recall (x) × QPS log (y)** — "up and to the right is better"; e Recall × tamanho-de-índice. Fontes: `ann_benchmarks/runner.py` + `plotting/metrics.py` + paper §2.2/§3.4–3.7. **R3:** único número (Annoy ≈1249 QPS @ recall 0.52, GLOVE) é do paper e **ilustrativo**, não herdado.

### T4 — instrumentação existente nos análogos (e o gap)

- pgvector: macros C `HnswBench`/`IvfflatBench` (`instr_time`, gated por `HNSW_BENCH`/`IVFFLAT_BENCH`) cronometram **fases de build/scan/vacuum** via `elog` — exigem recompile, não medem recall nem latência SQL. Citações: `.claude/knowledge-base/references/pgvector/src/hnsw.h`, `.claude/knowledge-base/references/pgvector/src/ivfflat.h`.
- pgvectorscale: `criterion` mede **micro-latência de função de distância e da estrutura list-search-result** (in-process, sem SQL, `rand` não-seedado). Citações: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/distance.rs`, `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/lsr.rs`.
- **Gap central (confirmado):** nenhum componente combina **recall + latência end-to-end + QPS + memória**. A lógica de recall existe só nos testes Perl do pgvector (que não cronometram). É exatamente isto que o harness do M2 constrói novo (ADR 0002 gate). **UNBENCHMARKED** em todos os números até o harness rodar.

## Cross-cutting Comparison

| Aspecto | ANN-Benchmarks (SOTA) | pgvector (testes Perl) | pgvectorscale (criterion/py) | → Harness TheoDB |
|---|---|---|---|---|
| Recall@k | distance-threshold + ε | id-overlap (≥0.99) + variante tie-aware | ausente | **distance-threshold (SOTA)** |
| Ground-truth | brute-force BLAS, HDF5 cacheado | exact = index off (por-run) | ausente | brute-force numpy **cacheado** |
| Latência/QPS | best-of-N, p50/95/99, QPS=1/best | ausente | micro-bench de função | **client-side, best-of-N, percentis** |
| Build/memória | `psutil` + index-size | macros C (recompile) | — | build-time + index size (`pg_relation_size`) |
| Datasets | HDF5 padrão, split seeded | synthetic 3-dim não-seedado | synthetic não-seedado | **synthetic seeded + HDF5 espelhado** |
| Reprodutibilidade | seed split, container 1-thread | `SeedRandom(42)` build | nenhuma | **seed total + conexão única** |

## Recommendations

Por questão, a decisão de design (detalhada nos ADRs):

1. **Linguagem/stack →** Python + `numpy` (ground-truth BLAS) + `psycopg` (boundary SQL) — espelha ANN-Benchmarks e o harness de teste dos análogos (não reinventar, Regra 9).
2. **Métrica de recall →** **distance-threshold** (`d ≤ dist(p*_k,q)+ε`), nunca id-overlap — paridade com o padrão da área (T1).
3. **Ground-truth →** brute-force exato via numpy, **cacheado** (HDF5/npz) com seed; validável contra o oráculo "index off" do pgvector.
4. **Métricas →** recall@k + p50/p95/p99 + QPS (1/best_mean) + build-time + index size; **best-of-N runs**, conexão única, RNG seedado.
5. **Datasets →** começar com **synthetic seeded** + um padrão pequeno (sift-128 subset) **espelhado localmente** (supply-chain; não depender de host vivo).
6. **Saída →** artefato reprodutível (JSON + markdown) em `docs/benchmarks/` — o artefato do gate M2 (ADR 0002).
7. **Pirâmide de teste →** unit (matemática de recall distance-threshold + casos de empate; ground-truth) + integration (contra container `theo-db:dev` real). `/deps-audit` nas deps antes de pinar.

## ADRs

### D1 — Harness em Python (numpy + psycopg), não Rust/C

**Status:** Accepted
**Decision:** o harness é um pacote Python com `numpy` (ground-truth + métricas) e `psycopg` (execução SQL contra o container).
**Rationale:** é a stack do próprio ANN-Benchmarks e do harness de teste dos análogos; numpy BLAS dá ground-truth exato rápido; iterar em Python é o mínimo que resolve (KISS, Regra 9 — não reinventar um framework de bench). Rust/criterion mede micro-latência de função, não recall end-to-end — escopo errado.
**Alternatives:** estender os benches criterion do pgvectorscale (rejeitado — in-process, sem SQL, sem recall); macros C do pgvector (rejeitado — recompile, só fases de build). 
**Consequences:** deps Python sujeitas a `/deps-audit` (D1 licença).

### D2 — Recall distance-thresholded (semântica ANN-Benchmarks), não id-overlap

**Status:** Accepted
**Decision:** `recall@k = |{ retornado d ≤ dist(p*_k,q) + ε }| / k`, computado das **distâncias**, ε=1e-3 (tolerância de empate).
**Rationale:** é a definição SOTA (paper §2.1 + `metrics.py`); id-overlap diverge sob empates/duplicatas. Paridade de medição com a área é pré-requisito de qualquer claim "igual/superior ao ScaNN".
**Alternatives:** id-overlap puro (rejeitado — frágil, não-SOTA), embora seja o que os testes do pgvector usam (aceitável só para 0.99-threshold, não para reportar curva).
**Consequences:** o harness precisa do array de **distâncias true** (não só ids) no ground-truth.

### D3 — Ground-truth brute-force cacheado + seed total

**Status:** Accepted
**Decision:** ground-truth exato via numpy (argpartition + sort), cacheado em disco com seed; geração de dataset e seleção de queries seedadas; conexão única ao medir.
**Rationale:** reprodutibilidade é o ponto do gate (ADR 0002 / public-copy). Os análogos não seedam dataset/queries — débito que herdaríamos se não corrigíssemos.
**Consequences:** runs idênticos com mesmo seed; ground-truth recomputado só quando o dataset muda.

### D4 — Protocolo de medição: best-of-N + percentis + QPS, single-conn

**Status:** Accepted
**Decision:** build 1×; `--runs` repetições (≥3, default a confirmar); latência por-query medida client-side; reportar p50/p95/p99 + QPS=1/best_mean + build-time + `pg_relation_size` do índice; varrer ef_search/probes para a curva recall×QPS (Pareto).
**Rationale:** rigor estatístico FAANG (paper §3.4–3.7); a curva recall×QPS é o reporting que permite comparar com ScaNN.
**Consequences:** saída é uma curva, não um número único; honra `rules/testing.md` (determinístico) e `public-copy.md` (claim com método).

## Blocked questions / honesty register

| Flag | Item | Razão |
|---|---|---|
| `UNVERIFIED-DEFAULT` | `--runs` default = 5 | lido do resumo CLI, não do `main.py` argparse — confirmar no implement |
| `UNBENCHMARKED` | todo número de recall/latência/QPS dos análogos e do AlloyDB | nenhum harness reproduzível existe; é o que vamos construir |
| supply-chain | host `ann-benchmarks.com/*.hdf5` | fora da allowlist → espelhar dataset localmente, não depender do host vivo |
| `NEEDS-DEPS-AUDIT` | numpy/h5py/psycopg/scikit-learn/psutil | permissivas por inspeção; rodar `/deps-audit` antes de pinar (D1) |

## References

**OSS analogs (`.claude/knowledge-base/references/`):** `pgvector/` (`test/t/012_hnsw_vector_build_recall.pl`, `005_ivfflat_query_recall.pl`, `044_hnsw_iterative_scan_recall.pl`, `README.md`, `src/hnsw.h`, `src/ivfflat.h`); `pgvectorscale/` (`tests/test_basic_operations.py`, `tests/conftest.py`, `tests/requirements.txt`, `scripts/run-python-tests.sh`, `pgvectorscale/benches/distance.rs`, `pgvectorscale/benches/lsr.rs`).

**SOTA (allowlist):** ANN-Benchmarks repo — `github.com/erikbern/ann-benchmarks` (`ann_benchmarks/plotting/metrics.py`, `datasets.py`, `runner.py`, `algorithms/bruteforce/module.py`, `requirements.txt`); paper — Aumüller, Bernhardsson, Faithfull, *ANN-Benchmarks*, `arxiv.org/abs/1807.05614`.

**Project rules consumed:** `rules/discover-phd-rigor.md` (R1–R6), `rules/testing.md` (pirâmide unit+integration), `public-copy.md` (claim só com benchmark), ADR `0002-north-star` / PRD D3 (o gate).
