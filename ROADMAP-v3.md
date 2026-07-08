# TheoDB — Roadmap v3 (Amplitude de produto: HTAP + vector-relational + AI-native + operabilidade)

> **DRAFT para revisão** (2026-07-08). Sucessor estratégico do roadmap v2 (own-code Rust, M17–M60 — pilar
> vetorial entregue em paridade com pgvector). Origem: sign-off do owner (2026-07-08) para **amplitude de
> produto** — diversificar os pilares em vez de aprofundar o vetorial (retorno decrescente: M60/IVF são esforço
> ALTO; o valor diferenciador está nos outros pilares).
>
> Herda TODAS as travas do v2: engine PostgreSQL mantido (wire-compat), measurement-first (ADR 0002 — nada é
> "concluído" sem paridade/benchmark medido), licenças permissivas (D1 — Apache/MIT/BSD/PostgreSQL), Regra 9
> (não reinventar; adotar peça permissiva madura quando não há own-code que resolva), honestidade (Regra 3/5).

## Contexto (o que o v2 nos deixou, medido)

- **Vetorial:** `theodb_hnsw` em **paridade com pgvector** (SIFT1M real, M45); ~25× atrás do ScaNN no QPS a
  recall alto (M33 — o mesmo gap do pgvector). Dois honest-negatives (SBQ/M57, AH/M59) isolaram que o 25× exige
  um **carrier IVF batch-scan** (esforço ALTO, deferido). Teto de recall do grafo ~0.974 (M60, aberto, diferido).
- **AI-surface:** `ai.*` generativas (M18), NL→SQL + híbrida (M19), vectorizer (M54), summarize (M37) — entregues.
- **Columnar/BM25:** M30/ADR-0013 decidiu **MANTER** `pg_mooncake`/`pg_duckdb` (MIT, columnar ~14× a 5M
  medido) e `pg_textsearch` (BM25 nDCG 0.95) como **exceções permissivas** (Regra 9) — mas **ainda não
  embarcados**. O caminho de adoção é milestone futura (agora: v3).
- **Filtered ANN:** M52 entregou `WHERE … ORDER BY embedding <=> $1` planner-integrado com recall preservado.

## Estratégia do v3

1. **Amplitude sobre profundidade** — o pilar vetorial está bom (paridade pgvector); o v3 diversifica para os
   pilares onde TheoDB diferencia vs uma "colagem de extensões": HTAP unificado, vetor-first-class no relacional,
   AI-dentro-do-banco own-code, e operabilidade que "roda-se sozinho bem".
2. **Adotar quando own-code não paga (Regra 9):** columnar/HTAP entra pela **adoção** da peça permissiva já
   medida (`pg_mooncake`/`pg_duckdb`), NÃO por reescrita (PhD-level/anos — fora de escopo, ADR-0013). AI-surface
   e o resto são **own-code Rust** (o mandato).
3. **Measurement-first (ADR 0002 preservado):** cada milestone carrega um benchmark/gate reproduzível; nada é
   "concluído" sem evidência. Honest-negative é resultado válido.
4. **Incremental com paridade:** cada feature usa os testes atuais como prova; o produto permanece funcional a
   cada milestone.

## Pilar A — Columnar / HTAP (ADOTAR a peça permissiva)

### M61 — [ ] Embarcar o columnar/HTAP (pg_mooncake/pg_duckdb) na distribuição — o gate de adoção do M30

**Objective:** o M30/ADR-0013 decidiu MANTER o columnar permissivo (medido ~14× a 5M) mas **não o embarcou**.
Esta milestone faz a adoção: buildar a peça no PG17 (ou bump PG18), smoke end-to-end, e o gate de licença/CVE.

**Definition of done:**
- [ ] `pg_mooncake` (ou `pg_duckdb`, o que passar no gate) buildado na imagem do TheoDB; `CREATE EXTENSION` +
  smoke (criar columnstore, query analítica) verde em CI.
- [ ] Gate de licença (D1 — MIT ✓) + `/deps-audit` (CVE) da peça e suas transitivas.
- [ ] Benchmark de adoção reproduzível: columnstore vs row-store no MESMO dataset/box (confirma o ~14× do M30 na
  imagem embarcada) → `docs/benchmarks/m61-columnar-adoption.{md,json}`.
- [ ] Honestidade (Regra 9): documentar que columnar é **exceção permissiva adotada**, não own-code.

