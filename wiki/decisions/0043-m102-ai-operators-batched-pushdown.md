---
type: Decision
title: ADR 0043 — Predicados de IA como operadores set-oriented e otimizáveis pelo planner
description: ai.if_batch resolve N linhas num round-trip e ai.if_costly delega ao planner a ordenação de quals, entregando a metade batch que o ADR 0007 havia adiado — ~12× menos latência num modelo real.
resource: git:f7c7b93:docs/adr/0043-m102-ai-operators-batched-pushdown.md
tags: [adr, ai-surface, planner, batched-inference, pushdown, prompt-injection, m102]
adr_id: "0043"
adr_status: Accepted
decision_date: 2026-07-16
milestone: M102
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0043
    resource: git:f7c7b93:docs/adr/0043-m102-ai-operators-batched-pushdown.md
    title: ADR 0043 — M102 AI predicates as set-oriented operators
    last_modified: 2026-07-16
---

Fecha, para **predicados**, a metade batch que o
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md) havia deixado adiada.

# Contexto

O `ai.generate_batch` já fechara a metade batch da superfície *generativa*. Mas o `ai.if` — o
predicado que o usuário de fato quer pôr num `WHERE` — continuava **por linha** (N round-trips HTTP
sequenciais) e **opaco ao planner**: um escalar `VOLATILE` que o otimizador não consegue batchear,
custear ou reordenar contra quals baratos.

# Decisão — duas superfícies sobre a mesma inferência

**`ai.if_batch(condition, vals[]) -> bool[]`** monta N prompts por item e os responde num **único**
round-trip, com um prompt de sistema em lote instruindo "'yes' ou 'no' para cada", que é **a mesma
moldura** que o `ai.if` por linha usa. Como ambas as superfícies passam a carregar a mesma instrução,
suas respostas ficam diretamente comparáveis num modelo real.

**`ai.if_costly(condition, val) -> bool`** é o escalar por linha declarado com **`COST 100000`**, de
modo que a própria ordenação de quals do PostgreSQL avalia os quals relacionais baratos **primeiro**.
Um `WHERE cheap AND ai.if_costly(...)` então curto-circuita a IA nas linhas que o qual barato já
descartou — o push-down de filtro dependency-safe, **delegado ao planner** em vez de reinventado.

**`ai.call_count()` e `ai.call_reset()`** expõem a contagem de round-trips como a métrica de runtime,
provando "1 round-trip para N linhas" em tempo de query.

**`theodb.llm_test_model = 'parity'`** é um modelo determinístico, sem HTTP, usado **apenas** por
testes e benchmarks, para que a equivalência de resultado entre o operador em lote e o caminho por
linha seja provada **sem** um LLM real, pago e instável.

# Alternativas rejeitadas

**Manter só o `ai.if` por linha** — um round-trip por linha, medido 12× mais lento que o batch, e
opaco ao planner. **Uma reescrita completa de planner com nó de filtro semântico** — superfície FFI
grande, quando a ordenação de quais embutida no PostgreSQL **já** entrega o push-down assim que o
predicado carrega custo alto. **Asserir equivalência de resultado contra um LLM real** — não
determinístico, pago e flaky; um gate de correção ruim. O mecanismo é determinístico e é testado
deterministicamente; o modelo real é o **benchmark**, não a asserção.

# Medido

Em [m102](/benchmarks/archive/m102-ai-operators.md): batch faz **1 round-trip** contra **N** do caminho por
linha (1 contra 1000, no modelo determinístico); o push-down avalia a IA em **≤ K sobreviventes**, não
em todos os N; e a latência real com `gpt-4o-mini` fica **≈ 12× menor** em lote, com K=16 (dois runs:
12,17× e 11,81×).

# Teto honesto

É ganho de **composabilidade e round-trip com acurácia estatística**, **ortogonal ao recall vetorial**.
Nunca enquadrado como "mais rápido no vetor".

A correção do mecanismo — um round-trip, alinhamento de NULL, push-down — é provada
deterministicamente. A diferença **residual** num modelo real é *context-bleed*: as N perguntas
compartilham uma mensagem em lote, o que é efeito genuinamente estatístico. **As respostas não são
asseridas byte-idênticas num modelo real**; o modelo determinístico é o gate de correção.[^adr0043]

# Segurança

A superfície recebe valores não confiáveis que viram entrada de prompt — uma superfície de
prompt-injection **inerente**, idêntica à das funções `ai.*` pré-existentes, com o raio de dano
limitado ao **booleano da própria linha**: uma resposta envenenada ou não parseável vira `NULL`, nunca
escalação.

**Não existe quoting à prova de injeção para um prompt de texto livre** — diferente do SQL, onde há
identificador citável. O controle honesto é least-privilege: as funções são revogadas de PUBLIC e
carregam um comentário explícito de nunca conceder a um papel isolado. Como defesa em profundidade, a
montagem do prompt por item **colapsa quebras de linha** no valor, para que ele não forje uma nova
linha numerada no protocolo em lote. E o hook do modelo de teste curto-circuita **antes** da resolução
do endpoint, portanto não pode enfraquecer a guarda que nunca alcança — mas **deve** ficar desativado
em produção, sob pena de virar um stub silencioso.

# Adiado

Um modelo de custo calibrado por telemetria amostrada; um nó CustomScan de filtro semântico; e uma
cascata proxy/oráculo com garantia de recall.

[^adr0043]: ADR 0043 — M102: AI predicates as SET-oriented, planner-optimizable operators
