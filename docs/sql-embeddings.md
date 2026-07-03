# SQL embeddings — `theodb.embed()` (M2 DoD-3)

Generate vector embeddings directly from SQL, from a **configurable model**. TheoDB mirrors AlloyDB's
`embedding()` / `google_ml_integration` design: **the database calls a model endpoint — it does not ship
a model.** The image stays lean (no torch/ONNX inside Postgres — the embed surface is Rust in `theodb_rs`);
the model runs out-of-process and is fully swappable.

## Contract

```sql
theodb.embed(content text, model text DEFAULT NULL) RETURNS vector
```

Configuration (per-session or per-role GUCs):

| GUC | Required | Meaning |
|---|---|---|
| `theodb.embedding_endpoint` | yes | OpenAI-compatible `/v1/embeddings` URL |
| `theodb.embedding_model` | no | default model when the call omits one |
| `theodb.embedding_api_key` | no | sent as `Authorization: Bearer …` |

Unset endpoint → fail-fast typed error (`SQLSTATE 22023`), never a silent NULL.

## Two ways to provide the model (the DoD's "local e/ou remoto")

**A. Self-hosted local model** (no cloud, no credentials) — a real model served over HTTP:

```bash
pip install fastembed
python benchmarks/servers/embedding_server.py --host 0.0.0.0 --port 8088 --model BAAI/bge-small-en-v1.5
```

```sql
SET theodb.embedding_endpoint = 'http://host.docker.internal:8088/v1/embeddings';
SELECT theodb.embed('the cat sat on the mat');           -- vector(384)
-- semantic search end-to-end:
SELECT id FROM docs ORDER BY embedding <=> theodb.embed('a feline on a rug') LIMIT 5;
```

**B. Cloud provider** — point the same GUC at any OpenAI-compatible embeddings API:

```sql
SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';
SET theodb.embedding_api_key  = '…';
SELECT theodb.embed('hello', 'text-embedding-3-small');  -- vector(1536)
```

## Validated against real providers

- **Local model** (`benchmarks/servers/embedding_server.py`, fastembed bge-small-en-v1.5) — 384-dim, used by the
  integration tests (real, no mock).
- **Cloud — OpenAI** — `theodb.embed('…', 'text-embedding-3-small')` against `https://api.openai.com/v1/embeddings`
  returns `vector(1536)` with genuine semantics (paraphrase ≪ unrelated in cosine distance). The image ships
  `ca-certificates` so the Rust embed surface's TLS verification succeeds for HTTPS providers.

## Notes (honest)

- `benchmarks/servers/embedding_server.py` ships **bge-small-en-v1.5** (384-dim, ONNX via fastembed — no GPU, no
  torch). It is a real model, used as the test oracle and as a zero-dependency local option.
- The call is synchronous inside the backend (same as AlloyDB's pattern). For bulk embedding of large
  tables, batch outside a single statement; an async/batch helper is future work.
- The function is created on fresh DB init (`docker-entrypoint-initdb.d/30-theodb-embed.sql`) and is
  idempotent (`CREATE OR REPLACE`); applying it to an existing database is a no-op-safe re-run.
