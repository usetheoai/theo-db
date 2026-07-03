---
slug: m37-ai-summarize
milestone_id: M37
created_at: 2026-07-03
goal: Deliver the ai.summarize(content text, model text DEFAULT NULL) RETURNS text SQL function (mirroring ai.analyze_sentiment/ai.rank), so the last documented-but-unimplemented feature ships, measured by the pre-existing test_summarize_returns_text going GREEN + a negative-case test (NULL content → typed error) against the container.
---

# M37 — `ai.summarize` (fechar a última feature documentada ausente)

## Goal

Entregar a função SQL `ai.summarize(content text, model text DEFAULT NULL) RETURNS text` (espelhando
`ai.analyze_sentiment`/`ai.rank`), tornando a última feature documentada-mas-não-implementada
(`docs/features/11-sumarizacao-conteudo.md`) real — medido pelo teste **já existente**
`test_summarize_returns_text` indo de RED→GREEN + um negative case (`ai.summarize(NULL)` → erro tipado) contra o
container.

## Context

O teste RED já está committado (`benchmarks/tests/test_ai_sql.py` `test_summarize_returns_text`, M11) e o stub
chat server já roteia summarize (`tools/chat_server.py:92`). Falta só a implementação Rust+SQL — o wrapper mais
fino da família `ai.*` (blueprint `m37-ai-summarize-blueprint.md`). O `chat()` compartilhado (`chat.rs:19`) já
trata os negative cases (NULL content → 22023; completion vazia → 38000).

## Baseline Context

### Files that will be touched

| File | LoC | git sha | Why |
|---|---|---|---|
| `theodb_rs/src/chat.rs` | ~270 | (M18) | `ai_summarize(content, model)` = `chat(content, Some("Summarize..."), model)` — espelha `ai_sentiment:73`/`ai_rank:90` |
| `theodb_rs/src/api.rs` | ~640 | (M25) | `_ai_summarize` `#[pg_extern]` wrapper (espelha `_ai_sentiment:55`) + `CREATE FUNCTION ai.summarize` (espelha `ai.analyze_sentiment:334`) |
| `benchmarks/tests/test_ai_sql.py` | (exists) | (M11) | happy path já existe (`test_summarize_returns_text`); adicionar negative case (NULL → erro tipado) |
| `docs/features/11-sumarizacao-conteudo.md` | (exists) | — | status "📋 planejado" → "✅ Entregue" com file:line + teste |

### Current callers / dependents

- `chat::chat` (`chat.rs:19`) — a superfície compartilhada; `ai_summarize` a reusa (NULL/empty já tratados).
- `api.rs:55` `_ai_sentiment` + `api.rs:334` `CREATE FUNCTION ai.analyze_sentiment` — o padrão EXATO a espelhar.
- `tools/chat_server.py:92` — o stub roteia `"summarize" in sys_l`; o system prompt DEVE conter "summarize".
- `benchmarks/tests/test_ai_sql.py` `test_summarize_returns_text` — o RED que fica GREEN.

### Domain glossary

- **ai.summarize** — resumo de texto via LLM; retorna o output do `chat()` direto (texto livre, sem parsing).
- **negative case (texto livre)** — não há "resumo malformado"; os erros são NULL content (22023) + completion
  vazia (38000), herdados do `chat()`.

### Architecture boundaries affected

Nenhuma nova. Camada AI-surface (`chat.rs` lógica + `api.rs` superfície SQL), atrás do modelo síncrono por-linha
(ADR `0007-synchronous-per-row-model-http.md`). Sem nova dependência (parsimony rung 4 — reusa `chat()`).

## Prior Art & Related Work

- Blueprint (este ciclo): `m37-ai-summarize-blueprint.md`.
- In-repo: `ai_sentiment`/`ai_rank` (`chat.rs:73,90`) — o padrão; `_ai_sentiment` + `ai.analyze_sentiment` SQL
  (`api.rs:55,334`); o stub `tools/chat_server.py:92`; o teste `test_summarize_returns_text`.

## ADRs

### ADR-1 — só a função escalar (agg_summarize fora do escopo — YAGNI)
**Decisão:** entregar só `ai.summarize(content, model)`. **Rationale:** o `agg_summarize` do feature doc é packaging
aspiracional estilo AlloyDB; não é o DoD do M37 e é mecanismo novo (YAGNI). **Alternativa rejeitada:** o agregado
agora.

### ADR-2 — sem parsing de saída; negative case herdado do chat()
**Decisão:** `ai_summarize` retorna o output do `chat()` direto; os negative cases (NULL, completion vazia) são do
`chat()`. **Rationale:** resumo é texto livre — não há formato a validar (diferente de sentiment/rank). **Alternativa
rejeitada:** inventar validação de "resumo malformado" (complexidade acidental — não existe tal conceito).

## Dependencies

### Existing — use as-is
| Package | Version | Ecosystem | Why |
|---|---|---|---|
| (pgrx + `chat::chat`) | — | Rust | reusa a superfície HTTP + tratamento de erro existente |

### New — to be introduced
| Package | Version | Ecosystem | Rule 9 rationale | Why |
|---|---|---|---|---|
| (none) | | | — | — |

