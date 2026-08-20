# Reconstrução de `benchmarks/` — especificação

**Data:** 2026-08-12 · **Alvo:** `theo-db` · **Estado:** `benchmarks/` removida do working tree (268 arquivos, 2,7 MB), não commitada.

---

## 0. Leia isto antes de escrever qualquer linha

O conteúdo **não foi perdido**. Ele está íntegro em `bcf7819` e volta com um comando:

```bash
git restore -- benchmarks          # recupera os 268 arquivos, ~1s, zero risco
git show HEAD:benchmarks/<arquivo> # inspecionar um arquivo isolado sem restaurar tudo
```

Esta especificação existe para o caso de o time decidir **reescrever** em vez de restaurar. Registro a recomendação técnica, porque ela é relevante para quem for executar: **restaurar e depois medir o delta é quase sempre o caminho certo.** O que havia ali não era sucata — era 2094 LOC de harness com literatura citada corretamente, 65 arquivos de teste e invariantes de integridade de medição que custaram uma dezena de marcos para serem descobertos. Reescrever do zero tende a produzir algo *mais bonito e menos rigoroso*, e o modo de falha é silencioso: números plausíveis e errados.

Se a decisão for reescrever, **a Seção 4 é a parte não-negociável**. Perder um item de lá reintroduz um defeito de medição que o projeto já pagou para descobrir.

### Urgente, independente da decisão: o CI está quebrado agora

`.github/workflows/ci.yml` referencia o diretório apagado em quatro pontos. Na próxima promoção para `develop` esses jobs falham:

| Linha | Comando |
|---|---|
| 178 | `ruff check theodb_bench tests` |
| 180 | `vulture theodb_bench --min-confidence 80` |
| 275 | `path: benchmarks/.datasets/glove-25-angular.hdf5` (cache) |
| 296 | `python -m theodb_bench --hdf5 ... --index hnsw ...` |

Os jobs afetados são `harness-unit` (`working-directory: benchmarks`) e `image-and-bench`.

---

## 1. Por que este diretório existe

O `CLAUDE.md` define o mandato: *"measurement-first: o harness de recall@k reproduzível é pré-requisito de qualquer claim de performance"* e *"Nenhuma afirmação de performance sem artefato em `wiki/benchmarks/`"*.

Ou seja: **este diretório é o que dá ao projeto o direito de afirmar qualquer coisa sobre performance.** Sem ele, todo número publicado em `wiki/benchmarks/` (164 arquivos, que sobreviveram) vira alegação não-reproduzível.

Separação de responsabilidade, que deve ser preservada:

| Onde | O que é |
|---|---|
| `benchmarks/` | o **método** — código que mede |
| `benchmarks/artifacts/` | o **dado bruto** — JSON/CSV/flamegraph que os runners escrevem |
| `wiki/benchmarks/` | o **veredito** — conceito OKF que interpreta o número e declara os limites |

O artefato bruto sozinho não é veredito. O veredito sem artefato não é medição.

---

## 2. Inventário do que existia

268 arquivos. Distribuição:

| Caminho | Arqs | Papel |
|---|---|---|
| `theodb_bench/` | 26 | **o pacote núcleo** (17 módulos + 9 testes), 2094 LOC sem testes |
| `tests/` | 56 | suíte pytest (unit + `-m integration`) |
| *(raiz)* | 130 | runners por marco (`run_m32_sift1m.py` … `m177_*.py`) e harnesses SQL |
| `artifacts/` | 22 | dado bruto commitado (m175, m177, m184, m186) + README |
| `m107_graph_spike/` | 15 | crate Rust de spike (inclui `target/` — **não recriar o `target/`**) |
| `archive/` | 7 | runners de marcos encerrados, arquivados e não apagados |
| `servers/` | 3 | servidores de modelo local (`embedding`, `chat`, `rerank`) |
| `htap/` | 3 | CH-benCHmark via BenchBase (config XML + queries) |
| `clickbench/theodb/` | 3 | entrada ClickBench (`benchmark.sh`, `template.json`, README) |
| `micro/` | 2 | micro-benches Rust avulsos |
| `oltp/` | 1 | HammerDB TPROC-C (`.tcl`) |