**Dependencies:** M30 (decisão KEEP + evidência). **Risco (MÉDIO):** compat de build PG17/18; peso da imagem.

### M62 — [ ] Superfície HTAP unificada — transacional + analítico na mesma tabela

**Objective:** com o columnar embarcado, entregar a experiência HTAP real (o pilar-chave do AlloyDB): a mesma
tabela serve OLTP (row) e OLAP (column) sem ETL manual, com roteamento por tipo de query.

**Definition of done:**
- [ ] Um fluxo declarativo: tabela row-store transacional + réplica/coluna analítica sincronizada (via a peça
  columnar), documentado como o "HTAP do TheoDB".
- [ ] Benchmark HTAP: carga mista (INSERTs OLTP + agregações OLAP concorrentes) medindo latência de ambos →
  `docs/benchmarks/m62-htap.{md,json}`.
- [ ] Veredito honesto vs AlloyDB HTAP (in-memory columnar): nosso é lakehouse/columnar-adotado — **aposta
  diferente** (D2), declarada, não cópia.

**Dependencies:** M61. **Risco (MÉDIO-ALTO):** sincronização row↔column; consistência sob carga mista.

## Pilar B — Vector-relational unificado

### M63 — [ ] Vector JOIN — vetor como first-class no join relacional

**Objective:** hoje o vetor é first-class no `ORDER BY` (M52). Faltam os JOINs vetoriais: `a JOIN b ON a.emb <=>
b.emb < τ` (similarity join) planner-integrado, o que torna o vetor parte do modelo relacional, não um silo.

**Definition of done:**
- [ ] Similarity join suportado com uso do índice (não nested-loop O(n²)) — o planner escolhe o AM vetorial no
  join; recall preservado.
- [ ] TDD + benchmark de recall/latência do join vs o baseline seqscan → `docs/benchmarks/m63-vector-join.{md,json}`.
- [ ] Caso de uso end-to-end: deduplicação/entity-resolution por similaridade em SQL puro.

**Dependencies:** M52 (filtered ANN), M35 (scan estruturado). **Risco (ALTO):** integração no planner de join.

### M64 — [ ] RAG-sobre-SQL unificado — a query única (relacional + vetor + analítico)

**Objective:** o "one query" story: combinar filtro relacional + retrieval vetorial + (opcional) agregação
analítica columnar numa query só — o RAG que não sai do banco.

**Definition of done:**
- [ ] Uma query de referência: `WHERE <filtro relacional> ORDER BY <vetor> LIMIT k` + join com uma agregação
  columnar, tudo planner-integrado, medida de recall + latência.
- [ ] Documentação do padrão RAG-nativo (retrieval + rerank + contexto) em SQL → guia + benchmark.
- [ ] Veredito honesto vs fazer o mesmo com pgvector + app-layer (o que ganhamos por ser unificado).

**Dependencies:** M63, M61 (para a leg analítica), M53 (híbrida). **Risco (MÉDIO).**

## Pilar C — AI-native / RAG lifecycle (own-code)

### M65 — [ ] Reranking own-code (`ai.rerank`) — qualidade de retrieval de 2ª ordem

**Objective:** o RAG SOTA rerankeia os top-k do retrieval com um cross-encoder. Falta a superfície `ai.rerank`
(own-code Rust + HTTP client mínimo ao modelo, como o resto do `ai.*`), fechando o lifecycle retrieval→rerank.

**Definition of done:**
- [ ] `ai.rerank(query, docs[])` própria (Rust), retornando scores; integrável com a híbrida (M53) e o vector
  join (M63).
- [ ] Qualidade medida: nDCG@10 / MRR em BEIR real com vs sem rerank → `docs/benchmarks/m65-rerank.{md,json}`
  (o gate: rerank melhora o nDCG mensuravelmente).
- [ ] Honestidade: se o rerank não melhorar o nDCG no dataset, honest-negative + decisão.

**Dependencies:** M53 (híbrida/BEIR harness), M18 (ai.* + HTTP client). **Risco (MÉDIO).**

### M66 — [ ] Estratégias de chunking declarativas no vectorizer

**Objective:** o vectorizer (M54) auto-embeda, mas o chunking (como partir o texto) domina a qualidade do RAG.
Faltam estratégias declarativas (fixed/sentence/semantic/overlap) com medida de impacto no recall.

