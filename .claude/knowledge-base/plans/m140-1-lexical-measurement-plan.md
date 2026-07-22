---
slug: m140-1-lexical-measurement
milestone_id: M140.1
created_at: 2026-07-22
goal: Decidir com número e teste pareado se a BM25 own-engine bate ts_rank_cd e pg_textsearch, e fechar o ADR de storage lexical.
---

# Plan: M140.1 — Medição + decisão de arquitetura de storage lexical

> **Version 1.0** — M140.1 é o **gate de rigor** do M140. Ele NÃO constrói produto: constrói um artefato de
> benchmark reproduzível que responde, com número e teste pareado, duas perguntas antes de qualquer engenharia
> cara: (a) a BM25 own-engine (Tantivy MIT, o motor do spike M139) bate o baseline `ts_rank_cd` e o
> `pg_textsearch` num corpus lexical? e (b) qual storage a engine deve usar — heap buffer-then-flush (o achado
> medido do M139: MVCC/WAL/crash de graça) ou index AM custom (o DoD original, à la ParadeDB)? Se a BM25 não
> bater o baseline, o M140 **para aqui**, barato. Se bater, o ADR de storage destrava M140.2–M140.4.

## Goal

> Enable a decisão do M140 a ser tomada por evidência medida, produzindo um artefato reproduzível em
> `docs/benchmarks/m140-1-lexical-measurement.md` que compara BM25(Tantivy) vs `ts_rank_cd` vs `pg_textsearch`
> no mesmo corpus (log-proxy known-item + BEIR) com teste pareado, measured by o gate offline
> `benchmarks/theodb_bench/test_m140_1_decision.py` passar (verde) com um veredito `flip`/`no-flip` derivado de
> `decide()` sobre números reais em `docs/benchmarks/m140-1-data/`.

## Context

O M139 deu **GO** ao spike lexical (`docs/adr/0051`): o Tantivy MIT pode viver no PG via buffer-then-flush sobre
heap, herdando MVCC/WAL/crash, com índice 2,8× menor que `pg_textsearch`. Mas o M139 é um spike de **viabilidade**
— não decidiu se vale a pena construir a engine (a qualidade de retrieval em corpus lexical) nem cravou o storage
final. O M138 (`docs/benchmarks/m138-bm25-fusion.md`) já mediu, na droplet, que a **fusão** híbrida com BM25 não
bate `ts_rank_cd` (RRF lava a força da perna); mas mediu também que a **perna BM25 isolada** vence `ts_rank_cd`
isolada em NFCorpus lexical-heavy (0,325 vs 0,206). O consumidor real do M140 — theo-lens, busca lexical-**pura**
em traces (`trace-read-repository.ts:365`, hoje `ts_rank`) — usa a perna isolada, não a fusão. **Este milestone
mede exatamente o eixo que decide o M140: retrieval lexical isolado, num corpus lexical, com a engine own-code.**

O corpus de traces de produção do theo-lens **não existe no repositório** (scout exaustivo: só fixtures demo de
2–6 spans e o seeder travel-agent). Decisão do owner (2026-07-22): usar **corpus público de logs (LogHub)** como
proxy lexical trace-like, com o caveat de fidelidade de domínio declarado — a validação em traces reais de
produção é o **boundary do M140.4 (consumidor theo-lens) / M141 (dogfood)**, não deste milestone.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `benchmarks/theodb_bench/metrics.py` | 67 | `1c2e095` (2026-06-28) | nDCG@k + recall@n (BEIR primary), exigem qrels | `ndcg_at_k`/`recall_at_n` assinatura + semântica byte-idênticas (M138 depende) |
| `benchmarks/theodb_bench/significance.py` | 93 | `45d6294` (2026-07-20) | teste pareado de permutação (M123/M138) | `decide()`/permutation API estável — reusar, não alterar |
| `benchmarks/theodb_bench/beir.py` | 172 | `61f9e16` (2026-07-07) | loader BEIR (scifact/nfcorpus) com qrels | loader estável; só consumir |
| `benchmarks/theodb_bench/db.py` | 286 | `148a045` (2026-07-21) | conexão psycopg2 + helpers PG do harness | reusar conexão; não regredir M138 |
| `benchmarks/requirements.txt` | 8 | — | deps dev-only do harness (D1 não se aplica — dev) | adicionar só `tantivy` (MIT) — rung 4 parsimony |
| `benchmarks/theodb_bench/knownitem.py` (NEW) | 0 | — | (novo) gerador de query known-item + MRR/Success@1 | — |
| `benchmarks/theodb_bench/logcorpus.py` (NEW) | 0 | — | (novo) loader do corpus público LogHub → docs | — |
| `benchmarks/theodb_bench/lexical_engines.py` (NEW) | 0 | — | (novo) 3 retrievers: TantivyBM25 / PgTsRank / PgTextsearchBM25 | — |
| `benchmarks/run_m140_1_lexical.py` (NEW) | 0 | — | (novo) runner orquestrador → JSON em docs/benchmarks/m140-1-data/ | — |
| `benchmarks/theodb_bench/test_m140_1_decision.py` (NEW) | 0 | — | (novo) gate offline: decide() sobre os JSON reais | — |
| `benchmarks/theodb_bench/test_knownitem.py` (NEW) | 0 | — | (novo) testes unitários de known-item + MRR | — |
| `docs/benchmarks/m140-1-lexical-measurement.md` (NEW) | 0 | — | (novo) o artefato de veredito | — |
| `docs/adr/0052-m140-1-lexical-storage-decision.md` (NEW) | 0 | — | (novo) ADR heap vs index AM | — |

### Current callers / dependents

- **Symbol:** `ndcg_at_k()` / `recall_at_n()` em `benchmarks/theodb_bench/metrics.py`
  - **Callers (produção do harness):** `benchmarks/run_m138_bm25_fusion.py`, `benchmarks/run_m53_hybrid_beir.py`
  - **Callers (tests):** `benchmarks/theodb_bench/test_m138_decision.py`
  - **External:** não — harness dev-only, não é API pública shipada.
- **Symbol:** `decide()` / permutation em `benchmarks/theodb_bench/significance.py`
  - **Callers:** `run_m138_bm25_fusion.py`, `test_significance.py`
  - **Nota:** M140.1 **reusa** sem modificar (Rule 9). Se `decide()` não expõe exatamente o que preciso, o
    consumo é por composição (chamar), nunca por edição.

### Domain glossary

- **known-item retrieval** — tarefa IR (TREC named-page finding) onde a query busca **um** doc conhecido; o
  relevante é aquele doc. Métrica: MRR/Success@1/Recall@10. Não precisa de qrels humanos — o "gold" é o próprio doc.