Reconstruir os 130 runners de raiz **não é necessário nem desejável** — cada um é o repro de um marco já concluído. O que precisa voltar a existir é o **pacote núcleo + a suíte de testes + a integração de CI**. Runners individuais podem ser recuperados sob demanda com `git show`.

---

## 3. O pacote núcleo `theodb_bench/`

Arquitetura: **uma única fronteira de I/O** (`db.py`), com toda a lógica pura testável sem container (DIP). Isto é o que torna `harness-unit` um job de CI barato e sem Docker.

```
__main__.py ──> harness.py ──> dataset.py ──> recall.py ──> metrics.py
                    │                                          (puro, sem I/O)
                    └──> db.py  ← ÚNICA fronteira de I/O (psycopg2)
```

### 3.1 Módulos de medição (o mínimo indispensável)

| Módulo | LOC | Contrato público | Responsabilidade |
|---|---|---|---|
| `recall.py` | 138 | `brute_force_ground_truth(corpus, queries, k, metric)` → `(ids, dists)`; `neighbors_ground_truth(train, queries, neighbor_ids, k, metric)` → `gt_dist`; `recall_at_k(true_distances, run_distances, k, eps=1e-3)` → `float` | Ground-truth exato e recall@k **por distância** |
| `metrics.py` | 67 | `latency_percentiles(samples_ms)` → `{p50,p95,p99,mean,std}`; `qps_best_of_n(run_means_s)`; `ndcg_at_k(ranked, qrels, k)`; `recall_at_n(ranked, qrels, n)` | Percentis, QPS, métricas de IR |
| `dataset.py` | 109 | `make_dataset(n, dim, n_queries, seed)`; `load_hdf5_subsample(path, n, n_queries, seed)`; `load_hdf5_full(path, n_queries, seed, k, metric)` | Corpus determinístico (sintético e ANN-Benchmarks HDF5) |
| `db.py` | 276 | classe `VectorDB` + `DBUnavailableError` + `IndexNotUsedError` | Única fronteira PostgreSQL |
| `harness.py` | 176 | `run_benchmark(config, db, out_dir)` → `report`; `artifact_stem(report)` | Orquestração + emissão do relatório JSON/MD |
| `significance.py` | 93 | `paired_significance(a, b, seed, n_resamples)` → dict | **Significância estatística pareada** |
| `regression.py` | 42 | `assert_byte_identical(baseline, candidate)`; `QidMismatchError` | Regressão de ranking byte-idêntica |

### 3.2 Módulos de domínio (por pilar)

| Módulo | LOC | Pilar | Contrato |
|---|---|---|---|
| `ann_adapter.py` | 76 | vetorial | `TheoDBAnn` — compatível com o `BaseANN` do ann-benchmarks (`fit`/`query`/`set_query_arguments`) |
| `beir.py` | 172 | lexical | `Dataset`, `synthetic_dataset()`, `load_beir_dataset(name, cache_dir, split)`, `lexical_embed(text, dim)` |
| `knownitem.py` | 63 | lexical | `mrr_at_k`, `success_at_1`, `recall_known_item`, `make_known_item_query` |
| `lexical_engines.py` | 198 | lexical | protocolo `Retriever` + `TantivyBM25`, `PgTsRank`, `PgTextsearchBM25` |
| `hybrid.py` | 86 | híbrido | `rrf_fuse(leg_rankings, k=60, weights)` — gêmeo offline de `ai.hybrid_search_rrf` |
| `columnar.py` | 76 | colunar | `run_columnar_vs_row(db, n, table)` — colunar vs row-store |
| `logcorpus.py` | 101 | lexical | `load_logcorpus(...)` — corpus LogHub como proxy de trace |
| `openai_embed.py` | 169 | embeddings | `CachedOpenAIEmbedder` — `warm(texts)` + `as_embed_fn()` |

