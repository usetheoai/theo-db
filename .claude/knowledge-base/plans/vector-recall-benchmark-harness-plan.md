---
slug: vector-recall-benchmark-harness
created_at: 2026-06-27
goal: "Enable o time do TheoDB a medir recall@k + latência/QPS de um índice pgvector contra o container theo-db:dev de forma reprodutível, measured by `pytest benchmarks/` verde E um relatório docs/benchmarks/*-pgvector-*.json com recall@10 medido em [0,1] e QPS > 0 (não UNBENCHMARKED)."
---

# Plan: Vector Recall@k Benchmark Harness

> **Version 1.0** — Implementa o harness measurement-first do M2 (ADR 0002): um pacote Python (`benchmarks/theodb_bench/`) que conecta ao container `theo-db:dev` (pgvector), gera vetores seedados, computa ground-truth brute-force exato, constrói índice HNSW/IVFFlat via SQL, e mede **recall@k (semântica SOTA distance-thresholded), latência p50/p95/p99, QPS, build-time e tamanho de índice**, emitindo um relatório reprodutível em `docs/benchmarks/`. Destrava a decisão de índice (adotar/forkar/ScaNN-AM) e o gatilho de fork D3 — converte todo número `UNBENCHMARKED` em evidência.

## Goal

> "Enable o time do TheoDB a medir recall@k + latência/QPS de um índice pgvector contra o container `theo-db:dev` de forma reprodutível, measured by `pytest benchmarks/` verde E um relatório `docs/benchmarks/*-pgvector-*.json` com recall@10 medido em [0,1] e QPS > 0 (não `UNBENCHMARKED`)."

## Context

O ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (LOCKED) e o `ROADMAP.md` M2 elevaram o harness de recall a **gate / 1º item** do pilar killer. A discovery `vector-recall-benchmark-harness` (blueprint SHIPPABLE_WITH_CAVEATS 89) fixou a metodologia FAANG: recall **distance-thresholded** (ANN-Benchmarks/Aumüller, não id-overlap), ground-truth brute-force exato, best-of-N runs, seeds. `public-copy.md` + PRD D3 proíbem claim sem benchmark reproduzível. Hoje **nenhum harness existe** (tudo `UNBENCHMARKED`). Este plano constrói o harness, medindo o que a imagem M0 oferece (pgvector HNSW/IVFFlat), extensível a pgvectorscale/ScaNN-AM depois.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `benchmarks/theodb_bench/__init__.py` (NEW) | 0 | — | (a criar — marca o pacote) | — |
| `benchmarks/theodb_bench/recall.py` (NEW) | 0 | — | (a criar — recall@k + ground-truth, lógica pura) | — |
| `benchmarks/theodb_bench/dataset.py` (NEW) | 0 | — | (a criar — geração de dataset seedado) | — |
| `benchmarks/theodb_bench/metrics.py` (NEW) | 0 | — | (a criar — percentis + QPS best-of-N) | — |
| `benchmarks/theodb_bench/db.py` (NEW) | 0 | — | (a criar — psycopg: gate, load, index, query, size) | — |
| `benchmarks/theodb_bench/harness.py` (NEW) | 0 | — | (a criar — orquestração + sweep + saída) | — |
| `benchmarks/theodb_bench/__main__.py` (NEW) | 0 | — | (a criar — CLI) | — |
| `benchmarks/tests/test_recall.py` (NEW) | 0 | — | (a criar — unit recall/ground-truth) | — |
| `benchmarks/tests/test_dataset.py` (NEW) | 0 | — | (a criar — unit dataset seedado) | — |
| `benchmarks/tests/test_metrics.py` (NEW) | 0 | — | (a criar — unit percentis/QPS) | — |
| `benchmarks/tests/test_integration.py` (NEW) | 0 | — | (a criar — integration contra container) | — |
| `benchmarks/requirements.txt` (NEW) | 0 | — | (a criar — deps pinadas) | — |
| `benchmarks/pyproject.toml` (NEW) | 0 | — | (a criar — config pytest + markers) | — |
| `docs/benchmarks/.gitkeep` (NEW) | 0 | — | (a criar — diretório de saída do gate) | — |
| `CHANGELOG.md` | 197 | `8b58153` (2026-06-27) | Contrato público de mudanças | Formato Keep a Changelog; só append em `[Unreleased]` |

Git sha base: `8b58153`. Todos os arquivos do harness são **NEW** (greenfield) — sem callers existentes.

### Current callers / dependents

- **Símbolos novos** (`recall_at_k`, `brute_force_ground_truth`, `latency_percentiles`, `qps_best_of_n`, `BenchmarkRunner`): nenhum caller hoje (pacote novo). Callers serão internos ao próprio pacote + os testes. **External (outras repos):** não — ferramenta interna.

### Domain glossary