- **MRR@k** — Mean Reciprocal Rank: média de 1/rank do doc-alvo no top-k (0 se fora do top-k).
- **Success@1** — fração de queries cujo doc-alvo ficou em rank 1.
- **ts_rank_cd** — função de ranking FTS do PostgreSQL core (cover-density); o baseline shipado do theo-lens.
- **BM25** — Okapi BM25, o ranking do Tantivy e do `pg_textsearch`; a own-engine do M139.
- **LogHub** — coleção pública de datasets de logs de sistema (HDFS, BGL, …), permissiva, usada em pesquisa de
  análise de logs; aqui o proxy lexical trace-like.
- **buffer-then-flush** — arquitetura do M139: Tantivy escreve num `MemStore` em memória (pgrx-free, thread-safe)
  e o flush para o heap PG (`theodb.lexical_files`) ocorre na main thread — MVCC/WAL/TOAST herdados do heap.

### Architecture boundaries affected

Nenhum boundary de produção (`rules/architecture.md`) é cruzado: **todo o trabalho vive em `benchmarks/`**, que é
tooling dev-only fora da distribuição (D1 não se aplica — `requirements.txt` já declara deps LGPL/BSD dev-only). O
ADR de storage (`0052`) **propõe** uma decisão de arquitetura de produção para o M140.3 consumir, mas M140.1 não
altera `theodb_rs/` nem o build shipado.

## Prior Art & Related Work

- **Internal blueprint / ADR:** `docs/adr/0051-m139-tantivy-pg-page-directory-design.md` — o veredito GO do spike
  e a arquitetura buffer-then-flush (a evidência medida de storage que o ADR-0052 consome).
- **Internal benchmark:** `docs/benchmarks/m138-bm25-fusion.md` — o método (BEIR + significância pareada) e os
  números da perna BM25 isolada vs `ts_rank_cd` que M140.1 confirma no eixo lexical-puro.
- **Harness reusado (Rule 9):** `benchmarks/theodb_bench/{metrics,significance,beir,db}.py` — não reimplementar
  nDCG/permutação/loader.
- **Reference project:** `knowledge-base/references/openllmetry/.../scifact/scifact_corpus.jsonl` (formato jsonl de
  corpus BEIR já presente) — formato de I/O do loader.
- **Skill de patterns:** nenhuma `skills/*-patterns/` casa com o tema (medição/benchmark lexical) — verificado no
  Step 0; nenhuma a citar ou sobrepor.
- **External literature:** BM25 (Robertson & Zaragoza 2009); RRF/known-item TREC named-page finding task
  (Craswell & Hawking, TREC-2004 Web track) — metodologia known-item; BEIR (Thakur et al. 2021, `arXiv:2104.08663`)
  — nDCG@10 protocol. LogHub (He et al., ISSRE 2020, `arXiv:2008.06448`) — o corpus público de logs.

## Objective

- [ ] Sub-goal 1 — MRR@10/Success@1/Recall@10 (known-item) implementados e testados (metrics + gerador de query).
- [ ] Sub-goal 2 — loader do corpus público LogHub → docs, determinístico e cacheável.
- [ ] Sub-goal 3 — 3 retrievers plugáveis (Tantivy BM25 own-engine, PG `ts_rank_cd`, `pg_textsearch` BM25) sobre o mesmo corpus.
- [ ] Sub-goal 4 — runner que roda BEIR (nDCG) + log-proxy (known-item) ≥3 seeds, mean±std, teste pareado, e emite JSON.
- [ ] Sub-goal 5 — tamanho de índice + latência de ingest medidos para os candidatos de storage no corpus.
- [ ] Sub-goal 6 — ADR-0052 decidindo heap vs index AM com o custo/benefício medido (reconcilia o DoD "index AM").
- [ ] Sub-goal 7 — report `docs/benchmarks/m140-1-lexical-measurement.md` com veredito honesto (bate ou não a baseline).

## ADRs

### D1 — Corpus proxy público de logs (LogHub) para o eixo lexical, com caveat declarado

- **Decision:** o eixo "traces/logs" do DoD usa um dataset público do LogHub (ex.: HDFS ou BGL) como proxy
  lexical trace-like, medido por known-item retrieval; o eixo BEIR (scifact+nfcorpus) provê a qualidade graded
  com qrels humanos. O report declara explicitamente o caveat de fidelidade de domínio.
- **Rationale:** o corpus de traces de produção do theo-lens não existe no repo (scout exaustivo). Decisão do
  owner (2026-07-22). O known-item não fabrica relevância (o gold é o próprio doc) — honesto sob `public-copy.md`.
  A validação em traces reais é o boundary explícito do M140.4/M141.
- **Alternatives considered:** (a) corpus representativo gerado do seeder travel-agent — rejeitado pelo owner por
  ser sintético; (b) DB real do theo-lens em droplet — não disponível/acessível; (c) só BEIR — rejeitado porque
  BEIR não é lexical trace-like e o risco (a) do milestone é justamente "medir num corpus lexical antes de construir".
- **Consequences:** número honesto e 100% reproduzível hoje, sem droplet; o caveat de proxy é permanente no
  report e vira dívida rastreada (validação real no M140.4).

### D2 — Reusar `theodb_bench` (nDCG/permutação/loader), estender só com known-item

- **Decision:** nDCG@k, recall@n, `decide()`/permutação e o loader BEIR são consumidos de `theodb_bench` sem
  edição; o novo código é apenas o que não existe: MRR/Success@1 (known-item), o loader LogHub, os 3 retrievers e o runner.
- **Rationale:** Rule 9 (não reinventar) + `parsimony-ladder` rung 4 (reusar dep já instalada). O harness M138 já
  é a autoridade de significância; divergir dela quebraria a comparabilidade com o M138.
- **Alternatives considered:** reescrever um harness lexical dedicado — rejeitado (duplicação de conhecimento,
  DRY; risco de divergir do método M138 já revisado pelo council-benchmark).
- **Consequences:** o diff é pequeno e cirúrgico; a comparabilidade com M138 é preservada; MRR é aditivo.

### D3 — `tantivy` (tantivy-py, MIT) como motor BM25 do eixo qualidade — não o build in-PG

- **Decision:** o eixo de **qualidade** (nDCG/MRR) usa `tantivy` (o binding Python MIT do mesmo Tantivy do spike)
  em processo; NÃO exige build do TheoDB nem `pg_textsearch` in-PG para medir ranking.
