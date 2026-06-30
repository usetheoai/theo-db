# Blueprint — M18: `ai.*` generative surface plpython3u → Rust/pgrx

**Slug:** m18-ai-surface-rust · **Date:** 2026-06-30 · **Milestone:** M18 (ROADMAP-v2)
**Status:** SHIPPABLE (internal-port blueprint)

> **Nature of this discovery (honesty, Rule 3 + Rule 9):** M18 is an **internal port**, not external prior-art
> research. The prior art is (a) the M17 blueprint `pgrx-extension-foundation` (how to build a pgrx HTTP client
> with parity + benchmark) and (b) our own plpython3u source (`sql/50-theodb-ai.sql`) which is the behavioral
> spec to preserve. There is therefore no `knowledge-base/references/{project}` to cite — the four-corner
> external-research model does not apply; the evidence base is our own code (file:line below). This blueprint
> synthesizes the port map into a design + invariants the PLAN phase consumes.

## Problem

Rewrite the 5 `LANGUAGE plpython3u` generative functions in `sql/50-theodb-ai.sql` to Rust/pgrx, at **proven
parity** (the 36-test oracle `benchmarks/tests/test_ai_sql.py` + the deterministic stub `tools/chat_server.py`),
preserving security (REVOKE, SSRF/no-redirect/api_key-no-leak), so the AI layer no longer uses plpython3u.

In scope (port): `ai._chat`, `ai.if`, `ai.analyze_sentiment`, `ai.rank`, `ai.generate_batch`.
Out of scope (stay SQL, keep working): `ai.generate`, `ai.summarize`, `ai._agg_summ_accum`, `ai._agg_summ_final`,
the `ai.agg_summarize` AGGREGATE (all `LANGUAGE sql`, call `ai._chat` by name). The aggregate is **SQL-based**
(not plpython3u) — **no pgrx aggregate needed**. NL (`sql/60/61`) plpython3u is M19, untouched here.

## Patterns (the design)

### P1 — `chat.rs` domain module mirroring `embed.rs` (3-boundary layering, blueprint ADR-1)
A new `theodb_rs/src/chat.rs` (domain) holds `chat(prompt, system, model) -> String` (the raw completion) +
the 4 task parsers. `pg.rs` (glue) and `lib.rs` (api-surface: `#[pg_extern]` + `extension_sql!`) extend as in M17.

### P2 — Generalize the shared HTTP core (DRY, Rule 9 — don't duplicate the client)
`embed.rs::post_json` already does send + bounded retry ({429,502,503}+connect/timeout, `MAX_RETRIES=2`,
`backoff`) + `with_max_redirects(0)` SSRF + non-2xx/non-JSON→38000. Its policy is **identical** to the
plpython3u `ai._chat` retry (`sql/50:28-29,86`). **Move the shared HTTP primitives to an `http.rs` (or keep in
a shared module) parameterized by a `fn_name`/noun** so both `theodb.embed*` and `ai._chat` use one client.
`is_recoverable_status`, `backoff`, `MAX_RETRIES`, `truncate` are reused verbatim. `pg::{err_input,err_external,
warn,guc}` reused verbatim.

### P3 — Chat request/response (the chat-specific delta vs embed)
- GUCs: `theodb.llm_endpoint` (required), `theodb.llm_model` (optional), `theodb.llm_api_key` (optional bearer)
  — `resolve_chat_cfg` mirrors `resolve_cfg` with the same http(s) SSRF guard + `model or guc or "default"`.
- Request body (**byte-identical** to `sql/50:53-58`): `{"model": mdl, "messages": [..]}`; system message appended
  ONLY when `system` is truthy (empty string → no system message); then the user message.
- Response parse: `choices[0].message.content`; missing → 38000 `"unexpected chat response shape (...)"` (body
  truncated 200); `None`/`""` → 38000 `"endpoint returned an empty completion"`.

