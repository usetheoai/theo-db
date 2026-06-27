# Discovery Plan: Vector Recall@k Benchmark Harness — FAANG-grade Methodology

> **Version 1.0** — Investiga a **metodologia SOTA de benchmark de recall@k + latência/QPS** para índices vetoriais em PostgreSQL (pgvector hoje; extensível a pgvectorscale/ScaNN-AM), ancorada no **ANN-Benchmarks** (padrão de fato da área) e nos harness de recall já presentes no `pgvector`/`pgvectorscale`. O blueprint resultante é o insumo de design do **harness que é o GATE measurement-first do M2** (ADR 0002): ele destrava a decisão de índice e o gatilho de fork D3.

**Slug:** `vector-recall-benchmark-harness`
**Owner:** paulohenriquevn (CTO)
**Created:** 2026-06-27
**Time budget:** 6h (per-project em ADR D1)

## Context

O ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (LOCKED) e o `ROADMAP.md` M2 elevaram o **harness de benchmark recall@k a 1º item / gate** do pilar killer: nenhuma decisão de índice (adotar pgvectorscale / forkar / ScaNN-as-PG-AM) nem claim de performance acontece antes dele. A discovery anterior (`alloydb-vector-ai-implementation`, SHIPPABLE 98.7) provou que **nenhum harness de recall reproduzível existe** para herdar — os análogos só têm testes de *correção* + micro-benchmarks de função; todos os números de performance estão `UNBENCHMARKED`. `public-copy.md` + PRD D3 proíbem claim sem benchmark reproduzível. Esta discovery fecha o **como medir** antes de escrever o harness, em grau FAANG / research-DB.

## Objective

**Uma frase:** o blueprint deve permitir **implementar um harness reproduzível que meça recall@k + latência (p50/p95/p99) + QPS + build-time + memória de um índice vetorial em PostgreSQL, com ground-truth exato e rigor estatístico**, de forma que TheoDB possa afirmar paridade/superioridade vs ScaNN/AlloyDB com evidência.