### Removed
| Package | Last version | Why |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 1 (ai_summarize em chat.rs + _ai_summarize wrapper + SQL ai.summarize + negative test + feature doc)
```

## Phase 1 — ai.summarize (o wrapper + a superfície SQL + docs)

### T1.1 — implementar ai.summarize e tornar o RED test GREEN

#### Why this step
É a única peça faltante da feature documentada. O RED test já existe; a implementação é um mirror de ~5 linhas de
`ai_sentiment` + o wrapper + a SQL. Fecha a última feature ausente de `docs/features/`.

#### Files to edit
- `theodb_rs/src/chat.rs`, `theodb_rs/src/api.rs`, `benchmarks/tests/test_ai_sql.py`, `docs/features/11-sumarizacao-conteudo.md`

#### TDD
- RED (JÁ EXISTE): `test_summarize_returns_text` (`benchmarks/tests/test_ai_sql.py`) — `SELECT ai.summarize('...')`
  retorna texto começando com "A concise summary" (o stub roteia o system prompt de summarize). Falha hoje (a
  função não existe). Given o stub chat server, when `ai.summarize(texto)`, then a saída começa com "A concise
  summary".
- RED (ADICIONAR): `test_summarize_null_content_raises_typed` — `SELECT ai.summarize(NULL)` levanta erro tipado
  (22023, via `chat()` "prompt must not be NULL"). Given NULL content, when `ai.summarize(NULL)`, then erro tipado.
- GREEN: `chat::ai_summarize(content, model) = chat(content, Some("Summarize the following text concisely."),
  model)` (o system prompt contém "summarize" para o roteamento do stub); `_ai_summarize` `#[pg_extern]` wrapper;
  `CREATE FUNCTION ai.summarize(content text, model text DEFAULT NULL) RETURNS text` (+ `REVOKE FROM PUBLIC` se o
  padrão das outras `ai.*` exigir — espelhar).
- REFACTOR: nenhum (é o mirror mínimo).

#### Concurrency tests
(none — single-threaded) — chamada HTTP síncrona por-linha, sem estado compartilhado (ADR 0007)

#### Failure scenarios
- **NULL content** → 22023 ("prompt must not be NULL"), herdado do `chat()`; testado.
- **Completion vazia do modelo** → 38000 ("empty completion"), herdado do `chat()`.
- **Endpoint HTTP falha** → erro tipado, herdado do `chat()` (post_json).

#### Acceptance criteria
- `test_summarize_returns_text` GREEN + `test_summarize_null_content_raises_typed` GREEN contra o container.
- `cargo pgrx install --release` 0 warnings; coexistência (test_ai_sql inteiro + M18/M19) verde.
- `ai.summarize` respeita o `REVOKE FROM PUBLIC` / VOLATILE das outras `ai.*` (segurança não regride — SSRF via o
  modelo síncrono per-row é o contrato existente do ADR 0007).

#### DoD
- `ai.summarize` na superfície SQL; happy + negative test verdes; `docs/features/11` → "✅ Entregue" com file:line +
  teste, validado por `deep-research/validate_citations.py` (PASS); CHANGELOG (Rule 6).

## Coverage Matrix

| Goal / DoD item | Task(s) |
|---|---|
| `ai.summarize(content, model) RETURNS text` (mirror sentiment/rank), erro tipado em saída malformada | T1.1 |
| Teste de contrato (happy + negative) em test_ai_sql.py | T1.1 (happy já existe; negative adicionado) |
| `docs/features/11` → Entregue com file:line + teste (validado) | T1.1 |
| Só a escalar (agg_summarize fora — YAGNI) | ADR-1 |
| Sem nova dependência; segurança não regride | T1.1 (reusa chat(); REVOKE/VOLATILE) |
| CHANGELOG (Rule 6) | T1.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| System prompt não conter "summarize" → stub não roteia → RED falha | BAIXO | prompt "Summarize the following text concisely." contém "summarize" (stub casa lowercase) | paulohenriquevn |
| Over-engineering (agg_summarize, preview flags do feature doc) | BAIXO | ADR-1 — só a escalar | paulohenriquevn |
| Segurança regride (REVOKE/SSRF) | BAIXO | espelhar o REVOKE FROM PUBLIC + VOLATILE das outras ai.*; ADR 0007 é o contrato existente | paulohenriquevn |

## Unresolved Questions

- Qualidade do resumo não tem benchmark (depende do LLM) → resolvido: nota de honestidade no feature doc + no
  CHANGELOG (a "validação em benchmark" para esta feature é o teste de contrato contra o container, não um número
  de qualidade — que dependeria do modelo).

## Failure scenarios

- **NULL content / completion vazia / HTTP falha** → erros tipados herdados do `chat()` (22023/38000). (T1.1)

## Final Phase — Integration Validation

- `cargo pgrx test` + `test_ai_sql.py` (happy + negative summarize) verdes contra o container.
- `cargo pgrx install --release` 0 warnings; coexistência AI-surface (M18/M19) + o resto de test_ai_sql verde.
- `docs/features/11` → "✅ Entregue" validado (`deep-research/validate_citations.py` PASS); CHANGELOG atualizado.
