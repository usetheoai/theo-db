# Generative-AI SQL functions (`ai.*`) — TheoDB M7-S3

TheoDB exposes generative-AI over your data directly in SQL, mirroring AlloyDB's `ai.generate` /
`google_ml_integration` — but **model-agnostic**: the database calls a **configurable OpenAI-compatible
chat-completions endpoint** (any local or cloud model). The image ships **no model** (only `plpython3u`);
it makes a server-side HTTP call, exactly like `theodb.embed` does for embeddings.

> No performance number on this page is a benchmark (CLAUDE.md TheoDB rule 5). These are functional contracts.

## Configuration (GUCs)

Set per session, per role, or per database (same mechanism as the M2 `theodb.embedding_*` GUCs):

```sql
SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions';  -- required
SET theodb.llm_model    = 'gpt-4o-mini';                                 -- optional default model
SET theodb.llm_api_key  = 'sk-...';                                      -- optional bearer token
```

The endpoint may be a **self-hosted local model** (any OpenAI-compatible `/v1/chat/completions` server) OR a
cloud provider — that portability is the lever over a managed-AI lock-in.

## Functions

| Function | Returns | Purpose |
|---|---|---|
| `ai.generate(prompt text, model text DEFAULT NULL)` | `text` | Raw text completion |
| `ai.if(prompt text, model text DEFAULT NULL)` | `boolean` | Natural-language condition → true/false |
| `ai.analyze_sentiment(content text, model text DEFAULT NULL)` | `text` | One of `positive` / `negative` / `neutral` |
| `ai.summarize(content text, model text DEFAULT NULL)` | `text` | Concise 1–2 sentence summary |
| `ai.rank(prompt text, model text DEFAULT NULL)` | `real` | Numeric score parsed from the model output |

All five are thin wrappers over a single private helper (`ai._chat`) — one HTTP source of truth.

### Examples

```sql
SELECT ai.generate('Explain what TheoDB is in one sentence.');

SELECT name FROM restaurant_reviews
WHERE ai.if('Is this a positive review? ' || review);

SELECT id, ai.analyze_sentiment(review_content) FROM reviews;

SELECT ai.summarize(article_body) FROM articles WHERE id = 42;

SELECT review FROM user_reviews
ORDER BY ai.rank('Score this review 0..1 by customer satisfaction: ' || review) DESC
LIMIT 20;
```

## Behavior & guarantees

- **Fail-fast, typed (Rule 8).** Endpoint unset → `22023`. Non-`http(s)` endpoint → `22023` (SSRF guard).
  Connection refused / timeout / 5xx → `38000`. A model that ignores the output format (`ai.if` non-boolean,
  `ai.rank` no number, sentiment out-of-set) → `22023` — never a silent wrong value.
- **Output-constrained.** Each function sends a system prompt that constrains the model's output so it parses
  deterministically; the parser is the last line of defense if the model misbehaves.

## Security

- **Least-privilege.** `ai._chat` and all five public functions are `REVOKE`d from `PUBLIC` (they make
  server-side outbound HTTP). `GRANT EXECUTE` to specific roles to expose them.
- **SSRF hardening.** Only `http(s)://` endpoints are accepted and HTTP redirects are disabled, so a
  session-set GUC cannot make the server fetch internal / cloud-metadata URLs.
- **API key handling.** `theodb.llm_api_key` is a session GUC — it is visible to `SHOW` and may be captured by
  `log_statement`. Set it **per session, out of band**, never in logged DDL. The key is never echoed in any
  error message.

## Limitations (honest)

- **Synchronous.** Each call is one blocking HTTP round-trip **per row**. `SELECT ai.generate(col) FROM
  big_table` issues one request per row — batch large jobs outside a single statement. **Array/cursor
  "accelerated" modes (per the spec) are a documented follow-up**, not in this slice (YAGNI — the scalar
  functions are the M7 DoD MVP).
- The model is external and configurable; output quality and cost depend on the endpoint you point at.

## Testing

- **Offline (CI):** a deterministic OpenAI-compatible stub (`tools/chat_server.py`) is the endpoint, so each
  function's SQL→HTTP→parse contract is tested with zero external calls (`benchmarks/tests/test_ai_sql.py -k
  'not real'`).
- **Real (opt-in):** `pytest -k real` runs against OpenAI when `THEODB_LLM_ENDPOINT` + `OPENAI_API_KEY` are
  set (key from the gitignored `.env`); it asserts sentiment polarity + shape, never exact text.

## Related

- M2 embeddings sibling: `docs/sql-embeddings.md`
- Target API spec: `docs/features/07-funcoes-ia-sql.md`, `docs/features/10-analise-sentimento.md`, `docs/features/11-sumarizacao-conteudo.md`
- Implementation: `sql/50-theodb-ai.sql`
