# M47 / FU-1 — same-graph scan-allocation micro-benchmark

Caracterização (não competição) do custo de **alocação** do ground-loop do scan HNSW que o M46 remove, medido **same-graph** via criterion. `presized` = scratch pré-alocado (M46); `unsized` = `::new()`.

> **O grafo é sintético** (`build_graph` em `benches/scan_hot_path.rs`): random-regular, N=50k nós, m0=32 vizinhos aleatórios cada, vetores aleatórios, seed=42 — construído UMA vez e compartilhado pelas duas bench_functions (mesmo grafo; a única variável é a estratégia de alocação). É representativo do **workload de ALOCAÇÃO** (o custo escala com ef, grau do nó e contagem de visitas — todos reproduzidos), **NÃO de recall**. A correção de recall do refactor é provada à parte, sobre um `HnswIndex` REAL, pelo oráculo pg_test `ground_search_matches_brute_exact_knn` (`ann/scan_core.rs`).

**Ambiente:** CPU `13th Gen Intel(R) Core(TM) i7-1355U`; código `git 6839b54`; container pinado em cores 8-11; box com 11 containers concorrentes do usuário (sem box quieto disponível).

**Link gate (Coverage #7) — validado:** `cargo bench --no-run --bench scan_hot_path` → `Finished bench profile [optimized]; Executable benches/scan_hot_path.rs` (linka standalone, zero `pg_sys`; `MemNeighborSource`/`HnswIndex` são `#[cfg(pg_test)]` e ficam fora do bench).

## Delta presized vs unsized (mean±std de 3 runs, µs)

| ef | presized (µs) | unsized (µs) | Δ mean | presized-faster runs | direção consistente? |
|---|---|---|---|---|---|
| 100 | 908.3±283.6 | 932.5±135.7 | -2.6% | 2/3 | **NÃO (flipa)** |
| 200 | 1792.8±336.6 | 1922.2±323.8 | -6.7% | 3/3 | sim |
| 400 | 4582.4±538.2 | 4780.6±826.1 | -4.1% | 2/3 | **NÃO (flipa)** |

*(Δ mean negativo = presized mais rápido na média.)*

## Veredito honesto: HONEST_NEGATIVE_WITHIN_NOISE

presized (M46 pre-size) is directionally faster on the mean at every ef (-2.6/-6.7/-4.1%), but the run-to-run box noise (absolute times swing up to -37%/+39% around the mean) EXCEEDS the effect; the presized-vs-unsized direction FLIPS run-to-run at ef=100 (run 2) and ef=400 (run 3). Only ef=200 is presized-faster in all 3 runs. On this shared dev box the same-graph allocation delta is NOT robustly significant. Valid per plan EC-2. NOTE: the tight run-1 criterion CIs (non-overlapping at ef=100/400) are WITHIN-RUN sampling precision, not run-to-run reproducibility — the run-to-run flips are the honest signal.

Os tempos absolutos oscilam até **-37%/+39%** em torno da média entre runs (o **ruído** do box compartilhado domina; a célula mais larga é presized/ef=100), e a direção presized-vs-unsized **flipa** run-to-run em ef=100 e ef=400 — só ef=200 é presized-faster nos 3 runs. O efeito (~2-7%) é menor que o ruído. **Resultado válido** (plano M47 / EC-2). Os CIs tight do criterion (run 1) são precisão **dentro-do-run**, não reprodutibilidade entre runs — os flips run-to-run são o sinal honesto.

## Caveat EC-2 (limite superior — honesto)

This synthetic-graph, no-page-I/O micro-bench is an UPPER BOUND on the production gain: it isolates the allocation cost, so allocation is a larger fraction of wall-time than in production, where page reads amortize it. No product/QPS-superiority claim is made from this number — the production claim remains the SQL quiet-box benchmark.

## Metodologia (reprodução)

```bash
docker build --target theodb-rs-builder -t theodb-rsbuild:m47 .
# link gate (bench standalone, sem pg_sys):
docker run --rm theodb-rsbuild:m47 sh -c 'cd /tmp/theodb_rs && cargo bench --no-run --bench scan_hot_path'
# medição pinada (isola dos containers concorrentes), 3×:
docker run --rm --cpuset-cpus=8,9,10,11 theodb-rsbuild:m47 sh -c \
  'cd /tmp/theodb_rs && cargo bench --bench scan_hot_path'
```

Grafo: `build_graph(50_000, 128, m0=32, seed=42)` (random-regular sintético) construído uma vez; duas `bench_function` chamam o MESMO `ground_search(&g, …, presized_bool)` sobre ele; ef ∈ {100,200,400}; criterion 100 samples/bench.
