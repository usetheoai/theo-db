# M53 — Híbrida de verdade: BEIR real (scifact) — hybrid RRF vs vector vs BM25 vs FTS

**Date:** 2026-07-07 · **Milestone:** M53 · **Dataset:** BEIR `scifact` (test split, 5.183 docs, 300 queries, qrels binário)
**Embedder:** OpenAI `text-embedding-3-small` (dim 1536, real API, pré-aquecido + cache em disco por `sha256`)
**Métricas:** nDCG@10 (primária) + Recall@100 · **Harness:** `benchmarks/run_m53_hybrid_beir.py` (reusa `theodb_bench.{beir,hybrid,metrics,db}`) · **3 runs** · **Image:** `theodb:m53-bm25` (theodb + pgvector + `pg_textsearch` v1.3.1)
**Raw:** `docs/benchmarks/m53-hybrid-beir.json`

**Verdict:** **A híbrida RRF do produto (`ai.hybrid_search_rrf`) IGUALA/SUPERA o vector-only em BEIR real** (nDCG@10 **0.7337 ≥ 0.7296**, Recall@100 **paridade 0.9733**) — o claim de recall da híbrida, aberto desde 2026-06-28 sem artefato decision-grade, agora **tem artefato** (`public-copy.md §4`). Adicionalmente, o leg **BM25 (`pg_textsearch`) supera dramaticamente o `ts_rank_cd` shipado como retriever lexical** (nDCG@10 **0.6881 vs 0.0703**) — a medição que **executa o gate de adoção BM25** do ADR 0013.

---

## 1. Resultados (mean sobre 3 runs — determinístico, spread 0.0)

| retriever | o que é | nDCG@10 | Recall@100 | determinístico |
|---|---|---|---|---|
| **hybrid** | `ai.hybrid_search_rrf` (RRF: vector `<=>` + `ts_rank_cd`) — **o path do produto** | **0.7337** | **0.9733** | ✅ (spread 0.0) |
| **vector** | pgvector `<=>` cosine, seqscan EXATO | 0.7296 | 0.9733 | ✅ |
| **bm25** | `pg_textsearch` (Okapi BM25, `<@>` top-k) | 0.6881 | 0.9182 | ✅ |
| **fts** | `ts_rank_cd` + GIN (o leg lexical **shipado**) | 0.0703 | 0.0694 | ✅ |

Os 3 runs produziram scores **byte-idênticos** (spread nDCG@10 e Recall@100 = 0.0 em todos os retrievers): dado corpus fixo, embeddings do cache e ordenação SQL determinística, a métrica é reprodutível. Reportamos valores pontuais — **não** fabricamos "mean±std" (seria teatro de medição; ver Rule 3).

## 2. Leitura honesta

- **Híbrida ≥ vector-only, com paridade de recall.** A fusão RRF do produto entrega nDCG@10 **0.7337 vs 0.7296** (vector puro) — um ganho **pequeno (+0.004) mas positivo e determinístico**, com Recall@100 idêntico (0.9733). O DoD ("o claim de recall da híbrida precisa de artefato") é cumprido: a híbrida não regride vs vector-only e o marginal é a favor da fusão. **Por que o ganho é pequeno:** a híbrida medida funde vector + **`ts_rank_cd`**, e o `ts_rank_cd` é fraquíssimo em scifact (0.07, ver abaixo) → contribui pouco para o RRF, então a híbrida ≈ vector. Uma híbrida fundindo vector + **BM25** (o leg opt-in do item 2) tende a ser mais forte — medi-la exige o `theodb_rs` novo na imagem bm25 (follow-up rastreado, §4).
- **BM25 ≫ `ts_rank_cd` como retriever lexical (gate de adoção).** BM25 nDCG@10 **0.6881** e Recall@100 **0.9182** contra `ts_rank_cd` **0.0703 / 0.0694** — um gap lexical enorme (~9.8× em nDCG@10), **consistente** com a medição prévia do M7 (BM25 0.9546 vs `ts_rank_cd` 0.5143 num fixture de 12 docs "not decision-grade"; `m7-bm25-vs-tsrank.md`). Esta é a evidência **decision-grade** que faltava para o gate de adoção BM25 do ADR 0013. **Decisão registrada:** BM25 é medível-e-materialmente-superior como leg lexical; o leg **opt-in** (`lexical_engine='bm25'`) foi shipado no código (item 2, 5 pg_test verdes) com `ts_rank_cd` preservado como **default**; adotar BM25 como default **na distribuição** permanece gated (ADR 0013 — exceção permissiva mantida-mas-gated; `pg_textsearch` não entra na imagem shipada agora), mas o gate de medição está **executado**.

