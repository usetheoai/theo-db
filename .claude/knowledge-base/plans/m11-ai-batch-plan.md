---
slug: m11-ai-batch
milestone_id: M11
created_at: 2026-06-28
goal: Ship ai.generate_batch(text[]) that answers N prompts in ONE LLM round-trip, closing docs/features 08
---

# Plan: `ai.generate_batch` — accelerated batch AI calls (feature 08)

> **Version 1.0** — Close `docs/features/08-acelerar-consultas.md`: the scalar `ai.*` make one HTTP
> round-trip per row. This slice adds `ai.generate_batch(text[]) -> text[]` that packs N prompts into a
> SINGLE `ai._chat` call (JSON-array response, length-validated, fail-fast), genuinely cutting N
> round-trips to 1. "Acceleration" is proven by **counting requests** (1 for a batch of N vs N for N
> scalar calls) — a real measurement, not an unbenchmarked latency claim. No new dependency.

## Goal

> Ship `ai.generate_batch(text[]) -> text[]` that answers N prompts in ONE `ai._chat` round-trip,
> measured by an integration test asserting the batch issues exactly 1 HTTP request to the stub (vs N for
> N scalar `ai.generate` calls) and returns N answers in order.

## Context

`docs/features/08-acelerar-consultas.md` documents "accelerated" AI execution. The shipped `ai.*`
(M7-S3) are scalar: `SELECT ai.generate(p) FROM t` issues one HTTP call to the configured chat endpoint
**per row** — N round-trips for N rows. Spec 08's acceleration is reducing those round-trips. The honest
mechanism for an OpenAI-compatible chat endpoint is to pack N prompts into a single request and ask for a
JSON array of N answers (1 round-trip). M11 ships that as `ai.generate_batch`, composed from the existing
private `ai._chat` helper (Rule 9). The deterministic stub (`tools/chat_server.py`) gains a request
counter so the round-trip reduction is a measured contract, and a JSON-array reply branch.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/50-theodb-ai.sql` | ~245 | M10 (2026-06-28) | scalar `ai.*` + `ai._chat` + `ai.agg_summarize`; baked via initdb.d | `ai._chat` + all existing fns unchanged; new fn additive + idempotent (CREATE OR REPLACE) |
| `tools/chat_server.py` | ~154 | M7-S3 (2026-06-28) | deterministic OpenAI-compatible stub | existing `_decide` branches unchanged; counter + json-array branch are additive |
| `benchmarks/tests/test_ai_sql.py` | ~330 | M10 (2026-06-28) | contract tests vs stub + real-OpenAI opt-in | existing tests stay green; batch tests appended |
| `smoke.sh` | ~100 | M10 | presence/privilege smoke | add `ai.generate_batch` presence check |
| `docs/sql-ai-functions.md` | ~190 | M10 | the ai.* doc | add an `ai.generate_batch` section |
| `CHANGELOG.md` | (exists) | — | public contract | `[Unreleased]` gets the M11 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `ai._chat(prompt, system, model)` (private, plpython3u) in `sql/50-theodb-ai.sql`.
  - **Callers:** the 5 scalar wrappers + `ai._agg_summ_final` (M10). `ai.generate_batch` becomes one more caller (ONE call per batch).
  - **External:** baked into `theo-db:dev`; reached over HTTP. Additive — no existing object changes.
- **`tools/chat_server.py`** consumed by `benchmarks/tests/test_ai_sql.py` (the `chat_server` fixture). The counter is read by the new round-trip test via a `GET /count` endpoint.
- Enumerated via `grep -nE 'ai\._chat|_decide|do_GET|do_POST' sql/50-theodb-ai.sql tools/chat_server.py`.

### Domain glossary

- **round-trip** — one HTTP request to the chat endpoint. Scalar `ai.generate` = 1 per row; batch = 1 per call.
- **`ai._chat`** — the single private HTTP helper (SSRF-guarded, typed fail-fast errors, REVOKE FROM PUBLIC).
- **JSON-array contract** — the batch asks the model for `["a1","a2",…,"aN"]`; the function validates `len == N`.
- **request counter (stub)** — a thread-safe counter in `chat_server.py` exposed at `GET /count`, used to measure round-trips deterministically (the stub runs under `ThreadingHTTPServer`).

### Architecture boundaries affected

Per `rules/architecture.md`: the change is within the **AI SQL surface** (`sql/50`) — an interface-layer
capability over the `ai._chat` adapter — plus **test infrastructure** (`tools/chat_server.py`). No new
layer, no new dependency, no production HTTP path beyond the existing `ai._chat`.

## Prior Art & Related Work

- **Internal (pattern to mirror):** `ai.rank` / `ai.if` (plpython3u fns that call `ai._chat` via `plpy` + parse output) and `ai._chat` itself; `ai.agg_summarize` (M10 — the other "many → one call" surface); `benchmarks/tests/test_ai_sql.py` (stub + real-OpenAI pattern); `tools/chat_server.py` (`_decide` seams).
- **Internal (discovery):** `knowledge-base/discoveries/blueprints/alloydb-vector-ai-implementation-blueprint.md` (the `ai.*` surface TheoDB mirrors) + `m7-*` blueprints.
- **External:** OpenAI chat-completions API (one conversation per request; batching = packing items into one request) — `https://platform.openai.com/docs/api-reference/chat`.
- **Reference:** `.claude/knowledge-base/references/` (when present).

