# Self-host quickstart — TheoDB with the AI-native surface (vectorizer + hybrid search)

Stand up a TheoDB you run yourself, with the AI-native surface a theo-data capability uses for retrieval: an
embedding column kept fresh by `theodb.create_vectorizer` + a background worker, and fused FTS+vector queries via
`ai.hybrid_search_rrf`. This is the recipe behind the M124 dogfood anchor (`theo-data-capability-on-theodb`) and is
proven by `benchmarks/dogfood_anchor_smoke.sh`.

> Scope: this is the **engine** self-host (PG17 + the `theodb_rs` extension). HA / replication / control-plane are
> deploy/platform concerns that live outside this repo.

## 1. Build + install the extension (pgrx)

```bash
# prereqs: rustup, a PG17 dev toolchain
cargo install --locked cargo-pgrx --version 0.19.0
cargo pgrx init --pg17 download          # provisions a managed PG17 (or point --pg17 at your own pg_config)

cd theodb_rs
cargo pgrx install --pg-config "$(cargo pgrx info path pg17)/bin/pg_config"   # builds + installs theodb_rs.so
```

Do NOT run PostgreSQL as `root` — `initdb`/the server refuse it. Use a dedicated OS user (e.g. `pgtest`) that owns
the data directory.

## 2. Enable the vectorizer background worker

The vectorizer worker only runs when the library is preloaded. In `postgresql.conf`:

```
shared_preload_libraries = 'theodb_rs'
```

Restart PostgreSQL after changing this (a reload is not enough for `shared_preload_libraries`).

## 3. Configure the embedding provider (instance-level so the worker sees it)

The worker runs in its own backend, so the embedding GUCs must be set at the **instance** level (`ALTER SYSTEM`
or `postgresql.conf`), NOT per-session `SET`:

```sql
ALTER SYSTEM SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';  -- http(s) only (SSRF-hardened)
ALTER SYSTEM SET theodb.embedding_model    = 'text-embedding-3-small';
ALTER SYSTEM SET theodb.embedding_api_key  = '<YOUR_KEY>';   -- keep out of version control
SELECT pg_reload_conf();
```

Never commit the key. Set it from an environment secret / a secrets manager, not a checked-in file.

## 4. Create the extension + a vectorizer

```sql
CREATE EXTENSION theodb_rs;   -- provides the `vector` type, the `ai.*` surface, and `theodb.*`

CREATE TABLE docs (
  id        int PRIMARY KEY,
  body      text NOT NULL,
  body_tsv  tsvector GENERATED ALWAYS AS (to_tsvector('english', body)) STORED,
  embedding vector(1536)
);

-- Create the vectorizer BEFORE loading content (see Troubleshooting: it does not backfill pre-existing rows).
SELECT theodb.create_vectorizer(
  'docs', 'id', 'body',          -- source table, PK col, content col
  'docs', 'embedding',           -- target table, embedding col
  'text-embedding-3-small', 1536,-- model, dims
  'fixed', 512, 64);             -- chunk strategy / size / overlap

INSERT INTO docs (id, body) VALUES (1, 'HNSW graph index enables approximate nearest neighbor search');
-- the ON-DML trigger enqueues a job; the worker embeds it within a poll interval (~1s) + a provider round-trip.
```

Watch the queue drain:

```sql
SELECT state, count(*) FROM theodb.vectorizer_queue GROUP BY 1;   -- expect rows to reach 'done'
SELECT count(*) FROM docs WHERE embedding IS NULL;                -- expect 0 once embedded
```

## 5. Query — fused FTS + vector (the retrieval a capability points at)

```sql
SELECT id, score
FROM ai.hybrid_search_rrf(
  'docs', 'id', 'body_tsv', 'embedding',
  query_text  => 'how does the index keep vector search fast',
  k => 60, per_leg_limit => 10, result_limit => 5);
-- query_text feeds the FTS leg AND is embedded (via theodb.embed) for the vector leg; RRF fuses the two.
```

## 6. Smoke it end-to-end

```bash
export PGPORT=... PGHOST=... PGUSER=... PGDATABASE=... OPENAI_API_KEY=...   # key from your secret store
bash benchmarks/dogfood_anchor_smoke.sh   # PASS ⇒ the anchor query path works with real embeddings
```

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Embedding column stays NULL for **pre-existing** rows | `create_vectorizer` wires an ON-DML trigger but does **not** backfill rows that already existed | Create the vectorizer **before** loading, or re-touch existing rows (`UPDATE … SET body = body`) to enqueue them |
| Queue rows go to `state = 'failed'` (worker) while `SELECT theodb.embed('x')` works in a session | Known worker-path defect on self-host — the background worker's embed step fails where the session path succeeds (tracked in [#132](https://github.com/usetheodev/theo-db/issues/132)) | Until fixed: populate the column with a session backfill `UPDATE docs SET embedding = theodb.embed(body)::vector WHERE embedding IS NULL;` — the **query** path is unaffected |
| `endpoint must be http(s)://` | SSRF guard rejected a non-http(s) `theodb.embedding_endpoint` (fail-closed, by design) | Use an `https://` endpoint |
| `theodb.embedding_endpoint is not set` | The embedding GUCs were `SET` per-session, so the worker (its own backend) does not see them | Set them with `ALTER SYSTEM` / `postgresql.conf` at the instance level |

## Related

- Anchor contract: `.claude/rules/dogfood-golden-rule.md § 1` (`theo-data-capability-on-theodb`).
- Evidence: `knowledge-base/dogfood/evidence/2026-07-20-anchor-smoke.md` + `…-anchor-failure-modes.md`.
- Smoke: `benchmarks/dogfood_anchor_smoke.sh`.