## 3. ⚠️ Caveats (Rule 3 — honestidade)

1. **Vector leg é seqscan EXATO** (`<=>` sem HNSW) → o nDCG reflete a **qualidade do embedding**, não recall de índice ANN. Proposital: isola a qualidade de **fusão**, não a de índice (o recall de índice é o eixo de M45/M46/M50/M51/M52).
2. **Assimetria `fts`↔`bm25` (herdada, `m7-bm25-vs-tsrank.md`):** `fts_query` tem `WHERE text_tsv @@ plainto_tsquery(...)` — dropa docs que não casam **todos** os termos da query, o que em scifact (claims científicos, paráfrase) zera muitas queries → Recall@100 **0.069**. `bm25_query` é um **top-k ranker puro** (sem filtro `@@`, sempre devolve 100 candidatos via Block-Max WAND). O gap `fts`↔`bm25` **conflaciona ranker + candidate-set** — NÃO é superioridade pura de ranker BM25. Mesmo assim, o nDCG@10 **0.688** do BM25 sobre o **seu próprio** top-k é forte e comparável ao vector.
3. **A híbrida medida usa o leg lexical `ts_rank_cd` (default).** A híbrida-com-BM25 (leg opt-in do M53 item 2) NÃO foi medida aqui — a imagem `theodb:m53-bm25` carrega o `theodb_rs` do M52 (sem o parâmetro `lexical_engine`). Follow-up rastreado (§4).
4. **Métrica apples-to-apples INTERNO.** nDCG@10 usa gain-linear + discount `log2(rank+1)` (`theodb_bench.metrics.ndcg_at_k`), que coincide com `pytrec_eval ndcg_cut`; mas **não** cruzamos com `pytrec_eval` nesta run. Portanto o claim é **comparação interna entre os 4 legs sobre o MESMO corpus/queries/qrels/embeddings** — NÃO "comparável ao leaderboard BEIR". O cross-check com `pytrec_eval` é follow-up (§4).
5. **Reprodutível a partir do cache de embeddings** (keyed por `sha256(model+dim+text)`); re-embed do zero pode mover a 3ª–4ª casa decimal (não-determinismo do provider). Box contida (`load_pre=10.21`, per-run `[9.87, 14.97, 12.45]`) — mas nDCG/recall são **determinísticos e independentes de carga** (sem seed, GT exato); latência **fora de escopo** deste artefato (o DoD é qualidade).

## 4. Follow-ups rastreados (honestidade — o que NÃO foi medido)

- Híbrida-com-BM25 (leg opt-in) vs híbrida-com-`ts_rank_cd`: rebuild da imagem bm25 com o `theodb_rs` do M53 → medir se `lexical_engine='bm25'` na fusão bate a híbrida default.
- Cross-check `pytrec_eval` (`ndcg_cut.10`, `recall.100`) para habilitar comparabilidade com o leaderboard BEIR (hoje: só interno).
- Segundo dataset (nfcorpus, qrels graduado) para exercitar o gain graduado do nDCG.

## 5. Metodologia / reprodução

```bash
docker build -f packaging/Dockerfile.m53-bm25 -t theodb:m53-bm25 .
docker run -d --name theodb-m53bench -p 55493:5432 -e POSTGRES_PASSWORD=postgres \
  theodb:m53-bm25 -c shared_preload_libraries=pg_textsearch
set -a; . ./.env; set +a   # OPENAI_API_KEY (gitignored)
PGHOST=localhost PGPORT=55493 PGUSER=postgres PGPASSWORD=postgres \
  python3 benchmarks/run_m53_hybrid_beir.py --dataset scifact --runs 3 --include-bm25 \
  --out docs/benchmarks/m53-hybrid-beir.json
```

BEIR scifact baixado de `public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip` (cache em `benchmarks/.cache/`, gitignored). Embeddings OpenAI cacheados por `sha256`. GT = qrels/test.tsv (binário). top=100, k_rrf=60 (Cormack 2009, idêntico ao `ai.hybrid_search_rrf`). Raw em `m53-hybrid-beir.json`.
