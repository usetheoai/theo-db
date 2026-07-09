# ADR 0022 — M63 vector JOIN: LATERAL-index-scan, not a custom join node; helper rejected

**Status:** Accepted · **Data:** 2026-07-09 · **Milestone:** M63 · **Owner:** Eng
**Relacionado:** blueprint `.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md` (ADR-1),
plan `.claude/knowledge-base/plans/m63-vector-join-plan.md` (D1/D2), ADR `0002` (North Star),
`docs/benchmarks/m52-filtered-ann.md` (o `WHERE … ORDER BY` index-served que o LATERAL reusa por-linha),
`theodb_rs/src/am/mod.rs:78` (`amcanorderbyop = true`), Unbreakable Rule 9 (não reinventar).

## Contexto

O DoD do M63 (ROADMAP.md § M63) pede um *similarity join* que **usa o índice ANN** (não nested-loop
O(n·m)), planner-integrado, recall preservado, + um caso end-to-end de deduplicação em SQL puro. A
discovery (blueprint, ≥2 fontes primárias por claim) concluiu que o padrão
`a CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` **já é** esse join hoje —
sem engine novo — porque cada iteração do LATERAL reduz `b.emb <=> a.emb` ao top-k single-vector que o
`amcanorderbyop` do `theodb_hnsw` serve (o mesmo shape que o M52 provou para `WHERE … ORDER BY`).

## Decisão D1 — Adotar o LATERAL-index-scan; NÃO construir custom join executor node

O M63 entrega o vector-join via o LATERAL sobre o AM existente. Os deliverables de código são:
(1) os `#[pg_test]` de `EXPLAIN`/recall/threshold que **provam** o Index Scan e o recall preservado;
(2) o benchmark 3-braços (`benchmarks/run_m63_vector_join.py`); (3) o caso dedup end-to-end. **Nenhum
nó de engine.**

**Evidência empírica (o achado central, R1 — POSITIVO).** O `#[pg_test] vector_join_uses_index_scan`
roda `EXPLAIN (COSTS OFF, VERBOSE)` sobre o LATERAL e o plano real é:

```
Nested Loop
  ->  Seq Scan on pg_temp.vja                     ← lado externo `a` (o driver do LATERAL) — correto
  ->  Limit
        ->  Index Scan using vjb_idx on pg_temp.vjb   ← ramo INTERNO do LATERAL É um Index Scan
              Order By: (vjb.emb <=> vja.emb)          ← ordenado pelo operador de distância (amcanorderbyop)
```

O ramo interno (o lado `b` do join) é um **Index Scan ordenado** no índice `theodb_hnsw` — não um
`Seq Scan` + `Sort` sobre o produto cruzado. Ou seja: **o planner empurra o índice dentro do LATERAL,
sem código de engine novo.** (O plano imprime o NOME do índice `vjb_idx`, não o nome do AM; a identidade
`theodb_hnsw` é estrutural — um Index Scan ordenado servindo `emb <=> a.emb` só existe porque `mod.rs:78`
setou `amcanorderbyop = true`.)

**Alternativas consideradas:**
- **(A) Custom CustomScan/Join node que empurra o AM.** *Rejeitada.* PhD-level (hook de planner + path
  generation + custom scan state + cost model de join), duplica LATERAL + `amcanorderbyop`, e o maintainer
  do pgvector confirma que "would still need N separate index lookups" (blueprint [A1]) — nenhum ganho
  algorítmico, só complexidade acidental (viola Regra 9 + "Esforço ≠ Complexidade").
- **(B) Materializar produto cruzado + top-level `ORDER BY` (o naive #812).** *Rejeitada como produto* —
  é O(n·m), não usa o índice; é o braço **T2** que medimos **contra**, não um deliverable.

**Consequências:** vector-join first-class hoje; recall = o recall do próprio AM (herdado, preservado por
construção — provado por `vector_join_recall_matches_exact_within_tol`). Risco R1 (o planner pode não
empurrar o índice em certos formatos) resolvido empiricamente pelo gate `EXPLAIN` — e o resultado é
positivo para o shape `ORDER BY <op> LIMIT k`.

## Decisão D2 — REJEITAR o helper `theodb.vector_join(...)`; raw-LATERAL-only + documentação

**Decisão: o helper NÃO embarca.** O raw LATERAL já é o idioma first-class, planner-nativo,
parametrizável e provadamente index-served. Uma função-helper `theodb.vector_join(left_tbl regclass, …)`
que codegen-a o LATERAL via `format()`/SQL dinâmico:

1. **Falha o rung 1 da parsimony-ladder** (`.claude/rules/parsimony-ladder.md` — "isto precisa existir?").
   O LATERAL cru resolve o caso de uso sem código novo; o helper é açúcar puro (YAGNI).
2. **Arrisca o próprio pushdown que envolve (R5).** SQL dinâmico via `regclass`/`format()` pode
   defeat-ar a escolha de Index Scan que o LATERAL estático mantém — ou seja, um helper poderia entregar
   um caminho **mais lento** que o idioma que embrulha. Nunca embarcar um wrapper mais lento que a coisa
   embrulhada.
3. **Adicionaria um contrato público SemVer** (`theodb.*`, REVOKE-from-PUBLIC) para zero ganho de
   capacidade — só de digitação.

**Fallback adotado:** documentar o idioma LATERAL (top-k, threshold, dedup self-join) no relatório do
benchmark (`docs/benchmarks/m63-vector-join.md`). O caso negativo (τ < 0) é o contrato documentado
"empty set" no raw-SQL (provado por `vector_join_negative_threshold_returns_empty`), não um `ERROR`
tipado — o `ERROR` tipado só existiria no helper, que foi rejeitado.

**Consequência:** zero novo código de produção no M63. O M63 é validação + medição + documentação. A
DoD (join index-served, recall preservado, dedup em SQL) é cumprida pelo LATERAL.

## Débito honesto (fora do escopo M63)

- **Batch/amortização** — o LATERAL faz N buscas independentes, sem compartilhar trabalho entre linhas
  externas próximas (o gap de throughput do pgvector #645/[E2]). Um ANN-join amortizado (learned filter à
  la Xling [D1]) é semente futura (ADR-2 do blueprint), rastreada como débito, **só com evidência**
  (anti-sunk-cost) — não escopo atual.