### P4 — The 4 task wrappers (parse parity — verbatim from the port map)
Each calls `chat(prompt, <system>, model)` then parses. **System prompts MUST be byte-identical** (the stub
routes on them — any drift breaks every offline test):
- `ai.if` → boolean. system=`Answer with exactly one word: yes or no.` Parse: `re.split(r"[^a-z0-9]+", out.strip().lower(),1)[0]`; `{yes,true,1,y,t}`→true, `{no,false,0,n,f}`→false, else **22023** `"ai.if: unparseable boolean from model: <out[:50]>"`.
- `ai.analyze_sentiment` → text. system=`Classify the sentiment of the text. Reply with exactly one of: positive, negative, neutral.` Parse: `re.split(r"[^a-z]+", out.strip().lower(),1)[0]`; in `{positive,negative,neutral}`→that token, else **22023** `"...did not return a known label: <out[:50]>"`.
- `ai.rank` → **real (f32)**. system=`Reply with a single number between 0 and 1 and nothing else.` Parse: first `re.search(r"-?\d+(?:\.\d+)?", out)` → `float`, no clamp; no match → **22023** `"...did not return a number: <out[:50]>"`.
- `ai.generate_batch(text[]) → text[]`. NULL array→22023; empty→`[]` NO call; NULL element→22023 `"...must not contain NULL elements..."`. user=`"\n".join("%d. %s"%(i+1,p))`; system=`"You are given %d numbered items. Respond with ONLY a JSON array of exactly %d strings — the answer to each item, in order. No prose, no markdown."%(n,n)`. Parse: strip leading ```` ```lang ```` (regex `^```[A-Za-z0-9_-]*\s*`) + trailing ```` ``` ```` then `json.loads`; not-list or len≠n → 22023; JSON `null`→SQL NULL element (preserved); non-string → 22023. Error substrings: `"valid JSON"`, `"non-string"`, `"expected a JSON array of N items"`.

### P5 — Keep `ai._chat` SQL-callable (load-bearing invariant)
`ai.generate`/`summarize`/`_agg_summ_final` and M19's `ai.nl_to_sql` call `ai._chat($1,$2,$3)` by name via SPI.
The Rust port MUST expose a SQL function named `ai._chat(text,text,text) RETURNS text` (a `#[pg_extern]` `_chat`
delegate + an `ai._chat` SQL wrapper, exactly like `theodb_rs._embed_text` + `theodb.embed` in `lib.rs:33-77`).

## Trade-offs / ADR seeds

- **ADR-A — generalize `post_json` to a shared `http.rs`** vs duplicate a chat variant. Choose generalize (DRY;
  the retry/SSRF policy is identical). Risk: touching embed's hot path — mitigated by the 13-test embed oracle
  staying green.
- **ADR-B — wrappers call the Rust `chat()` directly** (not via SPI `ai._chat`) for the in-Rust path, while the
  PUBLIC `ai._chat` SQL wrapper still exists for SQL callers. Avoids an SPI round-trip per wrapper call (the
  plpython3u version paid an SPI hop); parity preserved (same prompt+parse). The stub round-trip-count tests
  (`generate_batch` = 1 call) still hold because batch makes ONE `chat()` call.
- **ADR-C — `err_input`/`err_external` errcontext** is hardcoded `"theodb.embed"` (`pg.rs:11,22`). Parameterize
  the context (or add chat variants) so `ai.*` errors don't report `theodb.embed` context. Cosmetic (oracle
  asserts message substrings + SQLSTATE, never context) but correctness-of-record.

## Security invariants (must not regress)

http(s)-only + `with_max_redirects(0)` (no SSRF via redirect to metadata); api_key ONLY in the `Authorization`
header, NEVER in any error/warning string; `REVOKE ALL ... FROM PUBLIC` on every `ai.*` (incl. `_chat`); 30s timeout.

## Benchmark (CTO requirement — measurement-first, ADR 0002)

`benchmarks/bench_chat.py` (mirror `bench_embed.py`): same container, same chat stub, same model — measure
per-call latency of the Rust `ai.generate` (→Rust `ai._chat`) vs the plpython3u `ai._chat` (kept as
`ai._chat_py` for the comparison, like M17's `theodb.embed_py`). Honest framing: chat is **I/O-bound** (the
endpoint dominates) → expected result is **no regression** from the plpython3u→Rust rewrite, NOT a speedup.
Report `docs/benchmarks/m18-ai-rust-vs-plpython.md` (mean±std, ≥3 runs). The N→1 win is already proven for embed;
for chat the value is owning-the-code-in-Rust at parity.

## Coverage corners (adapted for an internal port)

- **Integration tests:** the 36-test `test_ai_sql.py` oracle is the parity gate (stub + real opt-in). New
  benchmark test optional.
- **Dependencies:** none new — reuse `minreq`/`serde_json`/`pgrx` already in `theodb_rs/Cargo.toml` (Rule 9).
- **Tools:** `tools/chat_server.py` stub (exists); `cargo pgrx`, clippy, ruff.
- **Techniques:** the M17 pgrx HTTP-client pattern (embed.rs) generalized to chat + the 4 parse ports above.

## References (internal)

- M17 blueprint: `.claude/knowledge-base/discoveries/blueprints/pgrx-extension-foundation-blueprint.md` (if present) / the shipped `theodb_rs/src/embed.rs`.
- Behavioral spec to preserve: `sql/50-theodb-ai.sql:16-296` (the 5 plpython3u bodies).
- Parity oracle: `benchmarks/tests/test_ai_sql.py` (36 tests) + `tools/chat_server.py`.