- **recall@k (distance-thresholded)** — `|{ retornado d ≤ dist(p*_k, q) + ε }| / k`, ε=1e-3; mede dos *distances*, não ids (ANN-Benchmarks).
- **ground-truth** — os k vizinhos mais próximos exatos (brute-force), o oráculo contra o qual o índice ANN é comparado.
- **p*_k** — o k-ésimo vizinho verdadeiro mais próximo; sua distância é o limiar de recall.
- **QPS best-of-N** — queries/segundo = `1 / menor latência-média-por-query entre N runs` (reduz jitter).
- **ef_search / probes** — parâmetros de query do HNSW / IVFFlat que trocam recall por velocidade.
- **pg_relation_size** — função SQL do Postgres que dá o tamanho em bytes de um índice/relação.

### Architecture boundaries affected

Per `rules/architecture.md`: o harness é **tooling/infraestrutura** (camada externa). Ele depende para dentro só de bibliotecas (numpy) e fala com o PostgreSQL via **adapter** (`db.py` encapsula psycopg — DIP: a lógica de recall/metrics não importa psycopg). Lógica pura (`recall.py`, `metrics.py`, `dataset.py`) não tem I/O (testável em ms). Nenhuma fronteira de produção do engine é cruzada (o harness não é parte do banco; é um cliente externo).

## Prior Art & Related Work

- **Internal blueprint:** `knowledge-base/discoveries/blueprints/vector-recall-benchmark-harness-blueprint.md` — Coverage Corner 4 §T1 (recall distance-threshold), §T2 (ground-truth brute-force), §T3 (latência/QPS best-of-N); ADRs D1–D4 (stack Python, semântica de recall, seed, protocolo).
- **Reference projects:** `knowledge-base/references/pgvector/test/t/012_hnsw_vector_build_recall.pl` (oráculo exact=index-off + threshold 0.99); `knowledge-base/references/pgvectorscale/tests/test_basic_operations.py` (padrão psycopg connect→load→index→query); `knowledge-base/references/pgvectorscale/scripts/run-python-tests.sh` (gate `pg_isready`).
- **External literature:** Aumüller, Bernhardsson, Faithfull, *ANN-Benchmarks*, `arxiv.org/abs/1807.05614` §2.1 (definição de recall) + `github.com/erikbern/ann-benchmarks` `ann_benchmarks/plotting/metrics.py` (implementação distance-threshold).

## Dependencies

| Ecosystem | Package | Version | License | CVE (pip-audit 2026-06-27) | Rule-9 (reuso, não reinventar) |
|---|---|---|---|---|---|
| pip | `numpy` | >=1.26 | BSD-3-Clause | nenhuma | ground-truth BLAS + linalg — não reimplementar k-NN exato |
| pip | `psycopg2-binary` | >=2.9 | LGPL-3.0 (linking exception) | nenhuma | driver Postgres maduro — não reinventar o wire protocol |
| pip | `pytest` | >=7 | MIT | nenhuma (dev-only) | runner padrão — não reinventar harness de teste |

CVE-check: `pip-audit -r benchmarks/requirements.txt` → **"No known vulnerabilities found"** (2026-06-27); `osv-scanner` disponível p/ cross-check no CI. **Nota de licença (D1, honesta):** `psycopg2` é LGPL — porém o harness é **tooling interno de dev/CI**, NÃO faz parte da imagem distribuída `theo-db:dev` (o `Dockerfile` não o inclui), então a restrição de licença da *distribuição* (D1) não se aplica — mesma lógica do `build-essential` GPL em M0 (dev-only, fora do artefato). Se algum dia o harness for redistribuído sob Apache, troca-se por `pg8000` (BSD, puro-Python) — swap de 1 linha no adapter `db.py` (DIP isola o driver).

## Objective

- [ ] Lógica pura de **recall@k distance-thresholded** + ground-truth brute-force, com casos de empate, 100% testada
- [ ] Geração de dataset **seedada** (determinística entre runs)
- [ ] Métricas: latência p50/p95/p99 + QPS best-of-N + build-time + index size
- [ ] Camada DB (`db.py`) com gate `pg_isready`, `CREATE EXTENSION` idempotente, build de índice e query top-k
- [ ] Harness orquestra sweep de `ef_search`/`probes` → curva recall×QPS, saída JSON+markdown em `docs/benchmarks/`
- [ ] **Evidência real:** rodar contra `theo-db:dev` e produzir números medidos (não `UNBENCHMARKED`)

## ADRs

### D1 — Stack Python (numpy + psycopg2), não Rust/C

**Decision:** harness é pacote Python com `numpy` (ground-truth + métricas) e `psycopg2` (boundary SQL).
**Rationale:** é a stack do ANN-Benchmarks e do harness de teste dos análogos (`rules/parsimony-ladder.md` Regra 9 — não reinventar; reuso de dep instalada). numpy BLAS dá ground-truth exato rápido. Blueprint ADR D1.
**Alternatives considered:** estender benches `criterion` do pgvectorscale (rejeitado — in-process, sem SQL, sem recall); macros C do pgvector (rejeitado — recompile, só fases de build).
**Consequences:** deps Python sujeitas a `/deps-audit` (D1 licença).

### D2 — Recall distance-thresholded, não id-overlap