> **Nota sobre `columnar.py`:** ele media contra `pg_mooncake`/`pg_duckdb`, **removidos do projeto no M143** (ADR-0056/0057, colunar é 100% own-code desde então). Este módulo precisa ser **reescrito contra `theodb_columnar`**, não restaurado como estava. É o único do conjunto onde "reconstruir" bate "restaurar".

### 3.3 Interface `VectorDB` (a fronteira a reimplementar)

```python
# conexão
connect() -> VectorDB ; ping() -> None ; close() -> None
# schema + carga
ensure_extension()            # CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE
create_table(table, dim, embed_col="embedding")
load_vectors(table, vectors, embed_col="embedding")
# índice + consulta
build_index(ddl) -> float                     # segundos
set_session(statement) -> None
query_topk(table, qvec, k, metric) -> (ids, dists, latency_s)
assert_index_used(table, qvec, k, metric)     # levanta IndexNotUsedError
index_size_bytes(index_name) -> int
# superfície híbrida/lexical
create_documents_table(table, dim) ; load_documents(table, docs)
vector_query_docs(table, qvec, n) ; fts_query(table, query_text, n)
hybrid_rrf_docs(table, query_text, qvec, k, n)
bm25_query(table, query_text, n, text_col) ; create_bm25_index(...) ; ensure_bm25_extension()
pg_textsearch_available() -> bool
# genéricos
explain_plan(sql) -> str ; timed_query(sql) -> (rows, ms)
```

Erros são **tipados e altos** (Regra 8): `DBUnavailableError` para falha de fronteira, `IndexNotUsedError` quando o planner escolheu seqscan, `ValueError` para métrica desconhecida. Nenhum valor mágico de retorno.

---

## 4. Invariantes de integridade — **não-negociáveis**

Cada item abaixo foi descoberto por um defeito real de medição. Reimplementar sem eles produz números que parecem certos e não são.

### 4.1 Recall

1. **Recall é limiar de distância, não sobreposição de ids.** Segue ANN-Benchmarks (Aumüller et al., arXiv:1807.05614 §2.1): `recall = |{retornado com dist ≤ dist(k-ésimo verdadeiro) + eps}| / k`. Sobreposição de ids diverge do padrão sob distâncias empatadas/duplicadas. `eps = 1e-3`.
2. **O oráculo enxerga float32.** `pgvector`/`theodb` armazenam `vector` como float4. O ground-truth arredonda para float32 **e então** calcula a distância em float64. Sem isso, quase-empates divergem entre oráculo e SUT.
3. **GT em escala ≥1M vem de `neighbors`, recomputado.** Usa os ids pré-computados do HDF5 e recalcula a distância a partir dos **vetores** — nunca confia no array `distances` do arquivo. 10⁶ operações em vez de 10¹⁰.
4. **Ids de vizinho fora de faixa falham alto.** NumPy silenciosamente daria a volta num id inválido e produziria GT errado.

### 4.2 Execução da medição

5. **O índice é forçado e verificado.** `SET enable_seqscan = off` **mais** `assert_index_used()` lendo o `EXPLAIN`. Forçar sem verificar não prova nada.
6. **Isolamento entre specs.** Antes de medir um spec, **derrubar os índices de todos os outros** — senão o planner escolhe arbitrariamente entre dois índices da mesma família na mesma coluna e o sweep de um achata sobre o outro.
7. **Warmup não-cronometrado** antes das rodadas cronometradas, para que percentis e QPS descrevam cache quente de forma consistente.
8. **QPS = 1 / min(média por rodada)** — best-of-N do protocolo ANN-Benchmarks. `mean`/`std` reportados são dispersão **intra-amostra**, não variância entre rodadas; o relatório diz isso explicitamente.
9. **`query_cap` aparece no rótulo.** Índice O(N) por consulta pode ter a amostra reduzida — e o corte vai no label (`[q=200]`), nunca escondido.