## Objective

- [ ] `ai.generate_batch(prompts text[], model text DEFAULT NULL) -> text[]`: ONE `ai._chat` call packing N numbered prompts; parse a JSON array; **validate `len == N`** (fail-fast 22023 on mismatch/invalid JSON).
- [ ] Empty array input → empty array output, **no LLM call** (cost safety). NULL array or any NULL element → typed 22023 (preserve the N-in-N-out alignment contract).
- [ ] `REVOKE ALL ... FROM PUBLIC` (parity with the scalar `ai.*`).
- [ ] Stub: thread-safe request counter + `GET /count` + a JSON-array reply branch (test infra).
- [ ] Integration test proves the **round-trip reduction**: batch of N → counter delta == 1; N scalar calls → delta == N; answers returned in order. Plus real-OpenAI evidence.

## ADRs

### D1 — Batch = one packed request returning a JSON array (the real round-trip reduction)

**Decision:** `ai.generate_batch` builds ONE prompt that numbers the N items and a system instruction to
return ONLY a JSON array of exactly N strings; it calls `ai._chat` once and parses+validates the array.

**Rationale:** An OpenAI-compatible chat endpoint processes one conversation per request; the honest way
to cut N round-trips to 1 is to pack the items into a single request (Rule 9 — reuse `ai._chat`; no new
endpoint). This is the essential mechanism of spec-08 "acceleration"; the win is measurable as request
count, satisfying Rule 5 (claim ⇒ measurement) without an unbenchmarked latency claim.

**Alternatives considered:** *Convenience wrapper that calls the scalar N times* — rejected: zero
round-trip reduction; calling it "accelerate" would be dishonest (Rule 3). *OpenAI async Batch API* —
rejected: different async endpoint + polling; heavy, out of scope, no current demand (YAGNI). *Server-side
parallel scalar calls* — rejected: no plpython async story; complexity without the single-request win.

**Consequences:** the batch depends on the model returning valid JSON of length N — a best-effort contract
with fail-fast validation (D2); for guaranteed per-item delivery the caller uses the scalar `ai.generate`.

### D2 — Strict length/JSON validation, fail-fast; only `ai.generate` batched (YAGNI)

