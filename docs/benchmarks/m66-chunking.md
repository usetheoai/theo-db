# M66 — Chunking strategies measured on BEIR: doc-recall@k / nDCG@10 per strategy

**Date:** 2026-07-09 · **Milestone:** M66 · **Dataset:** BEIR/NFCorpus (50 queries) · **Métrica primária:** nDCG@10
**Harness:** `benchmarks/run_m66_chunking.py` (chunka via `theodb.chunk` = o SUT; reusa `theodb_bench.metrics`/beir/openai_embed) · **JSON:** `docs/benchmarks/m66-chunking.json`
**ADR:** [`0025-m66-chunking-strategies.md`](../adr/0025-m66-chunking-strategies.md)

> **VEREDITO: STRATEGY_MATTERS.** A estratégia de chunking **move o recall** neste corpus: `sentence` (nDCG@10
> **0.397**) > `recursive` (0.391) > `fixed` (0.372), spread **0.025** (> ruído 0.005). O chunking por sentença
> vence o fixed-size por ~2.5 pontos nDCG@10 no NFCorpus — a estratégia importa, e a escolha é **dependente de
> corpus** (a literatura mostra config X vencendo num corpus e perdendo noutro; reportamos por-corpus, honesto).

---

## 1. Resultado (3 estratégias, mesmo corpus, k-adaptativo)

Corpus NFCorpus chunkado via `theodb.chunk(content, strategy, 512, 64)` (o chunker Rust sob teste), cada chunk
embedado (OpenAI text-embedding-3-small, cache determinístico), indexado numa chunk-table por-estratégia; 50
queries, doc-recall (um doc é recuperado se QUALQUER chunk seu ∈ top-k), **k-adaptativo** (iguala o budget de
contexto entre estratégias — a comparação justa, método Vecta):

| Estratégia | nDCG@10 | doc-recall@k | avg chunk (chars) | k-adaptativo |
|---|---|---|---|---|
| **sentence** | **0.3970** | 0.1899 | 390.1 | 21 |
| recursive | 0.3914 | 0.1871 | 390.4 | 20 |
| fixed | 0.3719 | 0.1778 | 455.0 | 18 |
| **spread (best − worst)** | **0.0251** | 0.0121 | — | — |

**Método:** NFCorpus (3633 docs → ~14-15k chunks por estratégia), chunk_size 512 / overlap 64, embed OpenAI
text-embedding-3-small (dim 1536), retrieval `theodb_hnsw` top-(k·3 chunks) → dedup para docs → doc-recall@k /
nDCG@10 no top-10. `load` 7.23 (box carregada — mas o resultado de qualidade é determinístico, não afetado).

## 2. Veredito (honesto, por-corpus)

- **STRATEGY_MATTERS:** o spread de nDCG@10 (**0.025**) é > a tolerância de ruído (0.005) → a escolha de
  estratégia move o recall neste corpus. **NÃO é honest-negative** (a estratégia importa aqui).
- **Ranking:** `sentence` > `recursive` > `fixed`. O `sentence` evita "hanging sentences" (fragmentos
  gramaticais que embedam mal) — consistente com o default do LlamaIndex. O `fixed` (corte duro por chars,
  chunks maiores 455) perde ~2.5 pontos.
- **k-adaptativo (a comparação justa):** cada estratégia recupera um k diferente (18-21) para entregar ~o
  mesmo budget de contexto — sem isso, a estratégia com chunks menores "ganharia" recall de página trivialmente
  (o erro que o benchmark do Vecta expôs). Reportado.
- **Dependência de corpus (a honestidade central):** este ranking é do NFCorpus (medical fact-checking). A
  literatura (Chroma, Vecta, arXiv:2410.13070) mostra que o vencedor **muda por corpus** — NÃO afirmamos que
  `sentence` é universalmente melhor. O valor é: (a) o chunking é configurável e own-code; (b) a estratégia
  medida-por-corpus move o recall; (c) o operador escolhe por medição no SEU corpus.

## 3. O que este benchmark NÃO afirma

- **NÃO** um vencedor universal — o ranking é do NFCorpus; corpora diferentes invertem (evidência na literatura).
- **NÃO** inclui `semantic` — deferido por evidência (ADR-0025: ganho 0-4pp, freq negativo, 14× custo).
- **NÃO** token-based — o v1 é char-based (declarado, token-based é v2 rastreado).

## 4. Caveats honestos

1. **Amostra:** 50 queries do NFCorpus (o dataset tem ~323). Direção (sentence > fixed) consistente; absolutos movem.
2. **Um corpus:** NFCorpus (medical). O ranking NÃO generaliza — é o ponto (dependência de corpus).
3. **Box carregada** (load 7.23 numa c-8): não afeta o nDCG (determinístico); os builds de índice foram lentos.
4. **chunk_size/overlap fixos** (512/64): variar (256/512/1024)×(0/64/128) é um sweep futuro; o v1 mede as 3 estratégias no ponto default.

## 5. Reprodução

```
# droplet com pgrx pg17 + CREATE EXTENSION theodb_rs CASCADE:
OPENAI_API_KEY=... PGHOST=localhost PGPORT=28817 PGUSER=theo PYTHONPATH=benchmarks \
  python3 benchmarks/run_m66_chunking.py --dataset nfcorpus --chunk-size 512 --overlap 64 \
    --limit-queries 50 --out docs/benchmarks/m66-chunking.json
```

Dados brutos: `docs/benchmarks/m66-chunking.json`.
