-- TheoDB M2 DoD-3 — SQL embedding generation from a CONFIGURABLE model.
-- Mirrors AlloyDB's embedding()/google_ml_integration pattern: the database calls a configurable
-- model endpoint (it does NOT ship a model). The endpoint can be a self-hosted local model
-- (tools/embedding_server.py — real fastembed/ONNX) OR any cloud OpenAI-compatible provider.
--
-- M17 (ROADMAP-v2 / ADR 0006): theodb.embed was REWRITTEN in Rust and now ships in the OWN pgrx
-- extension `theodb_rs` (theodb_rs/src/lib.rs), NOT here — so the SQL-only `theodb` extension no
-- longer defines it (plan ADR D1 — avoids a duplicate-definition clash). This file now only ensures
-- the `theodb` schema exists. theodb.embed reads the SAME session GUCs (unchanged contract):
--   SET theodb.embedding_endpoint = 'http://host:8088/v1/embeddings';   -- required
--   SET theodb.embedding_model    = 'BAAI/bge-small-en-v1.5';           -- optional default model
--   SET theodb.embedding_api_key  = '...';                              -- optional bearer token
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

-- deps (vector, vectorscale) declared in theodb.control `requires` (M15; plpython3u dropped in M19) — not created here
CREATE SCHEMA IF NOT EXISTS theodb;