**Definition of done:**
- [ ] Chunking configurável no vectorizer (`WITH (chunk_strategy=…, chunk_size=…, overlap=…)`), own-code.
- [ ] Benchmark: recall de RAG por estratégia num corpus real → `docs/benchmarks/m66-chunking.{md,json}` (qual
  estratégia ganha, medido).
- [ ] Edge/negative: documentos degenerados (vazio, gigante, 1 token) → typed error/handling.

**Dependencies:** M54 (vectorizer). **Risco (BAIXO-MÉDIO).**

## Pilar D — Auto-tuning / operabilidade

### M67 — [ ] Índices vetoriais auto-tunados — ef/probes por workload

**Objective:** hoje `ef_search`/`probes` são knobs manuais. Um banco maduro auto-ajusta pela workload (o P7 do
North Star). Own-code: observar o padrão de queries e sugerir/ajustar o knob para o alvo recall×latência.

**Definition of done:**
- [ ] Coletor de estatística de scan (recall estimado, pages read, latência) por índice — own-code.
- [ ] Auto-tune (ou recomendação) do `ef_search`/`probes` para um alvo de recall declarado; medida de que
  converge → `docs/benchmarks/m67-autotune.{md,json}`.
- [ ] Honest `amcostestimate` refinado com a estatística real (fecha o gap do M48/cost).

**Dependencies:** M35 (scan estruturado), M34 (probes/lists). **Risco (MÉDIO).**

### M68 — [ ] Observabilidade do query vetorial — EXPLAIN + métricas

**Objective:** operabilidade: hoje o scan vetorial é opaco. Expor `EXPLAIN (ANALYZE)` com pages-read/recall-est,
métricas runtime (o wiring-triad de observabilidade), para o operador ver e diagnosticar em produção.

**Definition of done:**
- [ ] `EXPLAIN` do scan vetorial mostra: índice usado, ef/probes efetivo, pages read, candidatos vistos.
- [ ] Métricas runtime (counter/histogram) do scan vetorial expostas (o pilar (c) do wiring-triad).
- [ ] Doc de operação: como diagnosticar recall baixo / latência alta em produção.

**Dependencies:** M67 (o coletor de estatística). **Risco (BAIXO).**

## Item vetorial diferido (não prioritário no v3)

### M60 — [ ] Qualidade de recall do HNSW próprio (~2pt vs pgvector) — DIFERIDO

Mantido aberto do v2. Só vira prioritário se decidirmos submeter benchmark público (recall≥0.99 é o ponto de
operação) OU perseguir o carrier IVF (o 25× do ScaNN). Fora do foco de amplitude do v3.

## Sequência e paralelismo

```
Pilar A (HTAP):    M61 (adotar columnar) ──▶ M62 (HTAP surface)
                        │
Pilar B (vec-rel):      └──▶ M63 (vector join) ──▶ M64 (RAG-sobre-SQL, usa A+B)
Pilar C (AI):      M65 (rerank) ──▶ M66 (chunking)   [independente de A/B]
Pilar D (ops):     M67 (auto-tune) ──▶ M68 (observabilidade)   [independente]
```

- M61 é a raiz do pilar HTAP; M63 a raiz do vector-relational; ambos convergem em M64 (a query unificada).
- Pilares C e D são **independentes** — paralelizáveis com A/B.
- **Ordem sugerida de valor:** M61 (HTAP destrava o pilar mais visível) → M65 (rerank, ganho de qualidade barato)
  → M63/M64 (o diferenciador vec-relational) → M67/M68 (maturidade) → M62/M66 (aprofundamento).

## Fora de escopo do v3 (honesto)

- **Reescrever columnar/BM25 próprios** — Regra 9, ADR-0013 (adotamos a peça permissiva).
- **O carrier IVF / o 25× do ScaNN** — esforço ALTO do pilar vetorial, diferido (não é amplitude).
- **Control-plane / deploy / HA** — fora do escopo do repositório (é o banco, não a plataforma).
- **Reescrever engine/HTTP/serde/crypto** — Regra 9.

## Referências SOTA (a popular no discover de cada milestone, regra R0)

Cada milestone roda `/discover` com a **regra R0** (busca web obrigatória: papers/OSS/blogs via WebSearch/WebFetch,
citados). Sementes por pilar: HTAP (DuckDB, pg_mooncake, AlloyDB HTAP), vector-join (pgvector, DiskANN filtered),
rerank (BGE/cross-encoders, BEIR), auto-tune (pg auto-tuning, ScaNN tuning), observability (pg_stat, EXPLAIN).
