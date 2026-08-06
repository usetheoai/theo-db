# Formação de Engenharia — TheoDB

### Guia completo para construção de um banco de dados PostgreSQL moderno com IA, índices vetoriais e sistemas distribuídos

> Currículo técnico interno do TheoDB. Ensina engenharia de banco de dados **através do sistema que estamos
> construindo de verdade** — cada conceito aterrissa em código real (`file:line`), uma decisão registrada (ADR),
> um benchmark reproduzível e o estado da arte com o **gap honesto**. Não é um livro-texto genérico: é o manual
> que forma quem vai desenvolver o TheoDB.

---

## Por que este livro existe (e o que ele NÃO é)

Existem livros excelentes sobre álgebra linear (Strang), algoritmos (CLRS), internals do PostgreSQL (Suzuki) e os
papers seminais de bancos. **Não vamos reescrevê-los** — seríamos piores que os originais (Regra 9, Não Reinvente).

O que **nenhum** desses livros tem é o nosso sistema: 12 ADRs de decisões reais, 27 blueprints de descoberta com
citações verificadas, 10 artefatos de benchmark medidos, e ~6.400 linhas de Rust implementando tipo vetorial,
IVFFlat, HNSW page-native, SIMD, quantização e superfície de IA embarcada. **Esse é o fosso.** Um capítulo de HNSW
que vai do paper de Malkov & Yashunin até `ann/hnsw.rs` e o benchmark M35 (100 QPS @ recall 0.98 a 1M) e o gap
honesto de ~25× vs ScaNN — isso ninguém no mundo consegue escrever, porque é o nosso banco.

Portanto o livro tem **dois modos**:

| Modo | Partes | O que faz |
|---|---|---|
| **CURADO** | I–III (matemática, história, PG internals) | Trilha de leitura anotada às fontes canônicas + o "porquê isto importa no TheoDB". Aterrissamos, não reproduzimos. |
| **ORIGINAL** | IV–IX (pgrx, índices, vetorial, benchmarks, SIMD, IA-no-banco) | O coração. Cada capítulo é original e defensável, ancorado em código + ADR + benchmark reais. |
| **ROADMAP** | X–XII (distribuído, operação, engenharia de perf) | Marcado honestamente: o que já existe vs o que é aposta futura (ex.: HA/Patroni ainda não implementado). |

---

## O contrato de honestidade (Regra 3, inquebrável)

Este manual segue a mesma disciplina dos nossos blueprints e benchmarks:

1. **Toda citação de código resolve no disco** (`arquivo:linha`). Zero citações alucinadas — se uma referência não
   existe, ela não entra.
2. **Todo número de performance vem de um artefato de benchmark reproduzível** em `docs/benchmarks/`, com hardware
   e comando de reprodução. Nunca "~X mais rápido" sem link (`../../.claude/rules/public-copy.md`).
3. **Gaps são explícitos.** Onde o TheoDB perde para o SOTA (ex.: ~25× vs ScaNN no eixo ANN puro — M33), o livro
   diz isso com o número, não esconde.
4. **Aspiracional é marcado como aspiracional.** Partes ainda não implementadas (HA, columnar em produção) são
   claramente roadmap, não fato.

---

## O padrão de cada capítulo ORIGINAL (Partes IV–IX)

Todo capítulo do coração segue a mesma espinha de cinco camadas:

```
1. TEORIA          — o conceito, o paper seminal, a intuição
2. MATEMÁTICA      — as fórmulas, a complexidade (Big-O de build e query)
3. NOSSA IMPLEMENTAÇÃO — o código real do TheoDB (arquivo:linha) + as decisões (ADR)
4. NOSSO BENCHMARK — os números medidos (docs/benchmarks/), com hardware e repro
5. SOTA & GAP      — como o estado da arte faz, onde ganhamos, onde perdemos (honesto)
```

Se um capítulo não consegue preencher a camada 3 (nossa implementação), ele pertence às Partes X–XII (roadmap),
não ao coração.

---

## Índice