### 4.3 Justiça entre sistemas comparados

10. **`lists` do IVFFlat deriva do N real.** Sob `--full-train`, ler o tamanho verdadeiro do HDF5. Derivar do default (5000 → `lists=5`) construiria um IVFFlat aleijado sobre 1M — comparação desleal disfarçada de medição.
11. **`probes` é clampado ao `lists` ANTES de deduplicar.** Em pgvector `probes > lists` é no-op; um rótulo `probes=10` num índice `lists=5` reportaria ponto duplicado sob rótulo errado.
12. **`query_rescore` do DiskANN escala com `sls`** (até o teto 1000 do pgvectorscale). Congelar rescore abaixo do sls limita o recall enquanto o QPS cai — fabrica um platô falso.
13. **Nunca fabricar um opclass.** Os AMs próprios são l2-only; sob `--metric cosine` o harness **avisa e pula**, em vez de emitir DDL inválida.

### 4.4 Estatística (a lição M123/M130/M131)

14. **Comparação de dois sistemas exige teste pareado**, não comparação de médias nem coeficiente de variação. Headline: **teste de randomização/permutação pareado** (Smucker, Allan & Carterette, CIKM 2007 — o recomendado em IR; Wilcoxon/sinal rejeitados por descartarem magnitude e empates).
15. **IC 95% por bootstrap pareado percentil**; **t-test pareado como cross-check concordante** (Urbano, SIGIR 2013 — reportar os três).
16. **Correção Monte-Carlo obrigatória:** `p = (count + 1) / (B + 1)`. A atribuição observada é uma das permutações; `p` nunca é 0.
17. **Seed fixa** (`20260720`) → `p` e IC reproduzem exatamente.
18. **Reportar tamanho de efeito, não só `p`:** `mean_diff`, `cohens_dz`, `wins/losses/ties`.

### 4.5 Determinismo e proveniência

19. **Corpus semeado.** Mesma seed → corpus e queries bit-idênticos. Os análogos OSS (testes Perl do pgvector, pgvectorscale) **não** semeiam; este é um diferencial deliberado.
20. **Desempate determinístico** em toda consulta de ranking (`ORDER BY score, doc_id`). Sem o `doc_id`, empates na fronteira do top-N são resolvidos por ordem física da tabela — não-determinístico entre execuções.
21. **Todo relatório carrega proveniência:** `sha` do git, `date`, `seed`, `host`, `gt_source` e um campo `methodology` em prosa.
22. **Regressão byte-idêntica** (`assert_byte_identical`): conjuntos de qid diferentes são **erro tipado**, nunca comparação parcial silenciosa. Ferramenta que o ann-benchmarks/VectorDBBench/ClickBench/BenchBase não oferecem — é capacidade própria e pegou defeitos de classe M114/M126.
23. **Chave da OpenAI nunca é logada, ecoada ou persistida**; ausência levanta `RuntimeError`, dimensão errada ou vetor todo-zero levanta `ValueError`. Nada de fallback silencioso para vetor zero, que corromperia o recall sem avisar.

---

## 5. Superfície de linha de comando

```bash
python -m theodb_bench [opções]
```

| Flag | Default | Efeito |
|---|---|---|
| `--seed` | 42 | semente do RNG |
| `--n` | 5000 | tamanho do corpus |
| `--dim` | 128 | dimensão (ignorada quando `--hdf5` define a sua) |
| `--n-queries` | 100 | número de consultas |
| `--k` | 10 | k do recall@k |
| `--metric` | `l2` | `l2` \| `cosine` |
| `--runs` | 3 | rodadas para o best-of-N |
| `--index` | `hnsw` | `hnsw`\|`ivfflat`\|`diskann`\|`both`\|`all`\|`theodb_ivfflat`\|`theodb_hnsw`\|`4way` |
| `--hdf5` | — | dataset ANN-Benchmarks (dim inferida do arquivo) |
| `--full-train` | off | train completo + GT via `neighbors` (caminho ≥1M) |
| `--theodb-hnsw-query-cap` | — | limita a amostra só do `theodb_hnsw` (scan O(N)/consulta) |
| `--dsn` | de `PG*` | DSN libpq |
| `--out` | `benchmarks/artifacts` | destino dos artefatos |