- **Rationale:** ranking BM25 é **independente do storage** — o score que o Tantivy produz é o mesmo in-PG ou
  standalone (o storage só afeta latência/índice/MVCC, não a ordenação). Medir qualidade off-PG é honesto e
  destrava a medição sem a barreira de link pgrx que o M139 documentou. `tantivy` é MIT (D1-clean), rung 4.
- **Alternatives considered:** (a) build in-PG do spike `spike-lexical` + medir via SQL — rejeitado: custo de
  droplet/build sem ganho de fidelidade de **ranking** (o score é idêntico); a latência/índice in-PG é medida no
  eixo de storage (T3) e já tem a evidência do M139. (b) reimplementar BM25 em Python — rejeitado (Rule 9).
- **Consequences:** a qualidade é medida hoje, local; a diferença in-PG vs standalone fica **restrita** a
  latência/índice (T3), onde o M139 já mediu 2,8× e o T3 confirma no corpus de logs.

### D4 — Veredito de storage por ADR medido: heap buffer-then-flush como default, index AM como alternativa rejeitada-com-razão

- **Decision:** o ADR-0052 decide **heap buffer-then-flush** como o storage da engine (M140.3+), com o index AM
  custom registrado como alternativa **rejeitada** por over-engineering, salvo se o T3 medir um custo de heap
  (índice/ingest) que inverta o balanço.
- **Rationale:** o M139 mediu que o heap dá MVCC/WAL/crash de graça (O(milhares de LoC) vs os 105k do ParadeDB) e
  índice 2,8× menor que `pg_textsearch`. Rule 9 + anti-YAGNI: um AM custom só se justifica se o heap medir um
  custo proibitivo — o T3 é o gate objetivo dessa inversão.
- **Alternatives considered:** index AM custom à la ParadeDB (o DoD original do M140, anterior ao spike) —
  rejeitado a menos que o T3 meça inversão; ParadeDB é AGPL (inelegível como código, só estudo).
- **Consequences:** o ADR reconcilia o DoD "index AM" com o achado heap do M139; se heap vence, M140.3 constrói
  sobre heap sem AM custom (menos LoC, menos superfície de crash).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Corpus log-proxy pode não refletir traces reais do theo-lens (favorecer BM25 ou ts_rank de modo diferente) | Medium | Caveat declarado no report; validação real no M140.4/M141; conclusão só de "sinal", não de produção | bench |
| Ranking off-PG (Tantivy-py) pode divergir sutilmente do Tantivy in-PG (versão/tokenizer) | Low | Fixar versão do `tantivy`; T3 confirma índice no corpus; o tokenizer é declarado no report | bench |
| Known-item favorece exact-match lexical (viés pró-BM25) | Medium | Reportar TAMBÉM nDCG BEIR (graded, qrels humanos) — os dois eixos; declarar o viés known-item no report | bench |
| docker postgres:18 pode não subir no ambiente / porta ocupada | Low | Runner detecta e falha claro (fail-fast); porta configurável; skip do eixo PG com WARN honesto se indisponível | bench |
| `tantivy` (tantivy-py) indisponível no ambiente | Low | `requirements.txt` fixa versão; runner falha claro pedindo `pip install -r`; sem fallback silencioso | bench |

## Unresolved Questions

- Q1 — Qual dataset LogHub específico (HDFS vs BGL vs mistura) dá o sinal lexical mais representativo de traces?
  → resolvido no T2 medindo em ≥1 e declarando qual; default HDFS (o mais citado), BGL como sensibilidade se der tempo.
- Q2 — O eixo `pg_textsearch` BM25 exige a extensão instalada localmente; se indisponível, o report cita os
  números medidos do M138 como referência em vez de re-rodar? → sim: `pg_textsearch` é **referência** (D3); o
  head-to-head primário é Tantivy vs ts_rank_cd; o report reusa os números M138 do `pg_textsearch` explicitamente citados.
- Q3 — Quantas queries known-item por corpus para significância? → ≥300 (paridade com o N do BEIR do M138),
  amostradas deterministicamente por seed.

## Dependency Graph

```
Phase 0 (metrics known-item + loader logcorpus) ──▶ Phase 1 (3 engines) ──▶ Phase 2 (runner + BEIR + significância)
                                                          │                          │
                                                          ▼                          ▼
                                                   Phase 3 (storage: índice+ingest)   │
                                                          │                          │
                                                          └──────────┬───────────────┘
                                                                     ▼
                                                       Phase 4 (ADR-0052 + report)  ──▶ Final: Integration Validation (roda tudo, emite artefato)
```

Phase 0 e a parte "engines" da Phase 1 podem paralelizar parcialmente, mas o runner (Phase 2) bloqueia em ambos.
Phase 3 depende só do engine Tantivy (Phase 1). Phase 4 depende de 2 e 3.

---

## Phase 0: Métrica known-item + loader de corpus

**Objective:** as fundações testáveis (pgrx-free, puro Python) — a métrica MRR/Success@1 e o loader do corpus
público — antes de qualquer engine.

### T0.1 — Métrica known-item (MRR@10 / Success@1 / Recall@10) + gerador de query

#### Objective
Adicionar as métricas known-item e um gerador determinístico de query a partir de um doc, reusando `metrics.py`.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — cria `theodb_bench/knownitem.py` com `mrr_at_k(ranked_ids, target_id, k)`,
   `success_at_1(ranked_ids, target_id)`, `recall_at_k` (reusa `metrics.recall_at_n` com qrel `{target:1}`), e
   `make_known_item_query(doc_text, rng)` que extrai um trecho distintivo (n-grama de termos raros) do doc.
2. **Why it is necessary now** — o eixo log-proxy do DoD (D1) mede known-item, e known-item não existe no harness
   (metrics.py só tem nDCG/recall que exigem qrels). É a fundação pura e testável (Phase-5 order: reference model
   antes de engine); sem ela o runner não tem oráculo.

#### Evidence
`benchmarks/theodb_bench/metrics.py:36-67` (`ndcg_at_k`/`recall_at_n` — a base a reusar via qrel singleton);
`docs/benchmarks/m138-bm25-fusion.md:26` (N=300 queries — o alvo de amostragem do Q3).

#### Files to edit
```
benchmarks/theodb_bench/knownitem.py — (NEW) mrr_at_k, success_at_1, recall_known_item, make_known_item_query
benchmarks/theodb_bench/test_knownitem.py — (NEW) RED tests primeiro (TDD)
```

#### Deep file dependency analysis
- `knownitem.py` (NEW) importa `metrics.recall_at_n` (Baseline row metrics.py) — reuso, não duplicação (D2).
- Nenhum downstream ainda; o runner (T2.1) será o primeiro caller.

