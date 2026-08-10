---
type: Decision
title: ADR 0024 — ai.rerank: reranking por cross-encoder via HTTP, com veredito honest-negative
description: A superfície ai.rerank embarca porque é correta e mensurável, mas o benchmark BEIR mediu degradação de −3,8% no nDCG@10 — nenhum ganho de qualidade é afirmado.
resource: git:f7c7b93:docs/adr/0024-m65-ai-rerank-cross-encoder.md
tags: [adr, rerank, cross-encoder, ai-surface, beir, honest-negative, m65]
adr_id: "0024"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M65
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0024
    resource: git:f7c7b93:docs/adr/0024-m65-ai-rerank-cross-encoder.md
    title: ADR 0024 — M65 ai.rerank
    last_modified: 2026-07-09
---

Uma superfície que embarca **apesar** de o benchmark ter dado negativo — e o ADR explica por que
isso é coerente, não contradição.

# Contexto

O RAG SOTA rerankeia o top-k com um **cross-encoder**: query e documento entram juntos no modelo e
sai um escalar de relevância. É mais preciso que o bi-encoder do retrieval, mas custa uma inferência
por par, logo só serve para top-k pequeno.

# Decisão D1 — assinatura e nome

```sql
ai.rerank(query text, documents text[], model text DEFAULT NULL, top_n int DEFAULT NULL)
  RETURNS TABLE(idx int, score real)
```

Ordenada por score decrescente, com `idx` 0-based no array de entrada, para permitir o join de volta
às linhas de origem.

Retornar `TABLE(idx, score)` — em vez de reordenar in-place — converge com AlloyDB, Cohere, Voyage e
Jina, e permite `ORDER BY score DESC`. O precedente interno exato é a busca híbrida RRF.

**O nome é `rerank`, não `rank`, de propósito:** o repositório **já tem** `ai.rank`, que é
LLM-scoring por linha — um prompt, um float, semanticamente diferente. Diverge-se do AlloyDB, que
chama o dele de `ai.rank`, para não colidir.

Rejeitadas: retornar `text[]` reordenado (perde o join e o score); e reusar `ai.rank` (sobrecarregar
um nome com dois contratos).

# Decisão D2 — reusar o cliente HTTP existente

`rerank.rs` reusa o `http.rs::post_json` compartilhado e GUCs livres de sessão; zero cliente novo. O
`http.rs` já tem retry em 429/502/503, defesa SSRF via `with_max_redirects(0)`, timeout de 30 s e
erro tipado. Reinventar seria violar a regra de não reinventar. O parser N-in/N-out — alinhamento
por `results[].index`, com mismatch, duplicata, fora de range e não-numérico virando erro tipado —
espelha a lógica já usada no embed.

# Segurança

Herda o fail-closed do `ai.embed`: endpoint http(s)-only, sem seguir redirects (para não alcançar
metadata interno), timeout, erro tipado, e `REVOKE ALL FROM PUBLIC` tanto na função interna quanto
na pública.

# O gate real — o benchmark, não a superfície

**Superfície que roda ≠ ganho de retrieval provado.** O critério exigia que `ai.rerank` só fosse
aceito se melhorasse nDCG@10 ou MRR mensuravelmente em [BEIR](/technologies/beir.md), com
honest-negative caso não melhorasse. A literatura é explícita que o ganho **não é universal** —
cross-encoders off-the-shelf já degradaram nDCG entre −0,3% e −3,1% em corpora fora de distribuição.

**Veredito medido — HONEST-NEGATIVE.** Em BEIR/SciFact, 100 queries, 3 runs determinísticos
([m65](/benchmarks/archive/m65-rerank.md)): o rerank com BGE-reranker-base **degradou o nDCG@10 em −3,8%**
(de 0,7327 para 0,6947), ao custo de ~1,96 s de p50 por query. O recall@50 ficou conservado (0,92
em ambos, sanidade confirmada). Exatamente o previsto pela literatura, já que SciFact é
fact-checking científico — fora da distribuição do reranker.

# Decisão pós-benchmark

**`ai.rerank` embarca** — a superfície está correta, testada e mensurável, e o valor é fechar o
ciclo retrieve→rerank de forma **medível e model-agnostic**, não um ganho universal.

**Nenhum ganho de qualidade é afirmado.** O operador escolhe o reranker adequado ao seu corpus por
GUC; um reranker in-domain pode ganhar onde este perdeu, mas isso exige o próprio benchmark, não
extrapolação.

**Rerank é opt-in** — ~2 s por query sem ganho garantido não é default.[^adr0024]

# Ressalvas

O corpus do benchmark pode estar fora da distribuição de treino do reranker default — o que torna o
honest-negative mais provável e ainda assim um resultado válido. Sem claim de ganho universal.

[^adr0024]: ADR 0024 — M65 ai.rerank: cross-encoder reranking via HTTP