**Decision:** Parse the reply as JSON; if it is not a list of exactly N strings (after stripping an
optional ```json fence), raise `22023` with a clear message. Ship ONLY `ai.generate_batch` (not batched
if/rank/sentiment).

**Rationale:** A silent partial/misaligned result is worse than a typed failure (Rule 8 — fail-fast,
typed). One representative batched function closes spec 08's "≥ 1 accelerated function" honestly; batching
the other four is YAGNI until demanded (the roadmap flags this explicitly).

**Alternatives considered:** *Best-effort padding/truncation to N* — rejected: silent corruption of the
caller's row alignment. *Batch all five ai.\** — rejected: YAGNI; quadruples surface with no demand.

**Consequences:** length-mismatch from a flaky model surfaces as a typed error the caller can retry; only
`generate` is accelerated this slice (documented).

### D3 — Two test layers + a request-counter measurement (deterministic + real)

**Decision:** Reuse the M7-S3/M10 test pattern (offline stub default + real-OpenAI opt-in). Add a
thread-safe request counter to the stub (`GET /count`) so the offline test measures round-trips by delta.

**Rationale:** the acceleration claim needs evidence; counting requests is the deterministic, zero-cost
measurement (Rule 5). The real-OpenAI test provides functional evidence (mandate).

**Alternatives considered:** *Assert latency* — rejected: non-deterministic, flaky, costs money. *No
measurement* — rejected: then "accelerate" is an unbenchmarked claim (forbidden).

**Consequences:** the stub gains a counter (additive, lock-guarded for `ThreadingHTTPServer`).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Model returns invalid JSON / wrong length | Medium | strict parse + `len==N` check → typed 22023 (D2); caller retries or falls back to scalar | AI |
| Large batch exceeds token limit (one big request) | Medium | documented: caller chunks; batch is best-effort; per-item guarantee via scalar | AI |
| One batch failure loses all N answers (vs scalar per-row) | Medium | typed fail-fast (no partial corruption); documented trade-off; scalar remains for per-row resilience | AI |
| Stub counter race under ThreadingHTTPServer | Low | counter guarded by a `threading.Lock` (race-aware); product code is single-threaded | AI |

## Unresolved Questions

- Q1 — Batch `ai.if`/`ai.rank`/`ai.analyze_sentiment` too? Resolved: no — YAGNI (D2); `ai.generate_batch` is the representative; others deferred until demanded.
- Q2 — Auto-chunk oversized batches? Resolved at plan time: no — caller chunks; documented limit (D2). Auto-chunking is future work with no current demand.

## Dependencies

M11 adds **no new dependency** (Unbreakable Rule 9). It composes the shipped `ai._chat`; the stub uses
only the Python stdlib (`json`, `threading`, `http.server`).

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `plpython3u` + `ai._chat` | shipped (M7-S3) | the HTTP helper the batch calls once | PostgreSQL License | already shipped; no change |
| Python stdlib (`json`, `threading`) | runtime | stub counter + JSON parse | PSF | stdlib; no dep |

No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (ai.generate_batch + stub counter/branch + tests) ──▶ Phase 2 (doc + real evidence + smoke + CHANGELOG)
```

## Phase 1: `ai.generate_batch` + stub support + tests

**Objective:** Ship the batched function + measure the round-trip reduction deterministically.

### T1.1 — `ai.generate_batch` (plpython3u) + REVOKE + stub counter/json-array + tests

#### Objective
Add `ai.generate_batch` to `sql/50`, add the stub request counter + JSON-array branch, and prove batch correctness + the 1-vs-N round-trip reduction.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — appends `ai.generate_batch(prompts text[], model text DEFAULT NULL) -> text[]`
   (plpython3u: validate, pack N numbered prompts, ONE `ai._chat` call, parse+validate a JSON array of N,
   return text[]) + `REVOKE ... FROM PUBLIC`; adds a thread-safe counter + `GET /count` + a `"json array"`
   reply branch to the stub; adds integration tests (correctness, ordering, round-trip count, negatives).

2. **Why it is necessary now** — it is the feature: spec-08 acceleration as a real round-trip reduction,
   proven by the request counter (Rule 5), composed from the shipped `ai._chat` (Rule 9).

#### Evidence
- plpython3u + ply.execute pattern: `sql/50-theodb-ai.sql` `ai.rank`/`ai.if` (call `ai._chat` via `plpy.prepare`).
- Stub shape + `_decide` seams: `tools/chat_server.py` (lines 29-87 `_decide`; do_GET line ~104; do_POST line ~108).
- Test + fixture pattern: `benchmarks/tests/test_ai_sql.py` (`chat_server` fixture yields the URL; `conn` fixture).

#### Files to edit
```
sql/50-theodb-ai.sql — add ai.generate_batch (plpython3u) + REVOKE
tools/chat_server.py — thread-safe request counter + GET /count + "json array" reply branch
benchmarks/tests/test_ai_sql.py — RED batch tests (correctness, ordering, round-trip count, negatives)
```

#### Deep file dependency analysis
- `sql/50` (Baseline row, invariant: existing fns + `ai._chat` unchanged): additive plpython3u fn + a REVOKE line.
- `tools/chat_server.py` (Baseline row, invariant: existing `_decide` branches unchanged): additive counter (module-level, lock-guarded), a `GET /count` route, and a `_decide` branch for the json-array system prompt that returns N canned answers (N parsed from the numbered user message).
- `test_ai_sql.py` (Baseline row, invariant: existing tests green): appends batch tests using the existing fixtures + a small `urllib` GET to `/count`.

#### Deep Dives
- **Function:** validate `prompts IS NOT NULL` (else 22023); `n = array_length`; if `n == 0` return `{}` (no call); if any element NULL → 22023 (alignment contract). Pack `user = "\n".join(f"{i+1}. {p}")`; `system = "You are given N numbered items. Respond with ONLY a JSON array of exactly N strings, the answer to each item in order. No prose, no markdown."` (contains "json array" for the stub). Call `ai._chat(user, system, model)` once via `plpy`. Strip an optional ```json fence; `json.loads`; require `list` of `len == n` → else 22023. Return the list (→ text[]).
- **Stub counter:** module-level `_count` + `threading.Lock`; increment inside do_POST for chat-completions only; `GET /count` returns `{"count": n}` under the lock. The `"json array"` branch: count numbered lines (`^\s*\d+\.`) in the user msg → return `json.dumps(["answer to item %d" % i for i in 1..N])`.
- **Edge cases:** empty array → `{}` no call; single prompt → array of 1; NULL element → 22023; non-JSON / wrong-length reply → 22023.

#### Pseudo-code / Signatures
```python
# ai.generate_batch(prompts text[], model text DEFAULT NULL) RETURNS text[]  (plpython3u)
if prompts is None: plpy.error("ai.generate_batch: prompts must not be NULL", sqlstate="22023")
n = len(prompts)
if n == 0: return []
if any(p is None for p in prompts): plpy.error("...: no NULL elements (breaks N-in-N-out)", sqlstate="22023")
user = "\n".join("%d. %s" % (i+1, p) for i,p in enumerate(prompts))
system = "You are given N numbered items. Respond with ONLY a JSON array of exactly N strings ... No prose, no markdown."
out = plpy.execute(plpy.prepare("SELECT ai._chat($1,$2,$3) AS v",["text","text","text"]),[user,system,model])[0]["v"]
s = out.strip(); s = strip_json_fence(s)
arr = json.loads(s)  # ValueError -> 22023
if not isinstance(arr, list) or len(arr) != n: plpy.error("...: expected JSON array of %d, got ..." % n, sqlstate="22023")
return [str(x) for x in arr]
```

#### Tasks
1. Append `ai.generate_batch` + REVOKE to `sql/50`.
2. Add counter + `GET /count` + json-array branch to `tools/chat_server.py`.
3. Rebuild `theo-db:dev`; fresh container (with `--add-host`).
4. Append batch tests to `test_ai_sql.py`.

#### TDD
```
RED:     test_generate_batch_one_roundtrip() [integration] — seed 3 prompts; read GET /count; SELECT ai.generate_batch(ARRAY[...]); read /count again; assert delta == 1 AND len(result) == 3 AND answers in order. MUST fail before the function + stub branch exist.
GREEN:   Implement the function + stub support so it passes.
REFACTOR: factor the json-fence strip if it helps; else "None expected".
VERIFY:  PG*=... pytest -m integration tests/test_ai_sql.py -k 'batch and not real' -q
```

#### Concurrency tests

**Concurrency posture:** the product fn `ai.generate_batch` is single-threaded (one query → one `ai._chat`
call → sequential parse). The only concurrency is the **stub's request counter**, which runs under
`ThreadingHTTPServer` and is guarded by a `threading.Lock`. A **concurrent test** (`test_stub_counter_is_threadsafe`)
fires K parallel HTTP requests at the stub from a thread pool and asserts `GET /count` advanced by exactly
K — proving the Lock prevents lost updates, so the round-trip measurement (delta==1 for a batch) is
reliable. (The atomic-counter invariant: every accepted request increments the counter exactly once.)

#### Acceptance Criteria
- [ ] Round-trip reduction proven — `PGHOST=... pytest -m integration tests/test_ai_sql.py -k 'batch_one_roundtrip' -q` exits `0` (batch of 3 → `/count` delta `1`; result length `3`, in order).
- [ ] N scalar calls = N round-trips (contrast) — the same test (or a sibling) asserts 3 `ai.generate` calls → `/count` delta `3`.
- [ ] `ai.generate_batch` REVOKED from PUBLIC — `psql -tAc "SELECT has_function_privilege('public','ai.generate_batch(text[],text)','execute')"` returns `f`.
- [ ] Negatives typed — empty array → `{}` (no call); NULL element → SQLSTATE 22023; wrong-length/invalid-JSON reply → 22023 (asserted via stub seams) — `pytest ... -k 'batch' -q` exits `0`.
- [ ] No regression — `pytest -m integration tests/test_ai_sql.py -k 'not real' -q` exits `0`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Batch green on the rebuilt image; round-trip delta measured `1` — `pytest ... -k 'batch and not real' -q` exits `0`.
- [ ] REVOKE FROM PUBLIC verified — `has_function_privilege` returns `f`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'generate_batch' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Phase 2: Doc + real-OpenAI evidence + smoke + CHANGELOG

**Objective:** Document the batch fn, capture real-OpenAI evidence, add smoke + CHANGELOG.

### T2.1 — `docs/sql-ai-functions.md` section + real evidence + smoke + CHANGELOG

#### Objective
Document `ai.generate_batch`, run a real-OpenAI evidence test, add the smoke presence check + CHANGELOG entry.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds an `ai.generate_batch` section (usage + round-trip note + JSON contract +
   fail-fast + dual-grant) to `docs/sql-ai-functions.md`; runs the opt-in real-OpenAI batch test and records
   output in the implementation log; adds an `ai.generate_batch` presence line to `smoke.sh`; CHANGELOG entry.

2. **Why it is necessary now** — the mandate requires real functional evidence; the doc makes the surface
   usable; the smoke guards presence in the baked image.

#### Evidence
- Doc format: `docs/sql-ai-functions.md` (existing ai.* sections + the M10 aggregate section).
- Real-test pattern: `benchmarks/tests/test_ai_sql.py` `-k real`.
- Smoke pattern: `smoke.sh` ai.* presence/privilege checks.

#### Files to edit
```
docs/sql-ai-functions.md — ai.generate_batch section (round-trip reduction, JSON contract, limits)
benchmarks/tests/test_ai_sql.py — real-OpenAI batch evidence test (-k real, skips cleanly)
smoke.sh — ai.generate_batch presence/privilege check
CHANGELOG.md — [Unreleased] M11 entry
```

#### Deep file dependency analysis
- `docs/sql-ai-functions.md` (Baseline row): additive section.
- `smoke.sh` (Baseline row): additive presence/privilege check (same shape as the ai.* / agg checks).
- `test_ai_sql.py`: appends one `-k real` test (skips without `.env`).

#### Deep Dives
- **Real evidence honesty:** the real test asserts shape (N answers for N prompts, non-empty), never exact text; the model output is recorded in the implementation log.

#### Tasks
1. Write the doc section.
2. Run the real-OpenAI batch test; record output in the implementation log.
3. Add the smoke check + CHANGELOG entry.

#### TDD
```
RED:     real test absent / doc section absent / smoke check absent.
GREEN:   `grep -c generate_batch docs/sql-ai-functions.md smoke.sh` > 0; real test present (skips cleanly without .env).
REFACTOR: none expected.
VERIFY:  grep -c 'generate_batch' docs/sql-ai-functions.md smoke.sh && pytest -m integration tests/test_ai_sql.py -k 'batch and real' -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — docs + a sequential real test; no concurrent product state.

#### Acceptance Criteria
- [ ] Doc section present — `grep -ci 'generate_batch' docs/sql-ai-functions.md` returns `> 0`.
- [ ] Smoke presence check present — `grep -c 'generate_batch' smoke.sh` returns `> 0`.
- [ ] Real-OpenAI evidence recorded — `grep -ci 'generate_batch' knowledge-base/implementations/m11-ai-batch-implementation.md` returns `> 0`.
- [ ] No unbenchmarked perf claim — `grep -ciE 'faster than|outperforms|x faster' docs/sql-ai-functions.md` returns `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Doc + smoke + real-evidence committed — `grep -c generate_batch docs/sql-ai-functions.md smoke.sh` returns `> 0`.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'generate_batch' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | feature 08 accelerated batch shipped | T1.1, T2.1 | `ai.generate_batch(text[])` — 1 round-trip for N prompts |
| 2 | round-trip reduction MEASURED (not claimed) | T1.1 | stub counter; batch delta==1 vs scalar delta==N (Rule 5) |
| 3 | reuse `ai._chat` (Rule 9, no new dep) | T1.1 | one `ai._chat` call (per ADR D1) |
| 4 | REVOKE FROM PUBLIC (security parity) | T1.1 | REVOKE on `ai.generate_batch` |
| 5 | typed fail-fast (invalid JSON / wrong length / NULL elem) | T1.1 | 22023 validation (per ADR D2) |
| 6 | empty input safe (no LLM call) | T1.1 | empty array → `{}` no call |
| 7 | real functional evidence (not stub-only) | T2.1 | real-OpenAI batch test + logged output (per ADR D3) |
| 8 | no regression to scalar ai.* / agg | T1.1 | existing ai.* tests green |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed — every phase DoD above exits `0`.
- [ ] AI suite green — `PGHOST=... pytest -m integration tests/test_ai_sql.py -k 'not real' -q` exits `0` (no regression).
- [ ] Round-trip reduction measured — batch of N → stub `/count` delta `1` (vs N scalar → `N`).
- [ ] `ai.generate_batch` REVOKED from PUBLIC — `has_function_privilege('public','ai.generate_batch(text[],text)','execute')` returns `f`.
- [ ] Real-OpenAI evidence captured — implementation log has the real batch output (or documented clean skip).
- [ ] File-size budget respected — changed files `wc -l` < `500` (per `rules/architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` — `grep -c 'generate_batch' CHANGELOG.md` returns `> 0` (Unbreakable Rule 6).
- [ ] Backward compatibility preserved — scalar `ai.*` + `ai._chat` + `ai.agg_summarize` unchanged; stub `_decide` branches unchanged.
- [ ] No new dependency (Rule 9) — image + stub use only what is already present.
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| chat endpoint (`ai._chat`, HTTP) | endpoint unset | call batch with no `theodb.llm_endpoint` | typed `22023` ("endpoint is not set") from `ai._chat` |
| chat endpoint | call fails / down | point at a dead port | typed `38000` ("chat endpoint call failed") propagated |
| model reply | not a JSON array / wrong length | stub seam returns prose / wrong-N array | typed `22023` ("expected JSON array of N") — fail-fast, no partial result |
| model reply | empty completion | stub `__EMPTY__` seam | typed `38000` ("empty completion") from `ai._chat` |
| input | empty array | batch over `ARRAY[]::text[]` | `{}` returned, NO LLM call (stub `/count` unchanged) |
| input | NULL element in array | `ARRAY['a',NULL]` | typed `22023` (alignment contract) |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate `ai.generate_batch` end-to-end against a freshly-baked container.

### Execution
```
docker build -t theo-db:dev .                                   # bake updated sql/50
docker run -d --name m11-it --add-host=host.docker.internal:host-gateway \
  -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev      # wait healthy
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_ai_sql.py -k 'batch and not real' -q     # stub + round-trip count
# real evidence (opt-in; loads .env):
THEODB_LLM_ENDPOINT=... OPENAI_API_KEY=... PG*=... \
  pytest -m integration tests/test_ai_sql.py -k 'batch and real' -q
```

### Acceptance Criteria
- [ ] Batch green on the rebuilt image; round-trip delta `1`; N answers in order — `PGHOST=... pytest -m integration tests/test_ai_sql.py -k 'batch and not real' -q` exits `0`.
- [ ] Negatives typed (empty→{} no call; NULL elem→22023; wrong-length→22023) + REVOKE FROM PUBLIC — `psql -tAc "SELECT has_function_privilege('public','ai.generate_batch(text[],text)','execute')"` returns `f`.
- [ ] Real-OpenAI evidence captured (or documented clean skip) — `grep -ci 'generate_batch' knowledge-base/implementations/m11-ai-batch-implementation.md` returns `> 0`.
- [ ] No regression to scalar ai.* / ai.agg_summarize — `PGHOST=... pytest -m integration tests/test_ai_sql.py -k 'not real' -q` exits `0`.