#### Deep Dives
- `mrr_at_k`: dado `ranked_ids` e `target_id`, retorna `1/(rank+1)` se `target_id` está no top-k, senão 0.0.
- `make_known_item_query`: tokeniza o doc, escolhe determinísticamente (por `rng` semeado) os `m` termos de maior
  raridade local (menor frequência no doc, desempate lexical), retorna-os como query. Invariante: mesma seed →
  mesma query (determinismo — `testing.md §6` proíbe randomness não-injetada).
- Edge cases: doc vazio → query vazia (retriever retorna nada → MRR 0, mensurável); target fora do top-k → 0.0;
  k≤0 → ValueError (fail-fast, `error-handling.md`).

#### Pseudo-code / Signatures
```pseudocode
def mrr_at_k(ranked_ids: list[str], target_id: str, k: int) -> float:
  if k <= 0: raise ValueError
  for i, did in enumerate(ranked_ids[:k]):
    if did == target_id: return 1.0/(i+1)
  return 0.0

def make_known_item_query(doc_text: str, rng, m=5) -> str:
  toks = tokenize(doc_text)                 # lowercase, split non-alnum
  if not toks: return ""
  by_rarity = sorted(set(toks), key=lambda t: (local_freq(t, toks), t))
  return " ".join(by_rarity[:m])
# Example: doc "error timeout blk_123 retry timeout" → query "blk_123 error retry" (rng-fixed)
```

#### Tasks
1. Escrever os RED tests (T0.1 TDD) primeiro.
2. Implementar `mrr_at_k`, `success_at_1`, `recall_known_item`, `make_known_item_query`.
3. REFACTOR: extrair `tokenize` compartilhável se o loader (T0.2) precisar do mesmo.

#### TDD
```
RED:  test_mrr_target_at_rank1_is_1_0() — target em rank 0 → 1.0
RED:  test_mrr_target_at_rank3_is_one_third() — rank 2 → 1/3
RED:  test_mrr_target_absent_is_0() — target fora do top-k → 0.0
RED:  test_mrr_k_zero_raises() — k=0 → ValueError (fail-fast)
RED:  test_success_at_1_true_only_when_rank0()
RED:  test_make_query_is_deterministic_under_same_seed() — mesma seed → query idêntica
RED:  test_make_query_empty_doc_returns_empty()
GREEN: implementar knownitem.py mínimo
REFACTOR: None expected (ou extrair tokenize)
VERIFY: cd benchmarks && python3 -m pytest theodb_bench/test_knownitem.py -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Todos os RED tests passam depois do GREEN.
- [ ] `make_known_item_query` é determinístico sob mesma seed (test prova).
- [ ] Pass: lint — `ruff check benchmarks/theodb_bench/knownitem.py` zero warnings.
- [ ] Pass: size — `knownitem.py` ≤ 150 linhas.

#### DoD
- [ ] `python3 -m pytest theodb_bench/test_knownitem.py -q` verde.
- [ ] `ruff check` limpo no arquivo novo.
- [ ] CHANGELOG `[Unreleased]` tocado.

### T0.2 — Loader do corpus público LogHub → docs

#### Objective
Carregar um dataset público de logs (LogHub HDFS por default) num conjunto de docs `{id: text}` determinístico e cacheável.

#### Why this step (action + reasoning)
1. **What this step does** — cria `theodb_bench/logcorpus.py` com `load_logcorpus(dataset, n, cache_dir, seed)`
   que baixa (ou lê do cache) as linhas de log do LogHub, amostra `n` determinísticamente e retorna docs.
2. **Why it is necessary now** — é o corpus do eixo D1. Fica isolado do runner (SRP) e cacheável para
   reprodutibilidade (o M138 cacheia BEIR em `.cache138`).

#### Evidence
`docs/benchmarks/m138-bm25-fusion.md:110` (padrão `--cache-dir` do M138); LogHub `arXiv:2008.06448` (dataset público permissivo).

#### Files to edit
```
benchmarks/theodb_bench/logcorpus.py — (NEW) load_logcorpus(dataset, n, cache_dir, seed) -> dict[str,str]
benchmarks/theodb_bench/test_knownitem.py — estende com test do loader sobre um fixture pequeno embutido
```

#### Deep file dependency analysis
- `logcorpus.py` (NEW) — sem dependência de produção; usa stdlib (`urllib`/`hashlib`) + arquivo de cache. Se o
  download não estiver disponível offline, lê de um fixture commitado pequeno (determinístico) e o report declara
  qual fonte foi usada (fail-clear, `error-handling.md`).
- Downstream: o runner (T2.1) e o engine layer (T1.x).

#### Deep Dives
- Determinismo: amostragem por `random.Random(seed)` sobre índices ordenados — mesma seed → mesmo subconjunto.
- Cache: hash do (dataset,n,seed) → arquivo jsonl em `cache_dir`; hit lê direto.
- Edge cases: dataset desconhecido → ValueError (fail-fast); `n` > linhas disponíveis → usa todas + WARN honesto;
  offline sem cache e sem fixture → erro claro com instrução de download (nunca corpus vazio silencioso).

#### Pseudo-code / Signatures
```pseudocode
def load_logcorpus(dataset="hdfs", n=2000, cache_dir=".cache140", seed=0) -> dict[str,str]:
  key = sha1(f"{dataset}:{n}:{seed}")
  if cache_hit(key): return read_jsonl(cache_path(key))
  lines = _source_lines(dataset)          # download OR embedded fixture; ValueError if unknown
  rng = Random(seed); idx = sorted(rng.sample(range(len(lines)), min(n, len(lines))))
  docs = {f"log-{i}": lines[i].strip() for i in idx}
  write_jsonl(cache_path(key), docs); return docs
```

#### Tasks
1. RED tests (loader determinístico + edge cases) primeiro.
2. Implementar loader com cache + fixture fallback.
3. REFACTOR: reusar `tokenize` de knownitem se necessário.

#### TDD
```
RED:  test_load_logcorpus_deterministic_same_seed() — mesma seed → mesmos ids/textos
RED:  test_load_logcorpus_unknown_dataset_raises() — dataset inválido → ValueError
RED:  test_load_logcorpus_cache_roundtrip() — 2ª chamada lê do cache (mesma saída)
GREEN: implementar logcorpus.py
REFACTOR: None expected
VERIFY: cd benchmarks && python3 -m pytest theodb_bench/test_knownitem.py -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Loader determinístico sob mesma seed (test prova).
- [ ] Dataset desconhecido → ValueError (fail-fast).
- [ ] Pass: lint — `ruff check` zero warnings.
- [ ] Pass: size — `logcorpus.py` ≤ 150 linhas.

