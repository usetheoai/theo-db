---
type: Decision
title: ADR 0025 — Chunking declarativo: chunker próprio char-based e chunk-table opt-in; semântico adiado
description: Três estratégias de chunking em Rust puro, Unicode-safe, com o modo 1-doc→N-chunks opt-in para não quebrar contratos existentes; chunking semântico rejeitado por evidência.
resource: git:f7c7b93:docs/adr/0025-m66-chunking-strategies.md
tags: [adr, chunking, rag, vectorizer, unicode, m66]
adr_id: "0025"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M66
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0025
    resource: git:f7c7b93:docs/adr/0025-m66-chunking-strategies.md
    title: ADR 0025 — M66 chunking declarativo
    last_modified: 2026-07-09
---

# Contexto

O [vectorizer](/features/16-vectorizer.md) auto-embedava colunas de texto, mas o **chunking domina a
qualidade do RAG** e o vectorizer era 1-doc→1-vetor in-place. A investigação encontrou um detalhe
revelador: a função `theodb.chunk_text` em plpgsql era **código morto** — existia e nunca era
chamada no fluxo.

# Decisão D1 — chunker próprio, char-based; sem tokenizer no v1

```sql
theodb.chunk(content text, strategy text DEFAULT 'recursive',
             chunk_size int DEFAULT 512, overlap int DEFAULT 64) RETURNS text[]
```

Em Rust puro, com três estratégias: `fixed` (janelas deslizantes), `sentence` (agrupa sentenças por
`.!?`) e `recursive` (hierarquia `\n\n` → `\n` → `. ` → ` `, à la LangChain), mais `overlap`
ortogonal. **Char-based e Unicode-safe** — conta e corta por caractere UTF-8, nunca por byte, de
modo que um grapheme multibyte nunca é partido. Substitui o `chunk_text` morto.

Char-based resolve o caso comum; token-based via BPE é complexidade acidental para o v1, rastreada
como débito. A lógica pura é testável offline — o antídoto direto ao chunker morto e nunca medido.

**Rejeitadas:** adotar `text-splitter` inteiro (resolve tudo de uma vez, mas traz dependência e a
API não casa 1:1 com os reloptions); e token-based no v1 (mais correto para budget do gerador, mas
exige o BPE).

# Decisão D2 — chunk-table opt-in; semântico adiado por evidência

Quando `chunk_strategy` é não-nulo, o worker cria e usa a chunk-table
`{target_table}_chunks (source_pk, chunk_index, chunk_text, embedding)`: 1 doc → N chunks → N
vetores, com um round-trip por doc, deletando os chunks antigos do PK antes do INSERT para não
deixar órfãos no re-embed. O modo 1→1 in-place é **preservado** quando a estratégia é nula, que é o
default.

Mudar in-place para chunk-table seria breaking no contrato de query — o retrieval passaria a
agregar sobre chunks —, então opt-in preserva os vectorizers e queries existentes.

**O chunking semântico é adiado por evidência, não por falta de tempo:** a literatura mede ganho de
0 a 4 pontos percentuais, frequentemente **negativo** ponta a ponta, a 14× o custo; o ganho só
aparece em corpora sintéticos com tópicos entrelaçados.

# Casos de borda e casos negativos

**Borda (entrada válida):** vazio ou só espaço produz 0 chunks; documento menor que o tamanho
produz 1 chunk; uma palavra gigante sem separador força corte por caractere, nunca loop infinito
nem chunk maior que o tamanho; multibyte respeita fronteira de caractere. **Bug pego em teste de
mesa antes do commit:** o carry de overlap acumulava chunks maiores que o tamanho quando o overlap
era zero.

**Negativo (entrada inválida):** `overlap >= size`, `size <= 0` e estratégia desconhecida viram
erro tipado, fail-fast.

# Evidência

Testes verdes na stack real — 16 de chunk (três estratégias, borda, negativo, multibyte, caminhos
de erro) e 13 de vectorizer (chunk-table criada, default preservado, delete removendo os N chunks).

**Benchmark BEIR/NFCorpus, 50 queries** ([m66](/benchmarks/archive/m66-chunking.md)): `sentence` e
`recursive` (nDCG@10 de 0,397 e 0,391) superam `fixed` (0,372), com spread total de 0,025. O degrau
**robusto** é sentence sobre fixed (Δ 0,025); o degrau fino entre sentence e recursive (Δ 0,0055) é
**empate estatístico** dentro do ruído e **não é afirmado**. O k é adaptativo para igualar o budget,
tornando a comparação justa.

**Débito honesto registrado:** n=1 run, com tolerância de ruído assumida; separar sentence de
recursive exigiria desvio pareado e ≥3 runs.[^adr0025]

# Nota de upgrade

As colunas de chunking vivem no schema da extensão Rust, então uma instalação nova já as traz.
Como a extensão pgrx não tem migrações incrementais, um deployment **pré-existente** precisa
reinstalar a extensão ou aplicar um `ALTER TABLE` manual. Declarado, não escondido.

# Ressalvas

Char-based pode divergir do budget de tokens do gerador. E o benchmark por estratégia é dependente
de corpus — a literatura mostra que "config X vence no corpus Y" não generaliza —, por isso é
reportado por corpus.

[^adr0025]: ADR 0025 — M66 chunking declarativo