**Saída:** `{out}/{date}-{dataset}.json` e `.md`. Uma linha por resultado no stdout com `recall@k`, `qps`, `p95`, `build`, `size`.

**Esquema do relatório JSON:** `sha`, `date`, `seed`, `n`, `dim`, `n_queries`, `k`, `metric`, `runs`, `dataset`, `host`, `gt_source`, `methodology`, `results[]` — cada resultado com `index`, `params`, `recall_at_k`, `qps`, `build_ms`, `index_bytes`, `p50`, `p95`, `p99`, `mean`, `std`.

---

## 6. Integração de CI (recriar exatamente)

### Job `harness-unit` — barato, sem container

```yaml
defaults: { run: { working-directory: benchmarks } }
timeout-minutes: 15
steps:
  - pip install -r requirements.txt ruff vulture
  - ruff check theodb_bench tests
  - vulture theodb_bench --min-confidence 80
  - pytest -m "not integration" -q
```

### Job `image-and-bench` — o portão de medição

```yaml
timeout-minutes: 45     # ~2x o observado (27,9 min medidos em 2026-07-27)
```

1. Cache do dataset em `benchmarks/.datasets/glove-25-angular.hdf5`.
2. Download com **verificação de sha256** — `51004cb0ae962159f0db507a51fec2b395de14b166f55976c89f16bd2f8b6391`.
3. Rodar o harness capado para tempo de CI:
   ```
   python -m theodb_bench --hdf5 .datasets/glove-25-angular.hdf5 --index hnsw \
     --metric cosine --seed 42 --n 10000 --n-queries 200 --k 10 --runs 2 --out /tmp/bench
   ```
4. **Portão:** falhar se `max(recall_at_k) < 0.90`. HNSW sobre dataset real clusterizado tem de recallar alto — o portão existe para pegar um harness silenciosamente quebrado.
5. `pytest -m integration` contra o container (o passo mais caro: ~22 min, ~80% do wall-clock).

O sweep decision-grade de diskann roda **fora** do CI e é commitado em `wiki/benchmarks/`.

---

## 7. Configuração e dependências

`benchmarks/pyproject.toml`:
```toml
[tool.pytest.ini_options]
testpaths = ["tests"] ; pythonpath = ["."]
markers = ["integration: requires a running theo-db:dev container"]
[tool.ruff]      line-length = 100 ; target-version = "py310"
[tool.vulture]   ignore_names = ["entry_sql"]   # fixture pytest de efeito colateral
```

`benchmarks/requirements.txt` — **todas dev-only, nunca embarcadas na imagem** (a restrição D1 de licença não se aplica a ferramenta de dev, mas a nota deve ser mantida explícita):

| Pacote | Licença | Papel |
|---|---|---|
| `numpy>=1.26` | BSD-3 | toda a matemática |
| `psycopg2-binary>=2.9` | LGPL c/ exceção de linking | driver (isolado em `db.py`) |
| `pytest>=7` | MIT | suíte |
| `h5py>=3.10` | BSD-3 | datasets ANN-Benchmarks |
| `fastembed>=0.3` | Apache-2.0 | modelo local de embedding (onnxruntime, sem torch) |
| `scann>=1.4` | Apache-2.0 | baseline head-to-head do M33; exige AVX2 |
| `scipy` | BSD-3 | **opcional** — só o p-valor exato do t-test; sem ele, aproximação normal registrada em `p_ttest_method` |

Permutação e bootstrap são numpy puro **de propósito**: a compatibilidade binária scipy↔numpy 2.x é frágil, e os dois são reamostragens triviais e bem compreendidas.

