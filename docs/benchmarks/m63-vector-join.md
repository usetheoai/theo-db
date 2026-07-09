# M63 — Vector JOIN via LATERAL-index-scan: join-recall + latência (3 braços + dedup e2e)

**Date:** 2026-07-09 · **Milestone:** M63 · **Metric:** cosine · **GT:** seqscan exato O(n·m) por-linha
**Harness:** `benchmarks/run_m63_vector_join.py` (reusa `theodb_bench.metrics`, espelha `run_m52_filtered_ann.py`) · **JSON:** `docs/benchmarks/m63-vector-join.json`
**ADR:** [`0022-m63-vector-join-lateral-not-node.md`](../adr/0022-m63-vector-join-lateral-not-node.md) (D1 LATERAL, não custom node; D2 helper rejeitado)

> **Veredito estrutural (o gate do DoD) — CUMPRIDO e PROVADO por `EXPLAIN` (`#[pg_test]`):** o similarity
> join `a CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` usa o **Índice** ANN no
> ramo interno — **não** é o nested-loop O(n·m). Recall preservado vs GT exato (per-row min + mean). O
> veredito de **latência** (T1 vs T3 pgvector) é medido pelo harness contra o container `theodb:m63` (numbers
> abaixo preenchidos a partir do `.json`); a paridade de latência **não** é o gate do DoD (que é "usa o
> índice, não O(n²)", estrutural).

---

## 1. O gate estrutural — `EXPLAIN` prova o Index Scan no inner do LATERAL (R1 — POSITIVO)

O `#[pg_test] vector_join_uses_index_scan` roda `EXPLAIN (COSTS OFF, VERBOSE)` sobre o LATERAL. O plano
real (GREEN na suíte `cargo pgrx test pg17 vector_join`):

```
Nested Loop
  ->  Seq Scan on pg_temp.vja                        ← lado externo `a` = o driver do LATERAL (correto)
  ->  Limit
        ->  Index Scan using vjb_idx on pg_temp.vjb  ← ramo INTERNO (`b`) É um Index Scan theodb_hnsw
              Order By: (vjb.emb <=> vja.emb)         ← ordenado pelo operador de distância (amcanorderbyop)
```

- O **ramo interno** (o lado `b`) é um **Index Scan ordenado** — não `Seq Scan` + `Sort` sobre o produto
  cruzado. O planner empurra o índice **dentro** do LATERAL, sem engine novo (blueprint [C1]/[B1]).
- O `Seq Scan on vja` é do lado **externo** (`a`), que é legitimamente o driver do LATERAL — não é o
  O(n·m). O forbidden shape seria `Seq Scan on vjb` (o lado interno), que o teste assere **ausente**.
- A forma **dedup** (`WHERE b.id <> a.id`) mantém o Index Scan (Q1 resolvido — assertado no mesmo teste).

**Isto é o achado central do M63 e é positivo:** o risco R1 do blueprint ("o planner pode não empurrar o
índice no LATERAL") **não se materializa** para o shape `ORDER BY <op> LIMIT k` — que é o shape do join.

## 2. Recall preservado vs GT exato O(n·m) (`#[pg_test] vector_join_recall_matches_exact_within_tol`)

Para cada linha externa `a_i`: `recall_i = |ANN_topk(a_i) ∩ EXACT_topk(a_i)| / k`. O teste assere o
**min** por-linha (não só a média — R2: uma média esconde uma linha com recall 0) e a média ≥ tolerância,
mais os edges k=1 (nearest-neighbour join == NN exato) e k≥|b| (retorna todo `b`, recall 1.0). GREEN sob
dados de cluster tight (tol 0.9). O threshold shape (`< τ`) casa a contagem exata de pares em τ∈{0,mid,large}
(`vector_join_threshold_correct`); τ negativo → conjunto vazio documentado (`vector_join_negative_threshold_returns_empty`).

## 3. Benchmark 3 braços — join-recall + latência (mean±std, ≥3 runs)

**Braços:** **T1** LATERAL sobre `theodb_hnsw` (o produto) · **T2** naive cross-join+sort O(n·m) (o
anti-objetivo; recall 1.0 por construção, o teto/piso a evitar) · **T3** LATERAL sobre pgvector hnsw
(o controle SOTA permissivo, disciplina M45/M52). Métrica primária = join-recall per-row (min + mean±std)
vs GT exato num subset tratável; latência p50/p95 como evidência de suporte.

Medido no droplet `theodb:m63` + pgvector (n_a=200, n_b=5000, dim=128, k=10, cosine, 3 runs, seed 42):

```
PGPORT=<port> python3 benchmarks/run_m63_vector_join.py --n-a 200 --n-b 5000 --dim 128 --k 10 --runs 3 \
  --out docs/benchmarks/m63-vector-join.json
```

| braço | join-recall min | join-recall mean±std | p50 (ms) | p95 (ms) |
|---|---|---|---|---|
| **T1 — LATERAL theodb_hnsw** | 0.80 | **0.9948** (±0.001 entre-runs) | **0.452** | 0.536 |
| T2 — naive cross-join+sort O(n·m) | 1.0 (exato) | 1.0 (exato) | 0.977 | 1.02 |
| T3 — pgvector hnsw (controle) | 1.0 | 1.0 | 0.42 | 0.46 |

**T1 (LATERAL-index) é 2.16× mais rápido que T2 (naive O(n·m))** (0.452 vs 0.977 ms) e em **paridade de
latência com o controle pgvector** (0.452 vs 0.42 ms), com recall preservado (mean 0.9948). O `min 0.80`
(algumas linhas com recall menor) é a variância honesta do HNSW — reportado, não escondido pela média.

**Nota estatística honesta (transparência, não spin):** (a) o `±0.001` é o desvio **entre as 3
médias-por-run**, NÃO a dispersão por-linha — o std por-linha real é ~0.024 (a cauda que produz o `min 0.80`);
o `mean±std` aqui mede a estabilidade da média entre runs, não a variância por-query. (b) A afirmação "a 2.16×
cresce com `n_b`" é uma **projeção do mecanismo** (T2 é O(n·m), T1 é O(log n) por linha), NÃO uma curva
medida — há **1 ponto** (n_b=5000); medir n_b∈{5k,10k,20k} é débito rastreado, não um claim fechado.
(c) O run 3 teve um spike de `load_avg` (0.24→0.86); os 3 runs são o piso mínimo — o gap 2.16× é robusto a
esse ruído (a p50 de T1 variou só 0.44→0.46), mas a box não estava perfeitamente idle no run 3.

## 4. Caso end-to-end — deduplicação / entity-resolution em SQL puro

kNN-self-join com duplicatas plantadas (`ε`-ruído sobre linhas existentes), recuperadas via
`… CROSS JOIN LATERAL (SELECT b.id, b.v <=> a.v AS d FROM t b WHERE b.id <> a.id ORDER BY b.v <=> a.v LIMIT 1) j WHERE j.d < τ`.
Reporta **precisão E recall de detecção de duplicata** (ambos, nunca um score misturado — R2). O
harness (`_dedup_arm`) usa o `theodb_hnsw`; a aritmética (`dedup_metrics`) é unit-testada
(normalização de par não-ordenado, false-positive baixa precisão, dup perdida baixo recall, found-vazio →
precisão indefinida sem crash).

Medido (20 duplicatas plantadas, ε-ruído, τ ajustado): **recall de detecção 1.0** (achou as 20/20
duplicatas plantadas), **precisão 0.115** (174 pares abaixo do τ — os vizinhos genuínos apertados também
caem sob o threshold; τ mais estrito sobe a precisão ao custo de recall). Reportado como par
precisão-E-recall separado (R2), nunca um score misturado. O ganho e2e é o **recall 1.0** — nenhuma
duplicata plantada escapou; a precisão é uma função do τ (o operador ajusta o trade-off).

## 5. Decisão do helper (D2) — REJEITADO (raw-LATERAL-only)

O helper `theodb.vector_join(...)` **não embarca** (ADR 0022 D2): o raw LATERAL já é o idioma first-class
index-served (rung 1 da parsimony-ladder — YAGNI); o SQL dinâmico do helper arriscaria o pushdown (R5) e
adicionaria um contrato SemVer para açúcar puro. O idioma LATERAL é a superfície documentada.

## 6. VEREDITO (DoD)

- **Estrutural (o gate) — CUMPRIDO:** o join usa o índice ANN no ramo interno do LATERAL (Index Scan
  ordenado, provado por `EXPLAIN` no `#[pg_test]`), **não** o nested-loop O(n·m). Recall preservado vs GT
  exato (per-row min + mean). Threshold + dedup corretos; caso negativo é contrato documentado.
- **Latência (não-gate) — MEDIDO, POSITIVO:** T1 (LATERAL-index) p50 **0.452 ms** bate T2 (naive O(n·m))
  0.977 ms em **2.16×**, e fica em **paridade com o controle pgvector** (0.42 ms). O braço-index não só
  cumpre a DoD estrutural (usa o índice) como vence o anti-objetivo em latência — não houve honest-negative
  a documentar aqui (public-copy.md §4: o número é o medido, 3 runs, no droplet com o controle pgvector).
- **Helper — REJEITADO** por parcimônia (ADR 0022 D2): zero novo código de produção; o LATERAL é a
  superfície first-class.
