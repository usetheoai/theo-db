# M65 — `ai.rerank` cross-encoder medido em BEIR: nDCG@10 com vs sem rerank (HONEST-NEGATIVE)

**Date:** 2026-07-09 · **Milestone:** M65 · **Dataset:** BEIR/SciFact (100 queries) · **Métrica primária:** nDCG@10
**Harness:** `benchmarks/run_m65_rerank.py` (reusa `theodb_bench.metrics` ndcg/recall + beir/openai_embed) · **JSON:** `docs/benchmarks/m65-rerank.json`
**ADR:** [`0024-m65-ai-rerank-cross-encoder.md`](../../adr/0024-m65-ai-rerank-cross-encoder.md)

> **VEREDITO: HONEST-NEGATIVE.** O reranker cross-encoder (BGE-reranker-base) **PIOROU** o nDCG@10 em
> **−3.8 pontos** (0.7327 → 0.6947) sobre o retrieval vetorial no SciFact, ao custo de **~1.96 s** de latência
> por query. Isto é exatamente o que a literatura prevê (cross-encoders off-the-shelf regridem em corpora fora
> da distribuição de treino) e o que o DoD do M65 explicitamente aceita. **A superfície `ai.rerank` funciona e
> é medível; o GANHO de qualidade NÃO é universal** — é uma função do par (reranker, corpus).

---

## 1. O que foi medido (2 braços, mesmo top-k)

Retrieval `theodb_hnsw` top-50 sobre SciFact embedado com OpenAI text-embedding-3-small (cache determinístico);
os 2 braços partem do MESMO top-50 (o rerank só reordena — não adiciona evidência, então Recall@50 é conservado):

| Braço | nDCG@10 | MRR@10 | Recall@50 | rerank p50 (ms) | rerank p95 (ms) |
|---|---|---|---|---|---|
| **A — baseline** (ordem vetorial) | **0.7327** | 0.7007 | 0.92 | — | — |
| B — +rerank (BGE-reranker-base) | **0.6947** | 0.6651 | 0.92 | 1964.8 | 2278.2 |
| **Δ (rerank − baseline)** | **−0.0380** | −0.0356 | 0.00 | — | — |

**Método:** SciFact (`limit_queries=100`), top-k 50 → rerank → nDCG@10 sobre o novo top-10, 3 runs. Reranker
`BAAI/bge-reranker-base` (Apache 2.0) self-hospedado (`rerank_server.py`, cross-encoder real). Embeddings OpenAI
cacheados a disco (determinísticos). Métricas `theodb_bench.metrics.ndcg_at_k`/`recall_at_n` (reusadas — Rule 9).

## 2. Veredito por-eixo (honesto)

- **nDCG@10 — HONEST_NEGATIVE:** Δ = **−0.038** (< tolerância de ruído 0.005, e o sinal é NEGATIVO). O rerank
  **degradou** a qualidade. Reportado com o número, não escondido nem spinado.
- **Determinismo — 3 runs byte-idênticos** (0.732684 / 0.694720 em todas as 3 runs): o Δ é robusto, não ruído
  de amostragem. `load_per_run` = [7.0, 7.86, 7.92] (box carregada — mas os 3 runs idênticos provam que a carga
  não afeta o resultado, que é determinístico por construção: mesmo modelo + mesmo input → mesmos scores).
- **Recall@50 — conservado (0.92 == 0.92):** o sanity check passa — o rerank só reordena o top-50, não adiciona
  nem remove documentos relevantes. Prova que a comparação é justa (mesmo conjunto de entrada).
- **Latência — o custo real:** ~**1.96 s** p50 / **2.28 s** p95 por query só para rerankear 50 docs. O rerank
  **dobra-a-triplica** o custo do retrieval sem ganho de qualidade neste corpus.

## 3. Por que HONEST-NEGATIVE (a explicação, não desculpa)

A discovery (blueprint, R0) já documentava o risco com números: cross-encoders off-the-shelf (ms-marco-MiniLM,
BGE-reranker-base) **degradaram nDCG −0.3% a −3.1%** em corpora fora da distribuição de treino (o nosso −3.8% é
consistente). SciFact é **fact-checking científico** — fora da distribuição de web-search em que o BGE-reranker-base
foi treinado. 2 dos 3 modos de falha da literatura aplicam-se aqui (o 3º — recall baixo — não, pois o
Recall@50 já é 0.92):
1. **Retrieval já bom:** o baseline nDCG@10 0.73 já é alto — o chunk certo já está no topo; reordenar só arrisca
   demover um relevante.
2. **Distribution shift:** o reranker over-weighta sinais que não transferem para o domínio científico.

## 4. A DECISÃO (o DoD exige)

- **`ai.rerank` embarca** — a superfície own-code (cross-encoder via HTTP, model-agnostic, reusa o client
  `ai.embed`) está correta, testada (14 pg_test GREEN) e medível. O valor é fechar o lifecycle retrieve→rerank
  de forma **mensurável e configurável**, não afirmar um ganho universal.
- **NÃO se afirma ganho de qualidade** (public-copy.md §4) — o benchmark mostra regressão neste par (BGE-base,
  SciFact). O operador escolhe o reranker adequado ao SEU corpus via GUC (`theodb.rerank_model`/`_endpoint`);
  um reranker in-domain (ou fine-tuned) pode ganhar onde este perdeu — mas isso é claim que exige o próprio
  benchmark no corpus-alvo, não uma extrapolação.
- **Rerank é opt-in** — dado o custo (~2 s/query) sem ganho garantido, o rerank não é default; o usuário decide.

## 5. Caveats honestos

1. **Amostra:** 100 queries do SciFact (o dataset tem 340). Direção (regressão) consistente com a literatura;
   o valor absoluto do Δ move com o corpus.
2. **Um par (reranker, corpus):** o resultado é de BGE-reranker-base × SciFact. NÃO generaliza para outros
   rerankers/corpora — é exatamente o ponto (o ganho não é universal). A superfície é model-agnostic.
3. **Box carregada** (load ~7-8 numa c-8): os absolutos de latência (~2 s) estão inflados pela carga; o Δ de
   nDCG é determinístico e não afetado.
4. **CPU inference:** o reranker rodou em CPU (self-host); a latência seria menor com GPU/endpoint otimizado — mas
   o Δ de qualidade (o gate) independe do hardware.

## 6. Reprodução

```
# droplet com pgrx pg17 + CREATE EXTENSION theodb_rs CASCADE + rerank_server (BGE-reranker-base):
THEODB_RERANK_ENDPOINT=http://127.0.0.1:8090/rerank OPENAI_API_KEY=... \
PGHOST=localhost PGPORT=28817 PGUSER=theo PYTHONPATH=benchmarks \
  python3 benchmarks/run_m65_rerank.py --dataset scifact --top-k 50 --runs 3 --limit-queries 100 \
    --out docs/benchmarks/m65-rerank.json
```

Dados brutos: `docs/benchmarks/m65-rerank.json`.