Legenda: 🟢 **ORIGINAL** (ancorado no nosso código) · 🔵 **CURADO** (trilha de leitura) · 🟡 **ROADMAP** (ainda não construído)

### PARTE I — Fundamentos Matemáticos 🔵
- 1. Álgebra Linear — vetores, normas L1/L2, produto interno, espaços, SVD, PCA · *aterrissa em: `vec.rs` (kernels de distância)*
- 2. Probabilidade & Estatística — distribuições, variância, covariância · *aterrissa em: recall@k, o RNG SplitMix64 de `ann/mod.rs`*
- 3. Algoritmos & Estruturas — complexidade, árvores, grafos, heap, skip lists · *aterrissa em: HNSW (grafo em camadas ~ skip list)*
- Fontes canônicas: Strang *Introduction to Linear Algebra* + MIT 18.06; Ross; CLRS; Skiena.

### PARTE II — Arquitetura de Banco de Dados 🔵
- 4. A evolução: System R → Ingres → PostgreSQL → Spanner/Aurora/AlloyDB
- 5. Por que PostgreSQL é a nossa base — e por que **não forkamos o engine** (`docs/adr/0001-no-engine-fork.md`)
- Papers: System R, ARIES, Spanner, Aurora, C-Store, HyPer, Calvin.

### PARTE III — Engine PostgreSQL 🔵
- 6. Organização do código-fonte (`src/backend/`)
- 7. Postmaster, Shared Memory, Buffer Pool, LWLocks
- 8. Parser (`gram.y`/`scan.l`), Planner (paths, custo, seletividade), Executor (scans, joins)
- 9. MVCC (snapshots, xmin/xmax, HOT) e WAL (checkpoint, recovery, ARIES) — *o mínimo para entender nosso index-AM*
- Fontes: *The Internals of PostgreSQL* (Suzuki); doc oficial; código-fonte.

### PARTE IV — Extensões PostgreSQL 🟢
- 10. pgrx: arquitetura de uma extensão Rust · *`lib.rs`, `Cargo.toml`, ADR `0006-own-code-postgres-based-rust-go.md`, `0009-theodb-rs-api-surface-single-module.md`*
- 11. Memory Context, SPI, tipos customizados, `#[pg_extern]`, hooks · *`api.rs` (640 LoC), blueprint `pgrx-extension-foundation`*
- 12. Operator Classes & Access Methods — a API que o TheoDB implementa · *`am/mod.rs`, ADR `0010-m26-index-am-scope.md`*

### PARTE V — Índices 🟢/🔵
- 13. Os clássicos (curado): B-tree, Hash, GiST, GIN, SP-GiST, BRIN
- 14. **A Index Access Method API na prática** 🟢 — `IndexAmRoutine`, `ambuild`/`aminsert`/`amgettuple`/`amvacuumcleanup` · *`am/mod.rs`, `am/build.rs`, `am/scan.rs`, blueprint `m26-index-am`*
- 15. **Persistência page-native & WAL numa extensão** 🟢 — páginas, `GenericXLog`, partial-read · *`am/page.rs` (757 LoC), ADR `0011-m31-rescope-simd-followup.md`*

### PARTE VI — Bancos Vetoriais 🟢 (o coração do TheoDB)
- 16. Embeddings & o tipo `vector` · *`vec.rs`, blueprint `m20-own-vector-type`, ADR sobre f32-parity*
- 17. Distâncias: L2, cosine, inner product — a matemática e a paridade f32 · *`vec.rs:*` (kernels SIMD), `ann/mod.rs` (`Metric`)*
- 18. **IVFFlat** 🟢 — k-means++, listas invertidas, probes · *`ann/ivf.rs`, `am/scan.rs`, benchmark **M34** (`docs/benchmarks/m34-ivfflat-reloption.json`)*
- **19. HNSW** 🟢 — **[capítulo-farol, escrito](./parte-06-vetorial/19-hnsw.md)** · *`ann/hnsw.rs`, `am/hnsw_page.rs`, benchmark **M35***
- 20. Quantização (SBQ / PQ / OPQ / AQ) 🟢 — comprimir vetores no índice · *`sbq.rs`, blueprint `m22-own-quantization`* — *o próximo salto de QPS (fecha o gap ScaNN)*
- 21. DiskANN / Vamana / NSG / ScaNN 🔵 — o SOTA que ainda não implementamos · *ADR `0004-scann-fork-decision.md`, benchmark **M33** (gap honesto)*