**Decision:** `recall@k = |{ retornado d ≤ dist(p*_k,q) + ε }| / k`, ε=1e-3, computado das distâncias.
**Rationale:** definição SOTA (ANN-Benchmarks paper §2.1); id-overlap diverge sob empates. Paridade de medição com a área é pré-requisito de claim "igual/superior ao ScaNN" (ADR 0002).
**Alternatives considered:** id-overlap puro (rejeitado — frágil sob empates/duplicatas; é o que os testes Perl do pgvector usam só para threshold 0.99, não para reportar curva).
**Consequences:** o harness precisa do array de **distâncias true**, não só ids.

### D3 — Dataset/queries seedados; conexão única ao medir

**Decision:** RNG seedado para geração de dataset e seleção de queries; build 1× por config; conexão psycopg única, sequencial, ao medir latência.
**Rationale:** reprodutibilidade é o ponto do gate (`public-copy.md`, `rules/testing.md` determinístico). Os análogos não seedam dataset/queries — débito que herdaríamos. Medição sequencial single-conn evita ruído de concorrência na latência.
**Alternatives considered:** múltiplas conexões concorrentes para QPS (rejeitado neste slice — introduz variância e mistura throughput de pool com latência de índice; QPS best-of-N de latência single-thread é o protocolo ANN-Benchmarks).
**Consequences:** QPS reportado é single-thread (protocolo SOTA); throughput multi-conn fica para slice futuro.

### D4 — Saída reprodutível JSON+markdown em `docs/benchmarks/`

**Decision:** cada run emite um JSON (dados crus: config, recall@k, percentis, QPS, build_ms, index_bytes, seed, sha) + um markdown legível, em `docs/benchmarks/`.
**Rationale:** é o artefato do gate M2 (ADR 0002) que `/review` e claims de performance citam; JSON p/ regressão, markdown p/ humano.
**Alternatives considered:** só stdout (rejeitado — não é artefato citável/versionável).
**Consequences:** `docs/benchmarks/` passa a versionar evidência (reprodutibilidade).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Harness mede só pgvector hoje (M0 image não tem pgvectorscale/ScaNN-AM) | Medium | `db.py` é index-agnóstico (recebe DDL do índice); pgvectorscale entra quando a imagem o tiver — sem reescrever o harness | dev |
| Ground-truth brute-force é O(N·Q·d) — caro em datasets grandes | Medium | Cachear ground-truth em disco com seed; começar com N pequeno (synthetic seeded) e sift-128 subset | dev |
| Números de latência sensíveis a ruído da máquina (CI compartilhado) | Medium | best-of-N runs (min) + reportar mean±std + documentar hardware no relatório | dev |
| Novo runner Python no repo (antes só Docker) | Low | Isolado em `benchmarks/`; não afeta a imagem nem o smoke do M0 | dev |

## Unresolved Questions

- Q1 — `--runs` default do ANN-Benchmarks é 5? (blueprint flag `UNVERIFIED-DEFAULT`) → o harness usa default **3** explícito e documentado; não dependemos do número do ANN-Benchmarks.
- Q2 — Dataset padrão (sift-128) será espelhado no repo ou gerado synthetic? → este slice usa **synthetic seedado** (sem dep de host externo, supply-chain limpo); HDF5 real fica para slice futuro (registrado).
- Q3 — Index size via `pg_relation_size` inclui toast/fsm? → usar `pg_relation_size(indexrelid)` (heap do índice); documentar a definição exata no relatório.

## Dependency Graph

```
Phase 0 (scaffold) ──▶ Phase 1 (recall+gt) ──▶ Phase 5 (harness) ──▶ Phase 6 (Integration Validation)
        │                    ▲   ▲   ▲                 ▲
        ├──▶ Phase 2 (dataset)┘   │   │                 │
        ├──▶ Phase 3 (metrics)────┘   │                 │
        └──▶ Phase 4 (db adapter)─────┘─────────────────┘
```
Phases 1–4 dependem só de Phase 0 e podem paralelizar; Phase 5 depende de 1–4; Phase 6 depende de 5.

---

## Phase 0: Scaffolding + runner config

**Objective:** criar o esqueleto do pacote e a config de teste para que pytest descubra os testes.

### T0.1 — Criar pacote + config pytest + deps

#### Objective
Estrutura `benchmarks/theodb_bench/` + `benchmarks/pyproject.toml` (pytest, markers `integration`) + `benchmarks/requirements.txt` + `docs/benchmarks/.gitkeep`.

#### Why this step (action + reasoning)
1. **What:** criar pacote vazio, config de runner e deps pinadas.
2. **Why now:** sem runner configurado os testes RED das fases seguintes não são descobertos (Pre-flight path validation do `/to-plan`). Marker `integration` permite rodar unit sem container.

#### Evidence
Recon: `find` não achou pyproject/pytest no repo (sem runner Python ainda). Blueprint §Dependencies (deps numpy/psycopg2/pytest).

#### Files to edit
```
benchmarks/theodb_bench/__init__.py — pacote vazio
benchmarks/pyproject.toml — [tool.pytest.ini_options] testpaths, markers=integration
benchmarks/requirements.txt — numpy, psycopg2-binary, pytest
docs/benchmarks/.gitkeep — diretório de saída
```