---

## 8. Suíte de testes

65 arquivos. `theodb_bench/test_*.py` (9) testam o pacote; `tests/` (56) cobrem integração e por-marco.

Testes do núcleo que **têm de existir** antes de qualquer número ser publicado:

| Arquivo | Prova |
|---|---|
| `test_significance.py` | nulo (a==b) → não-significativo; deslocamento uniforme → significativo; empates contados; entrada ruim → erro tipado |
| `tests/test_recall.py` | semântica de limiar de distância, contrato float32, `k > N` levanta |
| `tests/test_metrics.py` | percentis, best-of-N, nDCG, recall@n |
| `tests/test_dataset.py`, `tests/test_dataset_hdf5.py` | determinismo por seed; validação de faixa do HDF5 |
| `tests/test_harness.py` | orquestração com `db` injetado (sem container) |
| `tests/test_db.py` | SQL puro e mapeamento de erro sem conexão |
| `test_ann_adapter.py` | conformidade com o contrato `BaseANN` |

O marcador `integration` separa o que exige container. `harness-unit` roda `-m "not integration"`.

---

## 9. Ordem de reconstrução sugerida

Se for reescrever, esta ordem mantém o CI verde o quanto antes e adia o caro:

| Fase | Entrega | Desbloqueia |
|---|---|---|
| **1** | `pyproject.toml`, `requirements.txt`, esqueleto do pacote | `harness-unit` volta a existir |
| **2** | `recall.py` + `metrics.py` + `dataset.py` **com testes** (puro, sem I/O) | núcleo matemático auditável |
| **3** | `significance.py` + testes | qualquer comparação entre dois sistemas |
| **4** | `db.py` (fronteira) + `harness.py` (orquestração) | `python -m theodb_bench` |
| **5** | `__main__.py` com os specs de índice | portão de CI da fase 3 do `image-and-bench` |
| **6** | `regression.py`, `ann_adapter.py` | A/B byte-idêntico e compatibilidade externa |
| **7** | pilar lexical (`beir`, `knownitem`, `lexical_engines`, `hybrid`) | nDCG/BEIR/RRF |
| **8** | `columnar.py` **reescrito** contra `theodb_columnar` | pilar colunar (ver nota §3.2) |
| **9** | `artifacts/README.md` + `archive/README.md` | contrato dado-vs-conhecimento |

As fases 2–5 são o caminho crítico: sem elas, nenhum claim de performance do projeto é sustentável.

---

## 10. O que deliberadamente **não** reconstruir

- **`m107_graph_spike/target/`** — saída de build do Cargo, estava versionada por engano.
- **Os 130 runners de raiz por marco** — repro de marcos encerrados. Recuperar sob demanda com `git show HEAD:benchmarks/<arquivo>` quando um número específico precisar ser reproduzido.
- **`archive/`** — já eram órfãos declarados, arquivados e não apagados conforme `audit-trail-rotation.md`.
- **Qualquer coisa que dependa de `pg_mooncake`/`pg_duckdb`** — removidos no M143.

---

## 11. Consequências honestas de não reconstruir

Ditas sem rodeio, para que a decisão seja informada:

1. **20 marcos com medição publicada em `wiki/` ficam sem caminho de reprodução no working tree.** Os números continuam válidos e o código continua em git; o que se perde é o acesso imediato.
2. **O CI perde o portão de recall.** O `recall ≥ 0.90` sobre glove-25 é o que hoje detecta um harness silenciosamente quebrado.
3. **O mandato measurement-first do `CLAUDE.md` fica sem implementação.** Enquanto isso durar, todo novo claim de performance nasce sem o pré-requisito que o próprio projeto declara.
4. **A ferramenta de regressão byte-idêntica some.** Nenhuma das suítes oficiais (ann-benchmarks, VectorDBBench, ClickBench, BenchBase) a oferece — era capacidade própria.