#### DoD
- [ ] `python3 -m pytest theodb_bench/ -q` exit code 0.
- [ ] `ruff check benchmarks/theodb_bench/` returns zero warnings.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

---

## Phase 1: Os três retrievers plugáveis

**Objective:** uma interface `Retriever` com três implementações medindo ranking sobre o mesmo corpus.

### T1.1 — Interface Retriever + TantivyBM25 (own-engine) + PgTsRank

#### Objective
`lexical_engines.py`: `Retriever` (protocolo `index(docs)` + `search(query,k)->ranked_ids`), `TantivyBM25`
(tantivy-py MIT) e `PgTsRank` (docker postgres:18, `ts_rank_cd` sobre `to_tsvector`).

#### Why this step (action + reasoning)
1. **What this step does** — define a interface e as duas implementações primárias do head-to-head (own BM25 vs
   baseline ts_rank_cd).
2. **Why it is necessary now** — é o coração da medição; o head-to-head primário (D3) é Tantivy vs ts_rank_cd. A
   interface (DIP, `architecture.md §2`) permite o runner ser agnóstico ao motor.

#### Evidence
`trace-read-repository.ts:365` (o `ts_rank` exato a espelhar); `theodb_rs/src/lexical/pg_directory.rs` (o Tantivy
schema/tokenizer do spike a espelhar no tantivy-py para fidelidade); `docs/adr/0051` (o motor own-code).

#### Files to edit
```
benchmarks/theodb_bench/lexical_engines.py — (NEW) Retriever, TantivyBM25, PgTsRank
benchmarks/theodb_bench/test_lexical_engines.py — (NEW) RED tests com corpus tiny embutido
benchmarks/requirements.txt — adiciona `tantivy>=0.22` (MIT, dev-only)
```

#### Deep file dependency analysis
- `lexical_engines.py` (NEW) usa `tantivy` (nova dep MIT) e `theodb_bench.db` (Baseline row db.py) para a conexão PG.
- `PgTsRank` cria uma tabela temp com `to_tsvector('english', text)` + `ts_rank_cd(tsv, websearch_to_tsquery(...))`
  — espelho exato do theo-lens (o baseline).
- Downstream: runner (T2.1).

#### Deep Dives
- `TantivyBM25`: schema com um campo `text` (default tokenizer en) + `id` stored; `index(docs)` num RAMDirectory;
  `search` via `TopDocs(k).order_by_score()` (o mesmo que o spike). Invariante: mesmo tokenizer do spike (en_stem)
  para fidelidade do ranking (Drawback "divergência off-PG").
- `PgTsRank`: espelha `websearch_to_tsquery('english', q)` + `ts_rank_cd` — o baseline exato.
- Edge cases: query vazia → resultado vazio (não erro); corpus vazio → índice vazio, search vazio.

#### Pseudo-code / Signatures
```pseudocode
class Retriever(Protocol):
  def index(self, docs: dict[str,str]) -> None: ...
  def search(self, query: str, k: int) -> list[str]: ...   # ranked doc ids, best first

class TantivyBM25:  # tantivy-py, in-RAM
  def index(docs): build schema{id:stored, text:en_stem}; writer.add_document(...); commit()
  def search(q,k): parser.parse_query(q, ['text']); searcher.search(q, TopDocs(k)); return ids

class PgTsRank:  # docker postgres:18
  def index(docs): create temp table + INSERT + to_tsvector col
  def search(q,k): SELECT id ORDER BY ts_rank_cd(tsv, websearch_to_tsquery('english',%s)) DESC LIMIT k
```

#### Tasks
1. RED tests com corpus tiny (3 docs) provando ordenação conhecida por motor.
2. Implementar `Retriever`, `TantivyBM25`, `PgTsRank`.
3. Adicionar `tantivy` ao requirements; rodar `pip-audit` (D1 dev-only, mas CVE-check).
4. REFACTOR: extrair helper de conexão se duplicar db.py.

#### TDD
```
RED:  test_tantivy_ranks_exact_term_match_first() — doc com o termo raro vem em rank 0
RED:  test_tantivy_empty_query_returns_empty()
RED:  test_pgtsrank_ranks_by_tsrank_cd() — ordenação conhecida (skip se docker PG indisponível, com razão)
RED:  test_retriever_protocol_conformance() — ambos satisfazem index/search
GREEN: implementar lexical_engines.py
REFACTOR: None expected
VERIFY: cd benchmarks && python3 -m pytest theodb_bench/test_lexical_engines.py -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] TantivyBM25 e PgTsRank satisfazem o protocolo `Retriever` (test prova).
- [ ] `test_pgtsrank_*` roda contra docker postgres:18 real (ou skip com razão honesta se indisponível).
- [ ] `tantivy>=0.22` adicionado ao requirements; `pip-audit` limpo.
- [ ] `ruff check benchmarks/theodb_bench/lexical_engines.py` returns zero warnings and `wc -l` ≤ 250.

#### DoD
- [ ] `python3 -m pytest theodb_bench/test_lexical_engines.py -q` exit code 0 (docker PG up).
- [ ] `ruff check benchmarks/theodb_bench/lexical_engines.py` returns zero warnings.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

### T1.2 — PgTextsearchBM25 (referência) OU reuso dos números M138

#### Objective
Terceiro motor de referência: `pg_textsearch` BM25 in-PG. Se a extensão não estiver disponível localmente, o
report cita os números medidos do M138 explicitamente (não re-roda) — decisão honesta, não gap silencioso.

#### Why this step (action + reasoning)
1. **What this step does** — adiciona `PgTextsearchBM25` (se `pg_textsearch` instalável local) OU registra no
   runner que o eixo `pg_textsearch` usa os números de referência do M138.
2. **Why it is necessary now** — o DoD pede os três motores; mas `pg_textsearch` é **referência** (D3), não o
   head-to-head primário. A honestidade (Q2) exige declarar qual caminho foi usado.

#### Evidence
`docs/benchmarks/m138-bm25-fusion.md:31-34,68-72` (os números `pg_textsearch` BM25 já medidos: scifact 0,688 leg / nfcorpus 0,325 leg).

#### Files to edit
```
benchmarks/theodb_bench/lexical_engines.py — adiciona PgTextsearchBM25 (opcional) + flag de disponibilidade
benchmarks/theodb_bench/test_lexical_engines.py — test skip-aware se pg_textsearch ausente
```

#### Deep file dependency analysis
- `PgTextsearchBM25` usa a mesma conexão db.py; requer `CREATE EXTENSION pg_textsearch`. Se falhar, o motor se
  marca indisponível (fail-clear) e o runner usa os números M138 de referência.

#### Deep Dives
- Invariante: nunca fabricar número — se `pg_textsearch` ausente, o report cita M138 com atribuição explícita; se
  presente, mede no mesmo corpus.
- Edge case: extensão ausente → `available=False`, runner registra `source='m138-reference'`.

#### Tasks
1. RED test skip-aware.
2. Implementar `PgTextsearchBM25` com detecção de disponibilidade.
3. REFACTOR: None.

#### TDD
```
RED:  test_pgtextsearch_available_flag_false_when_missing() — sem extensão → available False, sem crash
RED:  test_pgtextsearch_ranks_when_available() — skip se ausente, com razão
GREEN: implementar PgTextsearchBM25
REFACTOR: None expected
VERIFY: cd benchmarks && python3 -m pytest theodb_bench/test_lexical_engines.py -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `pg_textsearch` ausente → `available=False`, sem crash, sem número fabricado.
- [ ] Presente → mede no mesmo corpus.
- [ ] `ruff check benchmarks/theodb_bench/lexical_engines.py` returns zero warnings.