#### Deep file dependency analysis
Todos NEW (Baseline Context). Nenhum downstream depende ainda.

#### Tasks
1. Criar `benchmarks/theodb_bench/__init__.py`.
2. Criar `benchmarks/pyproject.toml` com `testpaths=["tests"]`, `markers=["integration: requires running theo-db:dev container"]`.
3. Criar `benchmarks/requirements.txt` (numpy>=1.26, psycopg2-binary>=2.9, pytest>=7).
4. Criar `docs/benchmarks/.gitkeep`.

#### TDD
```
RED:     test_package_importable() — `import theodb_bench` não levanta (falha antes do __init__)
GREEN:   criar __init__.py + config
REFACTOR: None expected
VERIFY:  cd benchmarks && python -m pytest tests/test_smoke.py -q
```

#### Concurrency tests (only when applicable)
(none — single-threaded)

#### Acceptance Criteria
- [ ] `cd benchmarks && python -m pytest -q` coleta testes (0 erros de coleta)
- [ ] `pip install -r benchmarks/requirements.txt` resolve
- [ ] Pass: `wc -l benchmarks/theodb_bench/*.py` reporta cada arquivo ≤ 500 linhas

#### DoD
- [ ] `import theodb_bench` funciona no venv
- [ ] pytest descobre `benchmarks/tests/`

---

## Phase 1: Recall@k + ground-truth (lógica pura — o núcleo SOTA)

**Objective:** implementar a matemática de recall distance-thresholded e o ground-truth brute-force, 100% testada, sem I/O.

### T1.1 — `recall.py`: ground-truth brute-force + recall@k distance-threshold

#### Objective
Funções puras `brute_force_ground_truth(corpus, queries, k, metric) -> (indices, distances)` e `recall_at_k(true_distances, run_distances, k, eps=1e-3) -> float`.

#### Why this step (action + reasoning)
1. **What:** implementar o oráculo exato (numpy BLAS) e a métrica de recall com semântica de limiar de distância.
2. **Why now:** é o núcleo de correção do harness (ADR D2). Tudo depende dele; testá-lo isolado (ms, sem DB) é a base da pirâmide (`rules/testing.md`). Empates são o caso onde id-overlap erra — precisa de teste dedicado.

#### Evidence
Blueprint §T1 (fórmula `|{d ≤ dist(p*_k,q)+ε}|/k`, impl `metrics.py` do ANN-Benchmarks); §T2 (BLAS: angular normaliza+`-dot`, euclidean `lengths-2·dot`). `arxiv.org/abs/1807.05614` §2.1.

#### Deep file dependency analysis
`recall.py` NEW. Importado por `harness.py` (Phase 5) e pelos testes. Sem psycopg (lógica pura — DIP).

#### Deep Dives
- `brute_force_ground_truth`: para metric `l2` → `dists[i,j] = ||corpus[j]||² - 2·corpus·query[i]` (sort exato via argpartition+argsort); `cosine` → normaliza L2, `dists = 1 - corpus_n·query_n`. Retorna `(top_k_indices (Q,k), top_k_distances (Q,k))`.
- `recall_at_k`: para cada query, `threshold = true_distances[i, k-1] + eps`; conta `run_distances[i, :k] <= threshold`; recall = mean(counts)/k. **Invariante:** resultado ∈ [0, 1].
- **Edge/negative cases:** k > N (erro tipado `ValueError`); corpus vazio (`ValueError`); empates exatos (distância igual ao k-ésimo conta — testa que recall=1.0 quando o índice retorna um empate equivalente); run com menos de k resultados (conta só os presentes).

#### Pseudo-code / Signatures
```pseudocode
def recall_at_k(true_distances, run_distances, k, eps=1e-3) -> float:
  # precondition: true_distances.shape[1] >= k
  recalls = []
  for i in range(len(run_distances)):
    threshold = true_distances[i][k-1] + eps
    hit = sum(1 for d in run_distances[i][:k] if d <= threshold)
    recalls.append(hit / k)
  return mean(recalls)

# Example (k=2, empate no 2º): true_d=[[0.1, 0.2]], run_d=[[0.1, 0.2]] -> recall=1.0
# Example (miss): true_d=[[0.1,0.2]], run_d=[[0.1, 0.9]] -> recall=0.5
```

#### Tasks
1. Implementar `brute_force_ground_truth` (l2 + cosine).
2. Implementar `recall_at_k` (distance-threshold).
3. Validação de entrada: k>N → `ValueError`, corpus vazio → `ValueError`.

