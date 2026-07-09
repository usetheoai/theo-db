# M63 — Vector JOIN: vetor como first-class no join relacional (blueprint)

**Cycle:** DISCOVER · **Milestone:** M63 · **Data:** 2026-07-09 · **Autor:** council/research (Profa. Laura Stein)
**Perfil de rigor:** `.claude/rules/discover-phd-rigor.md` (R0 busca web obrigatória; R1 SOTA-anchoring; R3 benchmark-ou-`UNBENCHMARKED`)
**Escopo M63 (ROADMAP.md:1011):** similarity join `a JOIN b ON a.emb <=> b.emb < τ` (ou top-k por linha de `a`) **usando o índice ANN**
(não nested-loop O(n·m)), planner-integrado, recall preservado. Caso end-to-end: deduplicação / entity-resolution em SQL puro.

> **TL;DR do veredito (honesto):** o padrão **`CROSS JOIN LATERAL (… ORDER BY b.emb <=> a.emb LIMIT k)`** já é o similarity-join
> planner-integrado que usa o AM vetorial — **HOJE, sem código de engine novo** — porque cada iteração do LATERAL é um top-k
> single-vector que o `amcanorderbyop` do `theodb_hnsw` (M26/M49) serve, exatamente como o M52 provou para o `WHERE … ORDER BY`.
> O que **falta** (o trabalho do M63) NÃO é um join node: é **validar+medir+documentar** (join-recall vs brute-force O(n·m) +
> latência) e, opcionalmente, uma **função-helper SQL** `theodb.vector_join(...)` que encapsula o LATERAL idiomático. Um custom
> join executor node é PhD-level e **rejeitado por Regra 9** — o LATERAL do Postgres já resolve. Ver ADR-1.

---

## Evidência web (R0) — ≥ 2 fontes primárias por claim

### Claim A — Em pgvector/Postgres HOJE, o join **column-vs-column no topo NÃO usa o índice** (planeja Nested Loop + Sort O(n·m))
- **[A1] pgvector issue #812 — "Can an index be used with a nested loop?"** (github.com/pgvector/pgvector/issues/812).
  O usuário roda `SELECT … FROM face CROSS JOIN search_embeddings ORDER BY face.embedding <=> search_embeddings.embedding LIMIT 10`.
  `EXPLAIN` → `Nested Loop (rows=911391760) → Seq Scan on face` + `Sort` sobre o produto cruzado. O índice **não** é usado.
  Autor do pgvector (**@ankane**): *"even if the Postgres planner gained the ability to use the index in the first query, it would
  still need to do 5 separate index lookups"* — i.e., o índice ANN é intrinsecamente **por-linha**, não por-join.
- **[A2] pgvector issue #713** (github.com/pgvector/pgvector/issues/713#issuecomment). @ankane: *"an index won't help this query
  (comparing all vectors in A with all vectors in B), which is why it's not being used … The Postgres planner doesn't seem to
  consider it (even with `enable_seqscan = off`)."* Confirma: o join **column-vs-column** não empurra o AM vetorial.

### Claim B — O índice ANN FIRA quando a comparação é **coluna-vs-um-único-vetor + `ORDER BY <op> LIMIT k`** (o corpo do LATERAL)
- **[B1] pgvector README § "Why isn't a query using an index?"** (github.com/pgvector/pgvector, README): *"The query needs to have
  an `ORDER BY` and `LIMIT`, and the `ORDER BY` must be the result of a distance operator (not an expression) in ascending order."*
  O corpo do LATERAL (`ORDER BY b.emb <=> a.emb LIMIT k`) satisfaz isso literalmente por-linha de `a`.