Critérios de sucesso do blueprint:
- [ ] Todas as questões respondidas com citação a `.claude/knowledge-base/references/` (ou BLOCKED honesto)
- [ ] Definição operacional precisa de **recall@k** + método de **ground-truth exato**, com citação
- [ ] Protocolo de medição de latência/QPS (warm/cold, ≥N runs, mean±std, percentis) ancorado no SOTA
- [ ] Lista de datasets de referência + aquisição reprodutível
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `test/t/*recall*.pl`, `test/`, `README.md`, `src/hnsw.h`, `src/ivfflat.h`, `Makefile` | Harness de recall real (Perl) + o oráculo "exact = perfect recall" (README l.197); macros de bench |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/benches/{distance.rs,lsr.rs}`, `tests/{test_basic_operations.py,requirements.txt}`, `scripts/run-python-tests.sh`, `Cargo.toml`, `README.md`, `DEVELOPMENT.md` | Benches criterion + harness de teste Python contra PG real |
| Allowlist web | `github.com` (ANN-Benchmarks `erikbern/ann-benchmarks`), `arxiv.org` (recall/ANN benchmarking papers) | ANN-Benchmarks é o padrão SOTA de medição recall×QPS |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| Os **algoritmos** de índice (HNSW/SBQ/ScaNN internals) | Já cobertos pelo blueprint `alloydb-vector-ai-implementation` — **evitar re-trabalho** |
| `.claude/knowledge-base/references/{citus,hydra,duckdb,pg_mooncake,patroni,pgbackrest,cloudnative-pg,supabase-postgres,paradedb}/` | Não são sobre medição de recall vetorial |
| Código-fonte do AlloyDB | Closed-source |
| `*/target`, `*/build`, vendor trees | Artefatos |
| Web fora de `discover-web-allowlist.txt` | R5 |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** `pgvector`: 2h · `pgvectorscale`: 1.5h · web (ANN-Benchmarks + papers): 2.5h. Total 6h.

**Rationale:** ANN-Benchmarks é a fonte SOTA da metodologia → maior fatia. pgvector tem o harness de recall mais direto (Perl + oráculo exact). pgvectorscale confirma o protocolo de teste contra PG real.

**Alternatives considered:** só ler ANN-Benchmarks (rejeitado — perde como medir *dentro* do PG); split igual (rejeitado).

**Stop condition — per question:** Fase A vazia após 3 retries → BLOCKED ("Fase A exhausted"), segue. **Web inalcançável/404** → BLOCKED ("source unreachable"), nunca substituir por fonte fora da allowlist (R5/R6).

**Stop condition — per project:** budget esgotado → questões restantes BLOCKED ("budget exhausted"). Se todas done/blocked → `<promise>BLUEPRINT_BLOCKED</promise>` (não COMPLETE).

**Anti-pattern:** nunca fabricar metodologia/número não publicado (R3/R6).

### D2 — Investigation depth

**Decision:** ler os arquivos de teste/bench ponta a ponta (extrair o protocolo executável); ANN-Benchmarks → ler README + o módulo de cálculo de métricas + a definição de datasets.

**Rationale:** o valor é o protocolo reprodutível, não auditar os repos inteiros (KISS).

**Consequences:** o blueprint entrega um protocolo citável; partes não publicadas viram open question/UNBENCHMARKED.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad map) | Fase B (deep Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o `pgvector` mede **recall** nos seus testes, e qual é o **oráculo de ground-truth**? | tests | `.claude/knowledge-base/references/pgvector/test/t/` (`*recall*.pl`), `README.md` | Glob `test/t/*recall*.pl`; grep `recall`/`SELECT.*ORDER BY.*<->` nos `.pl` | Ler `012_hnsw_vector_build_recall.pl`, `005_ivfflat_query_recall.pl`, `044_hnsw_iterative_scan_recall.pl` + README §"exact nearest neighbor ... perfect recall" | Protocolo: como geram dados, rodam exact vs índice, computam recall%, threshold de aceite; `path:line` |
| Q2 | Quais **dependências** um harness de recall precisa, e o que o ANN-Benchmarks + os testes do pgvectorscale puxam? | deps | `.claude/knowledge-base/references/pgvectorscale/tests/requirements.txt`, allowlist ANN-Benchmarks `requirements.txt` | grep deps em `tests/requirements.txt`; WebFetch ANN-Benchmarks `requirements.txt` (github) | Ler ambos | Lista de deps (numpy, psycopg, h5py p/ datasets hdf5, etc.) + versões + papel de cada uma |
| Q3 | Como se **adquire datasets de referência reprodutíveis** (glove/sift/cohere) e se garante seeds/reprodutibilidade? | tools | allowlist ANN-Benchmarks (github), `.claude/knowledge-base/references/pgvectorscale/scripts/run-python-tests.sh` | WebFetch ANN-Benchmarks datasets module (github); ler o script de testes | Ler módulo de datasets + script | Como baixam/cacheiam datasets (hdf5 com train/test/neighbors), seed, formato do ground-truth |
| Q4 | **(SOTA)** Como o **ANN-Benchmarks** define a métrica **recall@k** e computa o **ground-truth exato (brute-force k-NN)**? | techniques | allowlist ANN-Benchmarks (github), `arxiv.org` (paper ANN-Benchmarks, Aumüller et al.) | WebFetch o módulo de métricas/ground-truth do repo + o paper | Ler a definição de recall (k-recall@k / recall@k) + o cálculo exato dos vizinhos | Definição matemática de recall@k + algoritmo de ground-truth. **R1** SOTA; **R2** repo+paper; **R3** N/A (definição, não perf) |
| Q5 | **(SOTA)** Qual o **protocolo de medição de latência/QPS** rigoroso (warm/cold cache, ≥N runs, mean±std, p50/p95/p99, single vs batch)? | techniques | allowlist ANN-Benchmarks (github), `arxiv.org` | WebFetch o runner/algorithm-runner do ANN-Benchmarks + paper | Ler como medem QPS×recall (Pareto frontier), repetições, warm-up | Protocolo de latência/QPS reprodutível. **R1/R2**; **R3** marcar números de exemplo como `UNBENCHMARKED` se sem método |
| Q6 | **(SOTA)** Como o `pgvector`/`pgvectorscale` instrumentam **build-time/memória** e micro-latência (bench macros, criterion)? | techniques | `.claude/knowledge-base/references/pgvector/src/{hnsw.h,ivfflat.h}`, `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/benches/{distance.rs,lsr.rs}` | grep `*_BENCH`/`elapsed`/`Instant` nos headers e benches | Ler as macros `HNSW_BENCH`/`IVFFLAT_BENCH` + os benches criterion | Como medem build/insert time e memória; o que falta p/ recall (gap) + `path:line` |
| Q7 | **(SOTA)** Como rodar o harness **dentro do PostgreSQL real** (carga, índice, query) de forma reprodutível e medir do lado cliente? | techniques | `.claude/knowledge-base/references/pgvectorscale/tests/test_basic_operations.py`, `.claude/knowledge-base/references/pgvector/test/t/` | Ler o setup psycopg (connect, COPY/INSERT, CREATE INDEX, ORDER BY `<=>` LIMIT k) | Ler `test_basic_operations.py` + um `.pl` de recall | Esqueleto executável: conectar → carregar → indexar → consultar → comparar com exact → recall + tempo; `path:line` |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1 | Covered |
| Dependencies | Q2 | Covered |
| Tools | Q3 | Covered |
| Techniques | Q4, Q5, Q6, Q7 | Covered (4 — frontier R4, ≤ 5) |

**Coverage: 4/4 corners (100%)** · Total **7 questões** (6–14; ≤5/corner; técnicas=4 ≥2).

> **Frontier rigor** (`rules/discover-phd-rigor.md`): cada técnica é (R1) ancorada no SOTA (ANN-Benchmarks/ScaNN — o padrão de medição), (R2) ≥2 fontes (repo allowlist + paper/arquivo em refs), (R3) número de perf só com método ou `UNBENCHMARKED`.

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | path em `.claude/knowledge-base/references/{...}` existe | BLOCKED "path not found", segue |
| Per-question Fase A | ≥1 hotspot OU 3 retries | BLOCKED "Fase A exhausted" |
| Web source (Q2–Q5) | URL ∈ allowlist E retorna | fora allowlist/404 → BLOCKED "source unreachable" (R5/R6) |
| After answering Qx | seção tem ≥1 citação | re-itera (1 retry) |
| Técnica com perf | método+fonte OU `UNBENCHMARKED` | adiciona `UNBENCHMARKED` |
| Before promising complete | 4 corners populados | recusa promise |

## Acceptance Criteria

- [ ] Todas as questões respondidas OU BLOCKED com razão
- [ ] 4 corners populados no blueprint
- [ ] Toda citação aponta a path real em `.claude/knowledge-base/references/{...}`
- [ ] **Frontier rigor:** recall@k definido com fonte SOTA + ground-truth exato citado; protocolo latência/QPS ancorado
- [ ] ≥1 ADR no blueprint sintetiza o design do harness (linguagem, deps, dataset, protocolo)
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint salvo em `.claude/knowledge-base/discoveries/blueprints/vector-recall-benchmark-harness-blueprint.md`

## Global Definition of Done

- [ ] Fases completas (plan → edge-cases → plan-confidence → execute → confidence)
- [ ] Verdict final no header do blueprint
- [ ] Zero citações fabricadas
- [ ] Coverage Matrix 100%
- [ ] ADRs referenciam ≥1 regra: `rules/discover-phd-rigor.md`, `rules/testing.md` (pirâmide — o harness terá unit+integration), `public-copy.md` (claim só com benchmark), PRD D3 / ADR 0002 (o gate).