#### TDD
```
RED:     test_recall_perfect_when_run_equals_truth() — recall=1.0 quando run==truth
RED:     test_recall_half_when_one_of_two_missed() — run_d=[[0.1,0.9]] vs true=[[0.1,0.2]] → 0.5
RED:     test_recall_tie_at_kth_counts_equivalent() — empate na k-ésima distância conta (recall=1.0)
RED:     test_recall_in_unit_interval() — recall sempre ∈ [0,1] (property, vários inputs)
RED:     test_ground_truth_l2_matches_known_neighbors() — corpus pequeno, vizinhos conhecidos
RED:     test_ground_truth_cosine_matches_known() — idem cosine
RED:     test_k_greater_than_n_raises_valueerror() — negative case (erro tipado)
RED:     test_empty_corpus_raises_valueerror() — negative case
GREEN:   Implementar recall.py
REFACTOR: extrair _normalize() se l2/cosine duplicarem
VERIFY:  cd benchmarks && python -m pytest tests/test_recall.py -q
```

#### Concurrency tests (only when applicable)
(none — single-threaded)

#### Acceptance Criteria
- [ ] Todos os testes de `test_recall.py` verdes
- [ ] recall_at_k retorna ∈ [0,1] em todos os casos
- [ ] Pass: `cd benchmarks && python -m pytest tests/test_recall.py -k raises -q` exits 0 — negative cases levantam `ValueError` tipado (não retorno mágico)
- [ ] Pass: coverage — `recall.py` 100% (critical path)
- [ ] Pass: size — `recall.py` ≤ 500 linhas

#### DoD
- [ ] `pytest tests/test_recall.py` verde
- [ ] Zero lint (`ruff check benchmarks/theodb_bench/recall.py`)

---

## Phase 2: Dataset seedado

**Objective:** geração determinística de corpus + queries.

### T2.1 — `dataset.py`: `make_dataset(n, dim, n_queries, seed, metric)`

#### Objective
Gerar corpus `(n,dim)` e queries `(n_queries,dim)` determinísticos via `numpy.random.default_rng(seed)`.

#### Why this step (action + reasoning)
1. **What:** dataset reprodutível seedado.
2. **Why now:** reprodutibilidade (ADR D3) — os análogos não seedam dataset (débito). É insumo independente; paraleliza com Phase 1/3/4.

#### Evidence
Blueprint §Tools (split seeded `random_state=1`; "harness deve seedar explicitamente").

#### Deep file dependency analysis
`dataset.py` NEW. Importado por `harness.py` + testes. Pura (sem I/O).

#### Deep Dives
- `make_dataset(n, dim, n_queries, seed, metric)` → `(corpus, queries)`. RNG `default_rng(seed)`. Para `cosine`, opcionalmente normaliza. **Invariante:** mesmo seed → arrays idênticos (bit-a-bit).
- Edge cases: n=0 → `ValueError`; dim=0 → `ValueError`.

#### Tasks
1. Implementar `make_dataset` com `default_rng(seed)`.
2. Validar n>0, dim>0.

#### TDD
```
RED:     test_dataset_deterministic_for_same_seed() — duas chamadas com seed=42 → np.array_equal
RED:     test_dataset_differs_for_different_seed() — seed=1 != seed=2
RED:     test_dataset_shapes() — corpus (n,dim), queries (n_queries,dim)
RED:     test_dataset_zero_n_raises() — negative case
GREEN:   Implementar dataset.py
REFACTOR: None expected
VERIFY:  cd benchmarks && python -m pytest tests/test_dataset.py -q
```

#### Concurrency tests (only when applicable)
(none — single-threaded)

#### Acceptance Criteria
- [ ] Pass: `cd benchmarks && python -m pytest tests/test_dataset.py -k deterministic -q` exits 0 — determinismo por seed provado
- [ ] Pass: `wc -l benchmarks/theodb_bench/*.py` reporta cada arquivo ≤ 500 linhas; `pytest --cov=theodb_bench` ≥ 90% (críticos 100%)

#### DoD
- [ ] `pytest tests/test_dataset.py` verde

---

## Phase 3: Métricas (percentis + QPS best-of-N)

**Objective:** estatística de latência rigorosa.

### T3.1 — `metrics.py`: `latency_percentiles(samples)` + `qps_best_of_n(run_means)`

#### Objective
`latency_percentiles(samples_ms) -> {p50,p95,p99,mean,std}` e `qps_best_of_n(run_mean_latencies_s) -> float` (= 1/min).

#### Why this step (action + reasoning)
1. **What:** percentis + QPS best-of-N puros.
2. **Why now:** protocolo SOTA (ADR D3 / blueprint §T3). Independente; paraleliza.

#### Evidence
Blueprint §T3 (`QPS = 1/best_search_time`, best-of-N=min, percentis p50/95/99).

#### Deep file dependency analysis
`metrics.py` NEW. Importado por `harness.py` + testes. Pura.

#### Deep Dives
- `latency_percentiles`: `numpy.percentile([50,95,99])` + mean/std. **Invariante:** p50 ≤ p95 ≤ p99.
- `qps_best_of_n`: `1.0 / min(run_means)`; run_means vazio → `ValueError`; min ≤ 0 → `ValueError` (latência inválida).

#### Pseudo-code / Signatures
```pseudocode
def qps_best_of_n(run_mean_latencies_s) -> float:
  if not run_mean_latencies_s: raise ValueError
  best = min(run_mean_latencies_s)
  if best <= 0: raise ValueError
  return 1.0 / best
# Example: [0.01, 0.008, 0.012] -> 1/0.008 = 125.0 QPS
```

