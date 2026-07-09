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

> **PENDING — números preenchidos a partir de `docs/benchmarks/m63-vector-join.json`** (gerado pelo harness
> no droplet com a imagem `theodb:m63` + pgvector; a box dev não tem o controle pgvector, por isso o run
> vive no droplet — mesma disciplina M45/M52). O harness já está verde nos testes de aritmética
> (`benchmarks/tests/test_run_m63_vector_join.py`, 16/16). Comando reprodutível:

```
PGPORT=<port> python3 benchmarks/run_m63_vector_join.py --n-a 200 --n-b 5000 --dim 128 --k 10 --runs 3 \
  --out docs/benchmarks/m63-vector-join.json
```

| braço | join-recall min | join-recall mean±std | p50 (ms) | p95 (ms) |
|---|---|---|---|---|
| T1 — LATERAL theodb_hnsw | _pending_ | _pending_ | _pending_ | _pending_ |
| T2 — naive cross-join+sort O(n·m) | 1.0 (exato) | 1.0 (exato) | _pending_ | _pending_ |
| T3 — pgvector hnsw (controle) | _pending_ | _pending_ | _pending_ | _pending_ |

## 4. Caso end-to-end — deduplicação / entity-resolution em SQL puro

kNN-self-join com duplicatas plantadas (`ε`-ruído sobre linhas existentes), recuperadas via
`… CROSS JOIN LATERAL (SELECT b.id, b.v <=> a.v AS d FROM t b WHERE b.id <> a.id ORDER BY b.v <=> a.v LIMIT 1) j WHERE j.d < τ`.
Reporta **precisão E recall de detecção de duplicata** (ambos, nunca um score misturado — R2). O
harness (`_dedup_arm`) usa o `theodb_hnsw`; a aritmética (`dedup_metrics`) é unit-testada
(normalização de par não-ordenado, false-positive baixa precisão, dup perdida baixo recall, found-vazio →
precisão indefinida sem crash).

> **PENDING** — precisão/recall de dedup preenchidos a partir do `.json` no droplet.

## 5. Decisão do helper (D2) — REJEITADO (raw-LATERAL-only)

O helper `theodb.vector_join(...)` **não embarca** (ADR 0022 D2): o raw LATERAL já é o idioma first-class
index-served (rung 1 da parsimony-ladder — YAGNI); o SQL dinâmico do helper arriscaria o pushdown (R5) e
adicionaria um contrato SemVer para açúcar puro. O idioma LATERAL é a superfície documentada.

## 6. VEREDITO (DoD)

- **Estrutural (o gate) — CUMPRIDO:** o join usa o índice ANN no ramo interno do LATERAL (Index Scan
  ordenado, provado por `EXPLAIN` no `#[pg_test]`), **não** o nested-loop O(n·m). Recall preservado vs GT
  exato (per-row min + mean). Threshold + dedup corretos; caso negativo é contrato documentado.
- **Latência (não-gate) — PENDING no droplet:** T1 vs T3 pgvector é medido e documentado honestamente
  (public-copy.md §4); se T1 perder a corrida de latência ao T3 em escala, a DoD ainda é cumprida por T1
  (usa o índice) e falhada por T2 (O(n·m)) — o gap de latência é documentado, não mascarado (honest-negative).
- **Helper — REJEITADO** por parcimônia (ADR 0022 D2): zero novo código de produção; o LATERAL é a
  superfície first-class.