### PARTE VII — Benchmarks 🟢
- 22. Como medir sem mentir: recall, QPS, p50/p95/p99, build time, index size · *`benchmarks/theodb_bench/`, blueprint `vector-recall-benchmark-harness`*
- 23. Armadilhas reais: degeneração de dados, cross-use do planner, cache vs pages-read · *ADR `0012-benchmark-data-degeneracy.md`, benchmarks **M31b/M32/M34/M35***
- Fontes: ANN-Benchmarks, Big ANN Challenge, VectorDBBench.

### PARTE VIII — SIMD 🟢
- 24. SSE/AVX2/AVX512/NEON, runtime dispatch, FMA · *`vec.rs` (kernels), blueprint `m31b-simd-distance`, benchmark **M31b***
- Fontes: Intel Optimization Manual; Agner Fog.

### PARTE IX — IA dentro do banco 🟢
- 25. Embeddings via SQL, chat, NL→SQL, hybrid search (BM25 + RRF) · *`embed.rs`, `chat.rs`, `nl.rs`, `hybrid.rs`, blueprints `m18/m19/m7-*`*
- 26. Segurança: prompt injection, NL→SQL seguro · *`nl.rs`, ADR `0007-synchronous-per-row-model-http.md`, blueprint `m7-nl-to-sql-safe`*
- Papers: Sentence-BERT, ColBERT, DPR, BEIR, Spider, BIRD, RRF.

### PARTE X — Sistemas Distribuídos 🟡 (roadmap)
- 27. Raft/Paxos, streaming & logical replication, Patroni, HA, PITR — *aposta futura; hoje é single-node*

### PARTE XI — Operação 🟡 (parcial)
- 28. Kubernetes Operator, CRDs, reconciliation, observabilidade · *`docs/operations/`, blueprints `m23/m24`*

### PARTE XII — Engenharia de Performance 🟢/🔵
- 29. Flamegraphs, perf, cache lines, false sharing, NUMA, huge pages · *aterrissa em: o profiler de fase de `am/scan.rs` (`THEODB_SCAN_PROFILE`)*

### APÊNDICES
- A. Mapa do código-fonte para estudar (PostgreSQL, pgvector, pgrx, DuckDB, Neon, CockroachDB)
- B. Nossos ADRs (12) — o registro de decisões
- C. Nossos blueprints (27) — a pesquisa de descoberta
- D. Nossos benchmarks (10) — as medições
- E. Blogs & competições recomendados

---

## Estado de escrita

| Capítulo | Status |
|---|---|
| 19 — HNSW (farol) | ✅ **escrito** — `parte-06-vetorial/19-hnsw.md` |
| Demais | 📋 índice definido; a escrever incrementalmente, um capítulo por vez, cada um ancorado em código+benchmark reais |

O livro cresce **capítulo a capítulo** (não num único despejo — isso seria raso e alucinado, o oposto de
FAANG-level). O capítulo 19 é o **template de qualidade**: todo capítulo original deve alcançar aquele nível de
aterrissagem no código e nos números.

**A máquina que produz capítulos:** a skill `/deep-research` *(fora do versionamento)* executa
este contrato — pesquisa profunda (nosso sistema + papers + benchmarks + cálculos + técnicas), destila nas 5
camadas, e o validador `scripts/validate_citations.py` mecaniza o contrato de honestidade (toda citação resolve no
disco; todo número tem benchmark ou `UNBENCHMARKED`; toda URL no allowlist). Um capítulo só é dado por pronto com
o validador em **PASS** — como o capítulo 19.