#### Tasks
1. Implementar `latency_percentiles`.
2. Implementar `qps_best_of_n`.

#### TDD
```
RED:     test_percentiles_ordered() — p50<=p95<=p99
RED:     test_percentiles_known_values() — input conhecido → percentis esperados
RED:     test_qps_best_of_n_uses_min() — [0.01,0.008,0.012] → 125.0
RED:     test_qps_empty_raises() — negative case
RED:     test_qps_nonpositive_latency_raises() — negative case
GREEN:   Implementar metrics.py
REFACTOR: None expected
VERIFY:  cd benchmarks && python -m pytest tests/test_metrics.py -q
```

#### Concurrency tests (only when applicable)
(none — single-threaded)

#### Acceptance Criteria
- [ ] `test_metrics.py` verde; invariante p50≤p95≤p99
- [ ] Pass: `wc -l benchmarks/theodb_bench/*.py` reporta cada arquivo ≤ 500 linhas; `pytest --cov=theodb_bench` ≥ 90% (críticos 100%)

#### DoD
- [ ] `pytest tests/test_metrics.py` verde

---

## Phase 4: Adapter DB (psycopg)

**Objective:** encapsular toda interação com o PostgreSQL (gate, extension, load, index, query, size).

### T4.1 — `db.py`: conexão, gate `pg_isready`, load, build index, query top-k, index size

#### Objective
Classe/adapter `VectorDB` com `ping()`, `ensure_extension()`, `load_vectors()`, `build_index(ddl)`, `query_topk(qvec, k) -> (ids, distances, latency_s)`, `index_size_bytes(name)`, `assert_index_used(qvec)`.

#### Why this step (action + reasoning)
1. **What:** isolar o I/O do PostgreSQL atrás de uma interface (DIP) — a lógica de recall/metrics não conhece psycopg.
2. **Why now:** é a fronteira externa (I/O não-determinístico → Failure scenarios). Encapsular permite testar a lógica pura sem DB e exercer o boundary no integration.

#### Evidence
Blueprint §Integration (padrão psycopg connect→extension→table→insert→`CREATE INDEX`→`ORDER BY <=> LIMIT k`; oráculo exact = `SET enable_indexscan=off`; `EXPLAIN` assegura Index Scan). `knowledge-base/references/pgvectorscale/tests/test_basic_operations.py`; `knowledge-base/references/pgvectorscale/scripts/run-python-tests.sh` (pg_isready gate).

#### Deep file dependency analysis
`db.py` NEW. Importa psycopg2. Importado por `harness.py` + `test_integration.py`. **É a única porta de I/O** (architecture boundary).

#### Deep Dives
- `ping()` → executa `SELECT 1` com timeout; falha → `DBUnavailableError` (tipado).
- `query_topk`: `SELECT id, embedding <=> %s::vector AS distance FROM tbl ORDER BY embedding <=> %s::vector LIMIT %s`; cronometra com `time.perf_counter` em volta do `execute+fetchall`.
- `exact_topk` (ground-truth in-DB, opcional): `SET LOCAL enable_indexscan=off` antes da mesma query.
- `assert_index_used`: roda `EXPLAIN (FORMAT JSON)` e verifica que o plano contém `Index Scan`/`Index Only Scan` no índice — guard contra medir exact por engano (blueprint §Corner1, `012...pl` EXPLAIN guard).
- `index_size_bytes`: `SELECT pg_relation_size(%s::regclass)`.
- **Invariantes:** `query_topk` retorna ≤ k linhas; latência > 0.
- Edge/negative: DB down → `DBUnavailableError`; índice não usado → `IndexNotUsedError`.

#### Tasks
1. `VectorDB.__init__(dsn)` + `ping()` com erro tipado.
2. `ensure_extension()` idempotente (`CREATE EXTENSION IF NOT EXISTS vector`).
3. `load_vectors(ids, vectors)` via `execute_values`.
4. `build_index(ddl)` + retorna build-time (perf_counter).
5. `query_topk` + `exact_topk` + `assert_index_used` + `index_size_bytes`.

#### TDD
```
RED:     test_ping_raises_dbunavailable_on_bad_dsn() — DSN inválido → DBUnavailableError (unit, sem container)
RED:     test_query_topk_sql_shape() — monta a SQL esperada (unit com fake cursor)
GREEN:   Implementar db.py
REFACTOR: extrair _exec helper
VERIFY:  cd benchmarks && python -m pytest tests/test_db.py -q
```
(Os caminhos felizes de `db.py` que exigem PostgreSQL real são exercidos no `test_integration.py` — Phase 6, marker `integration`.)

#### Concurrency tests (only when applicable)
(none — single-threaded)
Conexão única, sequencial (ADR D3). Sem locks/async/threads.

#### Acceptance Criteria
- [ ] `test_db.py` (unit) verde — erros tipados, SQL shape correta
- [ ] Pass: `cd benchmarks && python -m pytest tests/test_db.py -k raises -q` exits 0 — erros **tipados** (`DBUnavailableError`/`IndexNotUsedError`), nunca retorno mágico (Regra 8)
- [ ] Pass: `wc -l benchmarks/theodb_bench/db.py` ≤ 500 linhas