#### DoD
- [ ] `python3 -m pytest theodb_bench/ -q` exit code 0.
- [ ] `ruff check benchmarks/theodb_bench/` returns zero warnings.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

---

## Phase 2: Runner + BEIR + significância

**Objective:** o orquestrador que roda os dois eixos ≥3 seeds, aplica o teste pareado e emite o JSON de dados.

### T2.1 — Runner m140_1 (BEIR nDCG + log-proxy known-item + significância)

#### Objective
`run_m140_1_lexical.py`: para cada eixo, indexa nos motores, roda queries, computa métricas por seed
(mean±std), aplica `significance.decide()` no par (BM25 vs ts_rank_cd), emite JSON em `docs/benchmarks/m140-1-data/`.

#### Why this step (action + reasoning)
1. **What this step does** — orquestra tudo: BEIR (reusa `beir.py` + `metrics.ndcg_at_k`) e log-proxy (reusa
   `logcorpus` + `knownitem.mrr_at_k`), ≥3 seeds, `significance.decide()`.
2. **Why it is necessary now** — é o artefato executável do DoD; sem ele não há número nem gate.

#### Evidence
`benchmarks/run_m138_bm25_fusion.py:1-208` (o padrão de runner a espelhar); `benchmarks/theodb_bench/significance.py` (decide()).

#### Files to edit
```
benchmarks/run_m140_1_lexical.py — (NEW) runner CLI (--dataset, --seeds, --n, --out, --cache-dir)
```

#### Deep file dependency analysis
- Importa `logcorpus`, `knownitem`, `lexical_engines`, `beir`, `metrics`, `significance` — todos já construídos/reusados.
- Emite JSON consumido pelo gate offline (T2.2) e pelo report (T4.2).

#### Deep Dives
- Fluxo por eixo: carrega corpus → indexa nos 3 motores → para cada seed: gera queries (known-item) ou usa qrels
  (BEIR) → ranked_ids por motor → métrica por query → agrega mean±std → `decide(bm25_scores, tsrank_scores)`.
- Invariante de rigor (Phase 2 da skill): ≥3 seeds, mean±std reportado, teste pareado, sem cherry-pick.
- Edge cases: docker PG indisponível → eixo ts_rank marcado `skipped` com razão (não crash); corpus vazio → erro claro.

#### Pseudo-code / Signatures
```pseudocode
def run_axis(axis, engines, seeds) -> dict:
  per_seed = []
  for s in seeds:
    queries, golds = build_queries(axis, s)     # BEIR: qrels; log: known-item
    scores = {name: [metric(eng.search(q,k), gold) for q,gold in ...] for name,eng in engines}
    per_seed.append(aggregate(scores))
  verdict = significance.decide(flatten(bm25), flatten(tsrank), alpha=0.05)
  return {axis, per_seed, mean_std, verdict}
```

#### Tasks
1. RED test do runner em modo `--smoke` (corpus tiny embutido, 1 seed) provando shape do JSON.
2. Implementar o runner.
3. REFACTOR: extrair `run_axis` reutilizável.

#### TDD
```
RED:  test_runner_smoke_emits_valid_json() — --smoke gera JSON com {axis, verdict, mean_std} bem-formado
RED:  test_runner_skips_pg_axis_when_docker_down_with_reason() — sem PG → axis skipped, razão presente, sem crash
GREEN: implementar run_m140_1_lexical.py
REFACTOR: extrair run_axis
VERIFY: cd benchmarks && python3 run_m140_1_lexical.py --smoke --out /tmp/m140_smoke.json && python3 -c "import json;json.load(open('/tmp/m140_smoke.json'))"
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `--smoke` emite JSON bem-formado (test prova).
- [ ] docker PG down → eixo ts_rank `skipped` com razão (sem crash, sem número fabricado).
- [ ] ≥3 seeds no modo full; mean±std no JSON.
- [ ] `ruff check benchmarks/run_m140_1_lexical.py` returns zero warnings and `wc -l` ≤ 300.

#### DoD
- [ ] `python3 run_m140_1_lexical.py --smoke --out /tmp/m140_smoke.json` exit code 0.
- [ ] `ruff check benchmarks/run_m140_1_lexical.py` returns zero warnings.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

### T2.2 — Gate offline de decisão

#### Objective
`test_m140_1_decision.py`: lê o JSON real de `docs/benchmarks/m140-1-data/` e afirma o veredito (o número existe,
o teste pareado rodou, o `flip`/`no-flip` é derivado de `decide()`, não hardcoded).

#### Why this step (action + reasoning)
1. **What this step does** — o gate offline que torna o veredito auditável e não-fabricado (espelha `test_m138_decision.py`).
2. **Why it is necessary now** — é a **métrica do Goal**: o Goal é "o gate offline passa verde sobre números reais".

#### Evidence
`benchmarks/theodb_bench/test_m138_decision.py` (o padrão de gate offline a espelhar).

#### Files to edit
```
benchmarks/theodb_bench/test_m140_1_decision.py — (NEW) lê o JSON e afirma estrutura + veredito derivado
```

#### Deep file dependency analysis
- Lê o JSON emitido por T2.1; falha claro se ausente (roda o runner primeiro).

#### Deep Dives
- Afirma: cada eixo tem `verdict` com `p_value`, `mean_diff`, `wins/losses/ties`; o `flip` == (`p<0.05` e `mean_diff>0`);
  BEIR reproduz os números M138 dentro de tolerância (âncora anti-fabricação, como o M138 fez).
- Edge case: JSON ausente → skip com instrução de rodar o runner (não falso-verde).

#### Tasks
1. Escrever o gate (é ele próprio o test).
2. Rodar o runner full para gerar o JSON, então o gate.

#### TDD
```
RED:  test_beir_axis_reproduces_m138_within_tol() — nDCG BEIR bate M138 ±0.01 (âncora)
RED:  test_verdict_flip_is_derived_not_hardcoded() — flip == (p<0.05 and mean_diff>0)
RED:  test_each_axis_has_paired_test_fields()
GREEN: (o gate lê o JSON real gerado pelo runner)
REFACTOR: None expected
VERIFY: cd benchmarks && python3 -m pytest theodb_bench/test_m140_1_decision.py -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Gate lê o JSON real e afirma estrutura + veredito derivado (não hardcoded).
- [ ] BEIR reproduz M138 ±0.01 (âncora anti-fabricação).
- [ ] JSON ausente → skip com instrução (nunca falso-verde).