- **[B2] pgvector issue #703** — @ankane: *"the planner should use the index if you add a `LIMIT` to the query."* + **issue #713**
  (@masdeval mostra que `ORDER BY col <=> (SELECT single_vec) LIMIT 10` **usa** o índice; @ankane confirma a diferença: *"That query
  compares a single vector with a column"*). Dois fios independentes → o single-vector top-k é o que o AM serve.

### Claim C — `LATERAL` avalia o subquery **uma vez por linha externa** com os valores dessa linha (a semântica que torna o índice per-row)
- **[C1] PostgreSQL docs § 7.2.1.5 LATERAL Subqueries** (postgresql.org/docs/current/queries-table-expressions.html): *"for each row
  of the FROM item providing the cross-referenced column(s) … the LATERAL item is evaluated using that row's values … This is
  repeated for each row."* → o `b.emb <=> a.emb` do inner vira, a cada iteração, `b.emb <=> <constante>` = o caso B (index-served).
- **[C2] pgvector maintainer guidance (issue #645 "batch query for N vectors")** — o próprio pgvector recomenda o padrão
  `JOIN LATERAL (… ORDER BY … LIMIT 1)` como a forma de fazer N buscas numa query (o exemplo do #645 usa exatamente
  `JOIN LATERAL (SELECT … FROM t ORDER BY similarity DESC LIMIT 1) … ON true`). É o idioma consagrado no ecossistema.

### Claim D — O SOTA acadêmico de similarity/ANN-join confirma que o problema é **per-query top-k/threshold**, não um operador de join novo barato
- **[D1] Xling: A Learned Filter Framework for Accelerating High-Dimensional Approximate Similarity Join** (Wang, Pathak, Wang;
  arXiv:2402.13397, 2024). *"Similarity join finds all pairs of close points within a given distance threshold … usually not
  efficient on high-dimensional space due to the curse of dimensionality."* SOTA = filtro/índice por-ponto (MSBF/learned filter)
  para podar candidatos — **o join é uma sequência de buscas por-ponto**, não um operador relacional fechado. Valida a rota LATERAL.
- **[D2] Preference-driven Similarity Join** (Gao, Wang, Pei; arXiv:1706.04266, 2017) e a linhagem clássica **Gorder / iJoin /
  kNN-join** (busca arXiv `"knn join" OR "similarity join"`): kNN-join é definido como "para cada ponto de R, seus k vizinhos em S" —
  literalmente o top-k-por-linha-externa que o LATERAL executa. O ganho de escala vem de **poda por índice/filtro por-ponto**, o que
  o AM vetorial já provê.

### Claim E — Bancos vetoriais expõem "join" como **batch/bulk search + range search**, não um join relacional — o mesmo shape do LATERAL
- **[E1] Milvus — Bulk-vector search + Range search** (milvus-docs, `single-vector-search.md`): *"A bulk-vector search extends the
  single-vector search concept by allowing multiple query vectors to be searched in a single request … one [result set] for each
  query vector."* + *"Range search allows you to find vectors that lie within a specified distance range (`radius`/`range_filter`)."*
  → o "join" deles é **N buscas top-k** (batch) ou **range por-query** — exatamente o LATERAL top-k / LATERAL threshold do M63.
- **[E2] pgvector issue #645** (@CorentinvdBdO): *"The lack of batch search (100/1000 query vectors) is the sole reason we moved
  back to Qdrant."* → confirma, do lado da demanda, que o caso de uso "join vetorial" = batch/N-query search; e que o LATERAL do
  pgvector hoje é lento nesse caso (o gap que o M63 mede e a função-helper documenta). Qdrant expõe `search_batch` como primitiva.

> **Fontes que NÃO abriram / BLOCKED (Regra 3):** Semantic Scholar API (rate-limit, vazio) — contornado via arXiv API (D1/D2).
> Crunchy Data / Jonathan Katz posts abriram mas **não** tratam de vector-join (só de HNSW single-query) — não citados como
> evidência de join, apenas registrados como negativos. Nenhum claim deste blueprint depende de fonte não-aberta.

---

## Recomendação (ADR-1) — LATERAL-index-scan vs custom join node

### ADR-1: Adotar `CROSS JOIN LATERAL (… ORDER BY <op> LIMIT k)` como o similarity-join do M63; NÃO construir custom join node

**Status:** proposto (DISCOVER) · **Contexto:** DoD M63 pede "similarity join com uso do índice (não nested-loop O(n²)), planner
escolhe o AM vetorial, recall preservado".

**Decisão:** o M63 entrega o vector-join via o padrão **`FROM a CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb
LIMIT k) j`** (top-k por linha) e/ou **`… WHERE b.emb <=> a.emb < τ`** dentro do LATERAL (threshold / range). O trabalho de código é
(1) validar por `EXPLAIN` que o inner é `Index Scan` no `theodb_hnsw`/`theodb_ivfflat`; (2) o benchmark join-recall vs brute-force
O(n·m); (3) o caso end-to-end de deduplicação em SQL; (4) **opcional** — uma função-helper SQL `theodb.vector_join(left_tbl,
left_col, right_tbl, right_col, k, metric)` que gera/encapsula o LATERAL idiomático (pura ergonomia; zero engine novo).

**Alternativas consideradas:**
- **(A) Custom join executor node (CustomScan / novo `Join` que empurra o AM).** *Rejeitada.* É PhD-level (hook de planner + path
  generation + custom scan state + cost model de join), duplica o que o LATERAL + `amcanorderbyop` já fazem, e o próprio maintainer
  do pgvector diz que "would still need N separate index lookups" [A1] — i.e., **nenhum ganho algorítmico** sobre o LATERAL, só
  complexidade acidental. Viola Regra 9 (o Postgres já resolve) e "Esforço ≠ Complexidade" (CLAUDE.md).
- **(B) Materializar produto cruzado + top-level `ORDER BY` (o naive do #812).** *Rejeitada.* Não usa o índice, é O(n·m) Sort —
  literalmente o anti-objetivo do DoD ("não nested-loop O(n²)"). É o baseline **contra** o qual mediremos.
- **(C) `UNION ALL` de N single-vector queries (guidance do #59/#713).** *Considerada, subsumida.* Funciona e usa o índice, mas é
  SQL gerado/estático (N não-parametrizável em uma query) — o LATERAL é a forma **relacional, parametrizável e planner-nativa** da
  mesma coisa. `UNION ALL` fica documentado como fallback quando o LATERAL não empurrar o índice (ver Riscos R1).

**Consequências:** ganho = vector-join first-class **hoje**, recall = o recall do próprio AM (herdado, preservado por construção).
Custo/risco = o planner pode **não** escolher o Index Scan dentro do LATERAL em certos formatos (R1); a métrica de "join recall" precisa
de definição cuidadosa (R2). Diff mínimo (uma função SQL + testes + benchmark), anti-sunk-cost preservado.

**Reusa do M52 (Filtered ANN):** o M52 já provou (`docs/benchmarks/m52-filtered-ann.md`; pg_test `filtered_scan_preserves_recall_via_iterative`,
`iterative_scan_off_when_max_scan_tuples_zero`) que o `theodb_hnsw` serve `Index Scan` sob `WHERE … ORDER BY emb <=> $1 LIMIT k`.
O corpo do LATERAL É esse mesmo shape com `$1 = a.emb` — **o M63 é a extensão relacional do M52, não um mecanismo novo.**

---

## Coverage Corner 1 — Integration Tests

- **`EXPLAIN` prova o Index Scan no inner do LATERAL** (o gate estrutural): `EXPLAIN (ANALYZE, VERBOSE) SELECT … FROM a CROSS JOIN
  LATERAL (SELECT b.id FROM b ORDER BY b.emb <=> a.emb LIMIT 5) j` deve mostrar `Index Scan using … theodb_hnsw` no ramo interno,
  **não** `Seq Scan`+`Sort`. pg_test `vector_join_uses_index_scan` (âncora: M52 `filtered_scan_preserves_recall_via_iterative`).
- **Recall de join preservado vs brute-force exato**: para um conjunto pequeno (onde o O(n·m) exato é tratável), o multiset de pares
  `(a.id, b.id, dist)` do LATERAL-index == top-k exato por linha de `a` dentro da tolerância de recall do AM. pg_test
  `vector_join_recall_matches_exact_within_tol`.
- **Threshold/range join**: `… LATERAL (SELECT … WHERE b.emb <=> a.emb < τ ORDER BY … )` retorna exatamente os pares abaixo de τ
  (edge: τ=0 → só o self-match; τ grande → todos). Negative-case: τ negativo → erro tipado, não crash.
- **Coexistência**: os pg_tests M20–M22 / M45 / M52 permanecem verdes (a função-helper e o benchmark não tocam o scan hot-path).
- **Integração PG real**: rodar contra o container `theodb:m63` (mesma disciplina M52), não só `#[pg_test]` unitário.

## Coverage Corner 2 — Dependencies

- **Zero nova dependência de crate/engine.** O caminho é SQL puro sobre o AM existente (`theodb_hnsw`/`theodb_ivfflat`) + LATERAL
  nativo do Postgres 17. Parsimony-ladder rung 2/3 (o Postgres já provê LATERAL; o AM já provê `amcanorderbyop`).
- **Reusa interno:** `theodb_hnsw` (M26 `IndexAmRoutine`, M35 scan page-native), opclasses cosine/ip/l2 (M49), iterative scan (M52).
- **Licenças (D1):** nada novo entra; LATERAL é core PostgreSQL (PostgreSQL License). A função-helper opcional é `extension_sql!`
  (Apache-2.0 nosso). **Sem AGPL, sem crate externo** → gate de licença trivialmente satisfeito.
- **Harness de benchmark:** reusa `benchmarks/theodb_bench/` (recall/metrics) — mesma base do M32/M45/M52; sem nova infra.

## Coverage Corner 3 — Tools

- **`EXPLAIN (ANALYZE, VERBOSE, BUFFERS)`** — a ferramenta que prova o Index Scan por-linha + conta buffers lidos (evidência de que
  não é O(n·m)).
- **Harness `benchmarks/run_m63_vector_join.py`** (novo, espelha `run_m52_filtered_ann.py`): mede join-recall vs brute-force exato +
  p50/p95 latência do LATERAL-index vs o naive cross-join+sort, mean±std ≥3 runs, hardware citado (`analysis-golden-rule`).
- **`theodb_bench.recall` / GT exato** — o oráculo de recall (seqscan exato por-linha), já usado no M32/M45/M52.
- **`#[pg_test]` (cargo pgrx test)** — testes de correção do LATERAL/threshold/negative-cases dentro do engine.
- **`psql` + dataset SIFT/GloVe** (subset tratável para o brute-force exato do GT; escala grande só para latência, não para GT O(n·m)).

## Coverage Corner 4 — Techniques (SOTA-anchoring — R1)

- **SOTA do campo:** kNN-join (Gorder/iJoin) e ANN-join (Xling, arXiv:2402.13397 [D1]) definem o join como **"para cada ponto de R,
  top-k/threshold em S"**, acelerado por **poda por-ponto via índice/filtro** — não por um operador de join fechado. TheoDB fecha o
  gap com o AM vetorial (M26/M35) servindo cada top-k; o "join" é a iteração relacional (LATERAL). **Gap vs SOTA acadêmico:** Xling
  usa um *learned filter* (MSBF) para podar pares antes da distância — otimização de 2ª ordem, fora do escopo M63 (semente futura).
- **SOTA de produto (o alvo AlloyDB/vector-DBs):** Milvus [E1] expõe **bulk/batch search** (N-query) + **range search** (`radius`);
  Qdrant expõe `search_batch`. O padrão LATERAL do TheoDB é o **equivalente relacional-nativo** dessas primitivas — e é *mais*
  composível (junta com `WHERE`, agregação, columnar do M61/M62) do que uma API batch isolada. **Posicionamento:** "vetor no
  modelo relacional" > "batch API num silo vetorial".
- **A técnica parcimoniosa (o COMO):** LATERAL + `amcanorderbyop` (rung 2/3 da parsimony-ladder). O custom join node (rung 6) é
  complexidade essencial *só se* o LATERAL provar-se insuficiente na medição — o measurement-first decide (anti-sunk-cost).
- **Dedup / entity-resolution (o caso end-to-end):** `SELECT a.id, j.id FROM t a CROSS JOIN LATERAL (SELECT b.id FROM t b WHERE
  b.id <> a.id ORDER BY b.emb <=> a.emb LIMIT 1) j WHERE (a.emb <=> j.emb) < τ` — cada linha acha seu vizinho mais próximo via
  índice; o `< τ` filtra duplicatas. É o kNN-self-join clássico, planner-integrado.

---

## Design do benchmark (R3 — measurement-first, join-recall vs brute-force O(n·m) + latência)

**Artefato-alvo:** `docs/benchmarks/m63-vector-join.{md,json}` · **Harness:** `benchmarks/run_m63_vector_join.py`.

1. **Datasets:** subset SIFT (ex. `a` = 1k queries, `b` = 25k–200k base) — pequeno o suficiente para o **GT exato O(n·m)** ser
   computável (o brute-force `a × b` com distância exata + top-k por linha de `a`). Escala maior (1M `b`) **só** para o eixo de
   latência (onde o brute-force é intratável — aí compara-se LATERAL-index vs o naive-sort só até onde o naive termina).
2. **Métrica de join-recall (definição explícita — R2 do risco):** para cada linha `a_i`, `recall_i = |ANN_topk(a_i) ∩ EXACT_topk(a_i)| / k`;
   **join-recall = média sobre as linhas de `a`** (mean±std). Isto herda a semântica ANN-Benchmarks por-query, agregada sobre o lado
   externo do join. Gate DoD: join-recall(LATERAL-index) ≥ paridade com o recall single-query do AM (M45) — o join **não** deve
   degradar o recall do índice.
3. **Braços comparados (3 runs, mean±std, hardware citado):**
   - **T1 — LATERAL-index** (`theodb_hnsw`, o produto): p50/p95 + join-recall + buffers.
   - **T2 — naive cross-join + top-level sort** (o O(n·m), anti-objetivo): latência (prova o ganho do T1) — recall = 1.0 por
     construção (é exato), serve de teto de recall e piso de latência a evitar.
   - **T3 (controle) — pgvector `hnsw` LATERAL** (mesma query, container pgvector): paridade/veredito honesto vs o SOTA permissivo,
     como M45/M52.
4. **Veredito honesto (`public-copy.md`):** por eixo (join-recall, p50, buffers) — PARIDADE / SUPERIOR / GAP, sem cherry-pick. Sem
   claim de performance sem o `.json` reproduzível. Se o planner não empurrar o índice em algum formato (R1), marca-se
   `UNBENCHMARKED`/BLOCKED nesse braço, não se mascara.
5. **Caso end-to-end medido:** o self-join de deduplicação (acima) rodado sobre um corpus com duplicatas plantadas → precisão/recall
   de *detecção de duplicata* (não só de vizinho), provando o "SQL puro" do DoD.

---

## Riscos honestos (Regra 3)

- **R1 — O planner pode NÃO escolher o Index Scan dentro do LATERAL em certos formatos.** Evidência: o naive top-level [A1] já
  falha; e o próprio pgvector diz que o planner "doesn't seem to consider it" em column-vs-column [A2]. **Dentro** do LATERAL o
  cross-reference vira constante-por-linha [C1] (caso B, index-served), mas formatos como `WHERE b.emb <=> a.emb < τ` **sem**
  `ORDER BY … LIMIT` podem não disparar o pushdown (o AM exige `ORDER BY <op> LIMIT` [B1]). **Mitigação:** o benchmark T1 mede
  com `EXPLAIN`; onde o índice não firar, documenta-se o fallback (`ORDER BY … LIMIT` amplo + filtro `< τ` no outer, ou `UNION ALL`
  [alt C]) e marca-se honestamente. Este é o maior risco técnico do M63.
- **R2 — A métrica de "join recall" é sutil e fácil de reportar mal.** Um join-recall médio pode esconder linhas com recall 0. O
  M52 já cometeu (e corrigiu, Rule 3) erro de reportar recall médio como conclusivo sob variância alta. **Mitigação:** reportar
  mean±std por-linha + o pior-caso (min recall_i), GT exato, seed fixo; recall é o eixo determinístico (independente de carga da box).
- **R3 — Latência do LATERAL em batch grande pode ser ruim vs vector-DBs (o gap do #645/[E2]).** N buscas sequenciais = N× o custo
  single-query; o M52 já mediu o iterative scan ~3× o pgvector no caso seletivo. **Mitigação/honestidade:** o M63 mede e documenta
  o custo real; NÃO promete batch-superior sem número. Otimizações (resume-from-discarded, batch-amortização) são follow-up
  rastreado, não escopo M63. O valor do M63 é **composabilidade relacional**, não throughput de batch cru.
- **R4 (menor) — box contendida polui QPS** (lição M46/M52). Mitigação: median + back-to-back + buffers determinísticos; recall é o gate.

---

## Posicionamento honesto — o LATERAL-index-scan já é "vector join first-class"? O que falta?

**Sim, funcionalmente.** O LATERAL top-k/threshold que usa o `amcanorderbyop` do `theodb_hnsw` **É** um similarity-join
planner-integrado que respeita o índice e preserva o recall — é o mesmo mecanismo que o M52 já provou para o `WHERE … ORDER BY`,
estendido ao lado externo de um join. Não há operador relacional novo a inventar (Regra 9): o Postgres já fornece a peça (LATERAL),
o TheoDB já fornece a outra (o AM ordenável). **O que o M63 adiciona não é engine — é evidência + ergonomia:** (1) a prova por
`EXPLAIN`+benchmark de que o índice fira e o join-recall se preserva; (2) o caso end-to-end de dedup em SQL; (3) opcionalmente a
função-helper `theodb.vector_join` que esconde o idioma LATERAL.

**O que genuinamente FALTA para ser "first-class de verdade" (honesto, fora do M63):** (a) o planner **não** empurra o índice num
join column-vs-column de topo — para isso seria preciso o custom join node (rejeitado por Regra 9, sem ganho algorítmico [A1]);
(b) **batch/amortização** — o LATERAL faz N buscas independentes, sem compartilhar trabalho entre linhas externas próximas (o que
um verdadeiro ANN-join node faria, à la Xling [D1]); é o gap de throughput que motivou usuários a irem pro Qdrant [E2]. Nenhum dos
dois é bloqueante para o DoD do M63 (que pede "usa o índice, não O(n²)"), mas ambos são a fronteira honesta do "vector-join SOTA".

**Veredito de trajetória:** M63 é **parcimonioso e entregável hoje** (SQL + testes + benchmark + helper opcional), com risco
concentrado em R1 (o planner no LATERAL) — que o measurement-first resolve empiricamente. Nenhum custom executor node.

---

## ADRs

- **ADR-1 (acima):** LATERAL-index-scan como o vector-join do M63; custom join node rejeitado (Regra 9, [A1]). Alternativas B/C
  documentadas com razão de rejeição/subsunção.
- **ADR-2 (semente futura, fora do M63):** se o benchmark R3 mostrar que o batch-join é gargalo competitivo real, avaliar um
  ANN-join amortizado (learned filter à la Xling [D1] ou shared-traversal) — mas só com evidência (anti-sunk-cost). Registrado como
  débito honesto, não escopo atual.

## Cross-references
- Extende: `docs/benchmarks/m52-filtered-ann.md` (o `WHERE … ORDER BY` index-served que o LATERAL reusa por-linha).
- AM: `theodb_rs/src/am/mod.rs:78` (`amcanorderbyop=true`), `theodb_rs/src/am/scan.rs` (heap top-k + iterative M52), M49 opclasses.
- Perfil de rigor: `.claude/rules/discover-phd-rigor.md` (R0/R1/R3). Mandato: `CLAUDE.md` (Regra 9; Esforço≠Complexidade; North Star).
- Fontes web: pgvector issues #812/#713/#703/#645 + README; PostgreSQL docs §7.2.1.5; arXiv:2402.13397 (Xling), arXiv:1706.04266;
  Milvus bulk/range search docs.