#### DoD
- [ ] `pytest tests/test_db.py` verde
- [ ] Pass: `grep -rl 'import psycopg2' benchmarks/theodb_bench/ | grep -v db.py` retorna vazio (DIP — psycopg só em `db.py`)

---

## Phase 5: Harness (orquestração + sweep + saída)

**Objective:** juntar tudo num runner que varre parâmetros e emite o relatório.

### T5.1 — `harness.py` + `__main__.py`: BenchmarkRunner + CLI + saída JSON/markdown

#### Objective
`BenchmarkRunner.run(config)` que: gera dataset (seed) → ground-truth → para cada índice/param: build (tempo) → para cada query medir latência + topk → recall@k → agrega percentis/QPS/size → escreve `docs/benchmarks/{date}-pgvector-{index}.json` + `.md`. CLI `python -m theodb_bench`.

#### Why this step (action + reasoning)
1. **What:** orquestrar dataset+gt+db+recall+metrics num run reprodutível com saída.
2. **Why now:** é o wiring (caller que exercita tudo end-to-end) — sem ele as peças não viram o artefato do gate (ADR D4). Depende de 1–4.

#### Evidence
Blueprint §Recommendations 1–6; ADR D4 (saída JSON+md em docs/benchmarks/).

#### Deep file dependency analysis
`harness.py`/`__main__.py` NEW. Importa recall, dataset, metrics, db. É o **caller** (wiring triad pilar a). `test_integration.py` o exercita.

#### Deep Dives
- `config`: seed, n, dim, n_queries, k, metric, index_specs (lista de DDL + param sweep ef_search/probes), runs (default 3).
- Loop: build 1× por índice/param; `runs` repetições de todas as queries (best-of-N por query-mean); recall@k computado contra ground-truth (1×).
- Saída JSON: `{sha, seed, n, dim, k, metric, results:[{index, params, recall_at_k, p50,p95,p99, qps, build_ms, index_bytes}]}`. Markdown: tabela legível + curva recall×QPS textual.
- **Runtime-metric proof:** o JSON emitido com `recall_at_k`∈[0,1] e `qps>0` É a prova de wiring (não basta compilar).

#### Pseudo-code / Signatures
```pseudocode
def run(config) -> Report:
  corpus, queries = make_dataset(config.seed, ...)
  gt_idx, gt_dist = brute_force_ground_truth(corpus, queries, config.k, config.metric)
  db.ensure_extension(); db.load_vectors(corpus)
  results = []
  for spec in config.index_specs:
    build_ms = db.build_index(spec.ddl)
    for params in spec.sweep:
      db.set_query_params(params)
      run_means = []
      for _ in range(config.runs):
        lat, run_dists = [], []
        for q in queries:
          ids, dists, t = db.query_topk(q, config.k); db.assert_index_used(q)
          lat.append(t); run_dists.append(dists)
        run_means.append(mean(lat))
      recall = recall_at_k(gt_dist, run_dists, config.k)
      results.append({recall, qps_best_of_n(run_means), latency_percentiles(lat*1000),
                      build_ms, db.index_size_bytes(spec.name)})
  write_json_and_md(results, config)
```

#### Tasks
1. Implementar `BenchmarkRunner.run`.
2. Implementar writers JSON + markdown em `docs/benchmarks/`.
3. CLI `__main__.py` (args: --seed --n --dim --k --metric --runs --dsn).

#### TDD
```
RED:     test_runner_with_fake_db_emits_report() — injeta um FakeVectorDB (sem container); assert JSON tem recall∈[0,1], qps>0, e arquivo escrito
RED:     test_report_json_schema() — chaves obrigatórias presentes
RED:     test_cli_parses_args() — __main__ parseia --seed/--k
GREEN:   Implementar harness.py + __main__.py
REFACTOR: extrair writers se harness.py > 300 linhas
VERIFY:  cd benchmarks && python -m pytest tests/test_harness.py -q
```

#### Concurrency tests (only when applicable)
(none — single-threaded)

#### Acceptance Criteria
- [ ] `test_harness.py` verde (com FakeVectorDB injetado — DIP)
- [ ] JSON emitido valida o schema (recall∈[0,1], qps>0)
- [ ] Pass: size — `harness.py` ≤ 500
- [ ] DIP: `harness.run` aceita um db injetado (testável sem container)

#### DoD
- [ ] `pytest tests/test_harness.py` verde

---

## Coverage Matrix

