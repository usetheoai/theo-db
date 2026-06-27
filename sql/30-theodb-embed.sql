-- TheoDB M2 DoD-3 — SQL embedding generation from a CONFIGURABLE model.
-- Mirrors AlloyDB's embedding()/google_ml_integration pattern: the database calls a configurable
-- model endpoint (it does NOT ship a model). The endpoint can be a self-hosted local model
-- (tools/embedding_server.py — real fastembed/ONNX) OR any cloud OpenAI-compatible provider.
-- Configuration via GUCs (per-session or per-role):
--   SET theodb.embedding_endpoint = 'http://host:8088/v1/embeddings';   -- required
--   SET theodb.embedding_model    = 'BAAI/bge-small-en-v1.5';           -- optional default model
--   SET theodb.embedding_api_key  = '...';                              -- optional bearer token
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS plpython3u;
CREATE SCHEMA IF NOT EXISTS theodb;

CREATE OR REPLACE FUNCTION theodb.embed(content text, model text DEFAULT NULL)
RETURNS vector
LANGUAGE plpython3u
AS $$
import json
import urllib.request
import urllib.error

def _cfg(name):
    # name is a trusted literal (no user input) — current_setting(..., true) returns NULL if unset.
    return plpy.execute("SELECT current_setting('%s', true) AS v" % name)[0]["v"]

if content is None:
    plpy.error("theodb.embed: content must not be NULL", sqlstate="22023")

endpoint = _cfg("theodb.embedding_endpoint")
if not endpoint:
    plpy.error(
        "theodb.embed: theodb.embedding_endpoint is not set — "
        "SET theodb.embedding_endpoint = 'http://host:port/v1/embeddings'",
        sqlstate="22023",
    )

mdl = model or _cfg("theodb.embedding_model") or "default"
api_key = _cfg("theodb.embedding_api_key")

payload = json.dumps({"input": content, "model": mdl}).encode()
req = urllib.request.Request(endpoint, data=payload, method="POST")
req.add_header("Content-Type", "application/json")
if api_key:
    req.add_header("Authorization", "Bearer " + api_key)

try:
    with urllib.request.urlopen(req, timeout=30) as resp:
        body = json.loads(resp.read())
except urllib.error.URLError as e:
    plpy.error("theodb.embed: embedding endpoint call failed: %s" % e)

try:
    vec = body["data"][0]["embedding"]
except (KeyError, IndexError, TypeError) as e:
    plpy.error("theodb.embed: unexpected embedding response shape (%s): %s" % (e, str(body)[:200]))

if not vec:
    plpy.error("theodb.embed: endpoint returned an empty embedding")

return "[" + ",".join(str(float(x)) for x in vec) + "]"
$$;

COMMENT ON FUNCTION theodb.embed(text, text) IS
  'Generate an embedding for content via the configurable model endpoint (theodb.embedding_endpoint). '
  'Returns a pgvector value. Model selectable per-call or via theodb.embedding_model GUC.';
