# Blueprint: M37 — `ai.summarize` (a última feature documentada ausente)

> **Discovery verdict:** SHIPPABLE — trivial structural mirror. O teste RED **já existe e já está committado**
> (`test_summarize_returns_text`, M11 commit 6972fbd) e o stub chat server **já roteia** summarize
> (`tools/chat_server.py:92` — `"summarize" in sys_l → "A concise summary: " + _CANNED`). Falta SÓ a implementação
> Rust+SQL. Método: grounding do padrão `ai_sentiment`/`ai_rank` + a superfície `chat()`.

**Slug:** `m37-ai-summarize` · **Owner:** paulohenriquevn · **Created:** 2026-07-03

## Context

`ai.summarize` é a única feature em `docs/features/` genuinamente não implementada (`11-sumarizacao-conteudo.md`).
O DoD do M37 pede a função escalar `ai.summarize(content text, model text DEFAULT NULL) RETURNS text` espelhando
`ai_sentiment`/`ai_rank`. O `agg_summarize` (agregado, mencionado no feature doc estilo AlloyDB) está FORA do DoD —
YAGNI, não entra.

## Coverage Corner 1 — Integration Tests

O teste de contrato **já existe** (`benchmarks/tests/test_ai_sql.py` `test_summarize_returns_text`): chama
`SELECT ai.summarize('...')` e assere que a saída começa com "A concise summary" (prova o roteamento do system
prompt de summarize). Está RED hoje (a função não existe). M37 o torna GREEN. Adicionar um negative case
(`ai.summarize(NULL)` → 22023, via `chat()` que já trata NULL).

## Coverage Corner 2 — Dependencies

Nenhuma nova. Reusa `chat::chat()` (`chat.rs:19`) que já faz a chamada HTTP + trata NULL (22023) + completion vazia
(38000). Zero mecanismo novo.

## Coverage Corner 3 — Tools

`chat::chat(content, Some("Summarize..."), model)` — o system prompt DEVE conter a palavra "summarize" (o stub
roteia em `"summarize" in sys_l`, `tools/chat_server.py:92`). `#[pg_extern]` wrapper + `CREATE FUNCTION
ai.summarize` (mesmo padrão de `ai.analyze_sentiment`, `api.rs:334`).

## Coverage Corner 4 — Techniques

**Diferença de summarize vs sentiment/rank:** summarize retorna **texto livre** (não um label/número validável),
então NÃO há parsing de saída — retorna direto o output do `chat()`. Os negative cases (NULL content, completion
vazia) são tratados pelo `chat()` compartilhado (22023 / 38000), não por lógica nova. É o wrapper mais fino da
família `ai.*`.

## ADRs

### ADR-1 — só a função escalar (agg_summarize fora do escopo, YAGNI)
**Decisão:** entregar só `ai.summarize(content, model)` (o DoD). **Rationale:** o `agg_summarize` do feature doc é
estilo AlloyDB (packaging aspiracional com `theodb_ml.upgrade_to_preview_version()` etc.) — não é o DoD do M37, e
um agregado é mecanismo novo (YAGNI até haver demanda). **Rejeitado:** implementar o agregado agora.

### ADR-2 — sem parsing de saída (texto livre); negative case herdado do chat()
**Decisão:** `ai_summarize` retorna o output do `chat()` direto; o "erro tipado em saída malformada" para summarize
= completion vazia do modelo (38000) + NULL content (22023), ambos já tratados pelo `chat()`. **Rationale:** um
resumo é texto livre — não há formato a validar (diferente de sentiment/rank). **Rejeitado:** inventar uma
validação de "resumo malformado" (não existe tal conceito para texto livre — seria complexidade acidental).

## Recommendations

1. `chat.rs`: `ai_summarize(content, model) = chat(content, Some("Summarize the following text concisely."), model)`.
2. `api.rs`: `_ai_summarize` wrapper + `CREATE FUNCTION ai.summarize(content text, model text DEFAULT NULL) RETURNS text`.
3. Adicionar negative test (`ai.summarize(NULL)` → erro tipado); o happy path já existe.
4. `docs/features/11-sumarizacao-conteudo.md` → "✅ Entregue" com `file:line` + teste (validar com
   `deep-research/validate_citations.py`). Nota de honestidade: qualidade depende do LLM; sem benchmark de qualidade.

## Top 3 risks

- **R1:** o system prompt não conter "summarize" → o stub não roteia → teste RED continua falhando. *Mitigação:* o
  prompt "Summarize the following text concisely." contém "summarize" (o stub casa lowercase).
- **R2:** over-engineering (agg_summarize, preview flags do feature doc). *Mitigação:* ADR-1 — só a escalar.
- **R3:** inventar validação de saída para texto livre. *Mitigação:* ADR-2 — sem parsing; negative case herdado do chat().