#### DoD
- [ ] `python3 -m pytest theodb_bench/test_m140_1_decision.py -q` exit code 0 over the real JSON. **(Este é o metric do Goal.)**
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

---

## Phase 3: Medição de storage (índice + ingest)

**Objective:** os números que alimentam o ADR de storage — tamanho de índice e latência de ingest no corpus de logs.

### T3.1 — Índice + ingest latency dos candidatos de storage

#### Objective
Medir, no corpus de logs, o tamanho do índice Tantivy (o payload do heap buffer-then-flush) e a latência de
ingest, comparando com o número de referência do `pg_textsearch`/GIN (M139 mediu 2,8× menor).

#### Why this step (action + reasoning)
1. **What this step does** — instrumenta o `TantivyBM25.index()` para reportar bytes do índice + tempo de ingest;
   compara com o índice GIN/`pg_textsearch` no mesmo corpus.
2. **Why it is necessary now** — é o DoD-3 e o insumo do gate de inversão do ADR-0052 (D4): heap só perde para AM
   custom se medir custo proibitivo.

#### Evidence
`docs/adr/0051` + `docs/benchmarks/` do M139 (índice 2,8× menor — o número a confirmar no corpus de logs).

#### Files to edit
```
benchmarks/run_m140_1_lexical.py — adiciona subcomando/flag --storage que emite {index_bytes, ingest_ms} por motor
benchmarks/theodb_bench/test_lexical_engines.py — test que index_bytes>0 e ingest_ms>0 medidos
```

#### Deep file dependency analysis
- Estende o runner (T2.1) e o `TantivyBM25` (T1.1) com instrumentação de tamanho/tempo. Sem nova dep.

#### Deep Dives
- `index_bytes`: soma dos bytes dos segmentos Tantivy (o que iria pro heap `theodb.lexical_files`); para PG,
  `pg_total_relation_size` do índice GIN / da relação `pg_textsearch`.
- `ingest_ms`: wall-clock do `index(docs)` (mean±std ≥3 runs — rigor).
- Invariante: número medido, nunca estimado (`public-copy.md`).

#### Tasks
1. RED test (index_bytes>0, ingest_ms>0 medidos, não zero/estimado).
2. Instrumentar index() + flag --storage no runner.
3. REFACTOR: None.

#### TDD
```
RED:  test_tantivy_reports_index_bytes_positive()
RED:  test_ingest_latency_measured_over_3_runs_mean_std()
GREEN: instrumentar
REFACTOR: None expected
VERIFY: cd benchmarks && python3 -m pytest theodb_bench/test_lexical_engines.py -k storage -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] index_bytes e ingest_ms medidos (>0) para Tantivy; PG GIN/pg_textsearch medido ou citado do M139.
- [ ] mean±std sobre ≥3 runs para ingest.
- [ ] `ruff check benchmarks/theodb_bench/lexical_engines.py` returns zero warnings.

#### DoD
- [ ] `python3 -m pytest theodb_bench/test_lexical_engines.py -k storage -q` exit code 0.
- [ ] `jq '.storage.index_bytes' docs/benchmarks/m140-1-data/result.json` returns a value > 0.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

---

## Phase 4: ADR + report (o veredito)

**Objective:** consolidar os números num ADR de storage e num report honesto.

### T4.1 — ADR-0052: decisão de storage (heap vs index AM)

#### Objective
Escrever `docs/adr/0052-m140-1-lexical-storage-decision.md` decidindo heap buffer-then-flush (D4), com os números
de T3 e a evidência do M139, e o AM custom como alternativa rejeitada-com-razão (a menos que T3 meça inversão).

#### Why this step (action + reasoning)
1. **What this step does** — o ADR que reconcilia o DoD original "index AM" com o achado heap do M139, com número.
2. **Why it is necessary now** — é o DoD-2; M140.3 consome esta decisão.

#### Evidence
T3 (índice/ingest medidos); `docs/adr/0051` (M139 heap); `docs/benchmarks/m138-bm25-fusion.md` (referência pg_textsearch).

#### Files to edit
```
docs/adr/0052-m140-1-lexical-storage-decision.md — (NEW)
```

#### Deep file dependency analysis
- Documento; sem downstream de código. M140.3 (futuro) o cita.

#### Deep Dives
- Estrutura ADR: Contexto, Decisão (heap ou AM conforme T3), Alternativas (AM custom — rejeitada salvo inversão),
  Consequências, evidência medida. Invariante: a decisão SEGUE o número de T3, não o inverso.
- Edge case: se T3 medir inversão (heap custo proibitivo), o ADR decide AM e registra a inversão — honestidade sobre a hipótese.

#### Tasks
1. Escrever o ADR com os números reais de T3.

#### TDD
```
RED:  (documento — validado por check_xrefs.py resolvendo as citações do ADR)
GREEN: escrever ADR
REFACTOR: None expected
VERIFY: python3 scripts/check_xrefs.py 2>&1 | tail -5
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] ADR tem Decisão + ≥1 alternativa rejeitada com razão + Consequências + evidência medida (T3).
- [ ] A decisão segue os números de T3 (não hardcoded antes de medir).
- [ ] `check_xrefs.py` resolve as citações do ADR.

#### DoD
- [ ] `test -f docs/adr/0052-m140-1-lexical-storage-decision.md` exit code 0.
- [ ] `python3 scripts/check_xrefs.py` returns Overall PASS.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

### T4.2 — Report do benchmark (o artefato do Goal)

#### Objective
`docs/benchmarks/m140-1-lexical-measurement.md`: headline honesto (bate/não-bate), tabelas (nDCG BEIR + MRR
log-proxy), teste pareado, storage (índice/ingest), caveat de proxy declarado, reprodução.

#### Why this step (action + reasoning)
1. **What this step does** — o report medido, no formato do M138 (headline, tabelas, decisão, reprodução).
2. **Why it is necessary now** — é o DoD-1 e o artefato central do Goal.

