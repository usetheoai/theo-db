# M47 / FU-1 — same-graph scan-allocation micro-benchmark

Caracterização (não competição) do custo de **alocação** que o M46 remove no ground-loop do scan HNSW, medido **same-graph** (mesmo grafo seeded, imune a variação de grafo) via criterion. `presized` = scratch pré-alocado (M46); `unsized` = `::new()` (pré-M46).

**Ambiente:** CPU `13th Gen Intel(R) Core(TM) i7-1355U`; código `git 5475990`; container pinado em cores 8-11; box com 11 containers concorrentes do usuário (sem box quieto disponível).

## Delta presized vs unsized (mean±std de 3 runs, µs)

| ef | presized (µs) | unsized (µs) | Δ mean | presized-faster runs | direção consistente? | CI run1 (presized / unsized) |
|---|---|---|---|---|---|---|
| 100 | 908.3±283.6 | 932.5±135.7 | -2.6% | 2/3 | **NÃO (flipa)** | [856.43,920.64] / [1014.22,1075.02] |
| 200 | 1792.8±336.6 | 1922.2±323.8 | -6.7% | 3/3 | sim | [1557.74,1657.57] / [1605.97,1699.22] |
| 400 | 4582.4±538.2 | 4780.6±826.1 | -4.1% | 2/3 | **NÃO (flipa)** | [4293.1,4420.17] / [4523.54,4627.65] |

*(Δ mean negativo = presized mais rápido na média.)*

## Veredito honesto: HONEST_NEGATIVE_WITHIN_NOISE

presized (M46 pre-size) is directionally faster on the mean at every ef, but the run-to-run box noise (absolute times swing ±13-25%) EXCEEDS the effect (~2-7% on the mean); the presized-vs-unsized direction FLIPS run-to-run at ef=100 (run 2) and ef=400 (run 3). Only ef=200 is presized-faster in all 3 runs. On this shared dev box (11 concurrent user containers, no quiet box available) the same-graph allocation delta is NOT robustly significant. Valid per plan EC-2.

Os tempos absolutos oscilam ±13-25% entre runs (o **ruído** do box compartilhado domina), e a direção presized-vs-unsized **flipa** run-to-run em ef=100 e ef=400 — só ef=200 é presized-faster nos 3 runs. O efeito (~2-7%) é menor que o ruído. **Resultado válido** (plano M47 / EC-2: "o pre-size pode ser ruído mesmo isolado").

## Caveat EC-2 (limite superior — honesto)

This same-graph micro-bench is an UPPER BOUND on the production gain: it isolates the allocation cost with NO page I/O, so allocation is a larger fraction of wall-time than in production, where page reads amortize it. No product/QPS-superiority claim is made from this number — the production claim remains the SQL quiet-box benchmark.

## Metodologia (reprodução)

```bash
# builder stage (Rust + pgrx toolchain):
docker build --target theodb-rs-builder -t theodb-rsbuild:m47 .
# link gate (bench binário standalone, sem pg_sys):
docker run --rm theodb-rsbuild:m47 sh -c 'cd /tmp/theodb_rs && cargo bench --no-run --bench scan_hot_path'
# medição pinada (isola dos containers concorrentes), 3×:
docker run --rm --cpuset-cpus=8,9,10,11 theodb-rsbuild:m47 sh -c \
  'cd /tmp/theodb_rs && cargo bench --bench scan_hot_path'
```

Grafo: `HnswIndex::build(seeded_corpus(50_000,128,42), …, seed=42)` construído UMA vez; scan via `MemNeighborSource` sobre o MESMO grafo; ef ∈ {100,200,400}; criterion 100 samples/bench.