| # | Gap / Requirement (blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | recall@k distance-thresholded (não id-overlap) | T1.1 | `recall_at_k` por limiar de distância + testes de empate |
| 2 | ground-truth brute-force exato | T1.1 | `brute_force_ground_truth` (l2/cosine) + testes vizinhos conhecidos |
| 3 | dataset seedado reprodutível | T2.1 | `make_dataset(seed)` + teste determinismo |
| 4 | latência p50/p95/p99 + QPS best-of-N | T3.1 | `latency_percentiles` + `qps_best_of_n` |
| 5 | build-time + index size | T4.1, T5.1 | `build_index` (perf_counter) + `index_size_bytes` |
| 6 | rodar dentro do PG real (gate, extension, query) | T4.1 | `VectorDB` adapter + gate `pg_isready`/ping |
| 7 | sweep ef_search/probes → curva recall×QPS | T5.1 | loop de params no `BenchmarkRunner` |
| 8 | saída reprodutível JSON+md em docs/benchmarks/ | T5.1 | writers |
| 9 | EVIDÊNCIA REAL contra theo-db:dev | T6.1 | Integration Validation run |
| 10 | pirâmide de teste (unit puro + integration) | T1–T5 (unit) + T6.1 (integration) | markers pytest |
| 11 | erros tipados (DB down, índice não usado, dataset vazio) | T1.1, T4.1 | `ValueError`/`DBUnavailableError`/`IndexNotUsedError` |

**Coverage: 11/11 gaps cobertos (100%)**

## Global Definition of Done

- [ ] Todas as fases completas
- [ ] Todos os testes verdes — `cd benchmarks && python -m pytest` (unit) + `pytest -m integration` (com container)
- [ ] Zero type errors — N/A (sem mypy obrigatório; opcional `mypy theodb_bench`)
- [ ] Zero lint warnings — `ruff check benchmarks/`
- [ ] File-size budget respeitado (cada arquivo ≤ 500 linhas)
- [ ] CHANGELOG.md atualizado sob `[Unreleased]`
- [ ] Backward compat — N/A (pacote novo)
- [ ] Plan-specific: **relatório real** `docs/benchmarks/*-pgvector-*.json` com `recall_at_k`∈[0,1] e `qps>0` (não UNBENCHMARKED)
- [ ] **Runtime-metric proof** — o JSON do harness foi gerado por um run real contra `theo-db:dev` (não só testes verdes)
- [ ] Plan archived após READY_TO_MERGE + merge

## Failure scenarios (when I/O external)

O harness toca I/O externo: **PostgreSQL via psycopg2** (`db.py`).

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `postgres:theo-db:dev` (DB) | container down / DSN inalcançável | `test_ping_raises_dbunavailable_on_bad_dsn()` com DSN para porta morta | `ping()` levanta `DBUnavailableError` com mensagem clara (host:port); harness aborta cedo, não produz relatório falso |
| `postgres` (planner) | índice criado mas planner faz seqscan (índice "não usado") | integration: forçar `enable_indexscan=off` e chamar `assert_index_used` | `IndexNotUsedError` — não medir recall de exact por engano (guard do blueprint §Corner1) |
| dataset | corpus vazio / k > N | `test_empty_corpus_raises_valueerror()` / `test_k_greater_than_n_raises()` | `ValueError` tipado antes de tocar o DB (fail-fast na fronteira) |

## Final Phase: Integration Validation (MANDATORY)

### T6.1 — Run real contra `theo-db:dev` + suíte completa

**Objective:** provar 100% funcional com evidência — rodar o harness contra o container e produzir números medidos.

#### Concurrency tests

(none — single-threaded) — o harness mede latência com conexão psycopg única e sequencial (ADR D3); sem locks/async/threads.

#### Execution
```
# subir container (idempotente)
docker run -d -e POSTGRES_PASSWORD=postgres -p 5432:5432 --name theo-db-bench theo-db:dev || docker start theo-db-bench
bash smoke.sh   # confirma pgvector vivo

# unit
cd benchmarks && python -m pytest -q -m "not integration"
# integration (contra container)
PGHOST=localhost PGUSER=postgres PGPASSWORD=postgres python -m pytest -q -m integration
# coverage
python -m pytest --cov=theodb_bench --cov-report=term-missing
# lint
ruff check benchmarks/
# RUN REAL (a evidência do gate)
PGHOST=localhost PGUSER=postgres PGPASSWORD=postgres python -m theodb_bench --seed 42 --n 5000 --dim 128 --k 10 --metric l2 --runs 3
```

#### Acceptance Criteria
- [ ] Unit + integration verdes (`cd benchmarks && python -m pytest -q` e `python -m pytest -q -m integration`)
- [ ] Coverage ≥ 90% nos arquivos do harness (críticos `recall.py`/`metrics.py`: 100%)
- [ ] Zero lint warnings (`ruff`)
- [ ] **Runtime-metric proof** — `docs/benchmarks/{date}-pgvector-hnsw.json` existe, `recall_at_k`∈[0,1], `qps>0`, `build_ms>0`, `index_bytes>0`
- [ ] Failure scenarios exercidos: `DBUnavailableError`, `IndexNotUsedError`, `ValueError` (dataset) observados nos testes
- [ ] `test_integration.py` confirma recall do índice HNSW ≥ 0.90 num dataset seedado (sanity — índice ANN tem recall alto vs exact)

### If Validation Fails
1. Separar falhas do plano vs pré-existentes.
2. Corrigir todas as do plano antes de declarar completo.
3. Re-rodar a cadeia.