#### Evidence
`docs/benchmarks/m138-bm25-fusion.md` (o formato); o JSON emitido por T2.1; T3 (storage).

#### Files to edit
```
docs/benchmarks/m140-1-lexical-measurement.md — (NEW)
```

#### Deep file dependency analysis
- Consome o JSON de T2.1 + os números de T3. Sem downstream de código.

#### Deep Dives
- Seções: Headline (honest-positive OU honest-negative), Resultado BEIR (nDCG@10 + significância), Resultado
  log-proxy (MRR@10/Success@1 + significância), Storage (índice/ingest), Caveat de proxy (D1), Consequência para o
  roadmap (M140 segue ou para), Reprodução (comandos exatos). Invariante: nenhum número sem o JSON/tabela por trás.
- Edge case: se BM25 NÃO bater o baseline no eixo lexical → headline honest-negative + "M140 para no M140.1" (o boundary).

#### Tasks
1. Rodar o runner full (gerar JSON) + T3 (storage).
2. Escrever o report a partir dos números reais.

#### TDD
```
RED:  (report — validado por hooks/public-copy-lint sem framings banidos + presença das tabelas)
GREEN: escrever o report a partir do JSON real
REFACTOR: None expected
VERIFY: grep -c "nDCG\|MRR\|p =" docs/benchmarks/m140-1-lexical-measurement.md
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Report tem headline honesto + tabela BEIR + tabela log-proxy + storage + caveat de proxy + reprodução.
- [ ] Todo número no report tem o JSON/tabela por trás (não-fabricação).
- [ ] Sem framing banido (`public-copy.md`).

#### DoD
- [ ] `grep -cE "nDCG|MRR|p =" docs/benchmarks/m140-1-lexical-measurement.md` returns a count ≥ 3.
- [ ] `git diff CHANGELOG.md` shows a new `[Unreleased]` entry.

---

## Coverage Matrix

| # | Gap / Requirement (DoD) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Artefato docs/benchmarks/ BM25 vs ts_rank_cd vs pg_textsearch, nDCG/MRR + teste pareado, bate M138 ou reporta honesto | T0.1, T0.2, T1.1, T1.2, T2.1, T2.2, T4.2 | Runner mede os 3 motores em BEIR+log-proxy; gate offline afirma o veredito; report honesto |
| 2 | ADR decidindo storage (heap vs AM) com custo/benefício medido, reconcilia "index AM" | T3.1, T4.1 | T3 mede índice/ingest; ADR-0052 decide heap (D4) com número, AM rejeitado-com-razão salvo inversão |
| 3 | Tamanho do índice + latência de ingest medidos nos candidatos, no corpus | T3.1 | Instrumentação de index_bytes+ingest_ms, mean±std ≥3 runs |
| 4 | Rigor de medição (≥3 seeds, mean±std, pareado, sem cherry-pick, caveat proxy) | T2.1, T2.2, T4.2 | Runner ≥3 seeds + significance.decide(); caveat D1 no report |

**Coverage: 4/4 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] Todos os testes passando — `cd benchmarks && python3 -m pytest theodb_bench/ -q` verde.
- [ ] Zero type errors — n/a (Python sem tipos estritos no harness; ruff cobre).
- [ ] Zero lint warnings — `ruff check benchmarks/` limpo nos arquivos novos.
- [ ] File-size budget respeitado (≤ 500 linhas por arquivo; alvos declarados ≤150–300).
- [ ] CHANGELOG.md atualizado sob `[Unreleased]`.
- [ ] Backward compatibility — `metrics.py`/`significance.py`/`beir.py` NÃO modificados (só consumidos); M138 continua verde.
- [ ] Plan-specific: o gate offline `test_m140_1_decision.py` passa verde sobre os números reais em `docs/benchmarks/m140-1-data/` (o metric do Goal).
- [ ] Plan-specific: ADR-0052 + report escritos a partir dos números reais; veredito honesto (bate ou não).
- [ ] Runtime-metric proof — n/a (milestone de medição; o "runtime metric" é o próprio JSON de benchmark observado não-vazio).
- [ ] Plan archived após merge.

## Failure scenarios (I/O external)

O eixo `PgTsRank`/`PgTextsearchBM25` fala com um **docker postgres:18** (DB externo). Happy-path não prova resiliência.

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `docker postgres:18` (DB) | container não sobe / porta ocupada | runner com env sem PG up | eixo ts_rank marcado `skipped` com razão; runner não crasha; sem número fabricado (T2.1 test) |
| `docker postgres:18` (DB) | `pg_textsearch` extensão ausente | conectar sem a extensão | `PgTextsearchBM25.available=False`; report usa números M138 de referência (T1.2 test) |
| `tantivy` (lib in-proc) | pacote ausente | ambiente sem `pip install -r requirements.txt` | runner falha claro pedindo o install (fail-fast); nunca ranking vazio silencioso |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validar que o pipeline inteiro roda e emite o artefato real.

### Execution
```
cd benchmarks && python3 -m pytest theodb_bench/ -q            # unit (knownitem, engines, decision gate)
docker run -d --name m140pg -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgres:18   # PG p/ ts_rank
cd benchmarks && python3 run_m140_1_lexical.py --dataset hdfs --seeds 3 --out ../docs/benchmarks/m140-1-data/result.json
cd benchmarks && python3 run_m140_1_lexical.py --beir scifact,nfcorpus --out ../docs/benchmarks/m140-1-data/beir.json
cd benchmarks && python3 -m pytest theodb_bench/test_m140_1_decision.py -q   # gate offline sobre o JSON real
ruff check benchmarks/theodb_bench/knownitem.py benchmarks/theodb_bench/logcorpus.py benchmarks/theodb_bench/lexical_engines.py benchmarks/run_m140_1_lexical.py
python3 scripts/check_xrefs.py 2>&1 | tail -3
```

### Acceptance Criteria
- [ ] Todas as suítes unit verdes.
- [ ] O runner emite JSON real em `docs/benchmarks/m140-1-data/` (não-vazio).
- [ ] O gate offline `test_m140_1_decision.py` verde sobre o JSON real (o metric do Goal).
- [ ] Zero lint warnings nos arquivos novos.
- [ ] Report + ADR escritos a partir dos números reais; sem framing banido.
- [ ] Failure scenarios exercidos (docker down → skip com razão; tantivy ausente → erro claro).

### If Validation Fails
1. Separar falhas causadas pelo plano vs pré-existentes.
2. Corrigir as causadas pelo plano.
3. Re-rodar a cadeia.
4. Pré-existentes logadas na PR, não bloqueiam.
