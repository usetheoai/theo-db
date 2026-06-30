# Review: m18-ai-surface-rust

**Date:** 2026-06-30
**Reviewers (spawned agents):** 5 — architecture, tests, wiring, cross-validation, domain (PostgreSQL/pgrx/SSRF/parse)
**Diff base:** `84b64956..develop` (M18 — port the 5 plpython3u ai.* generative functions to Rust/pgrx)
**Findings:** 0 BLOCKER, 0 HIGH, 2 MEDIUM, ~8 LOW, several INFO
**Verdict:** **READY_TO_MERGE** (after fixes below; re-verified against the rebuilt `theo-db:m18`)

## MEDIUM (fixed)

### TESTS-M18-01 — parser edge behaviors untested at any layer
The pure parsers (`first_token`/`first_number`/`strip_fence`/`parse_batch`) were correct (verified by DOM-06
counterexamples) but several documented edges were exercised by NOTHING (the stub never emitted those shapes):
JSON-null array element → SQL NULL, `ai.rank` no-clamp (>1), whole-array NULL → 22023, null content.
- **Fixed:** added 3 stub seams (`__bignum__`, `__jsonnull__`, `__nullcontent__`) to `tools/chat_server.py`
  + a new `benchmarks/tests/test_ai_edge.py` (4 tests) — all green against `theo-db:m18`. The frozen oracle
  `test_ai_sql.py` is untouched (the new edges live in a separate file).

### F1 (cross-val) — DoD "AI layer no longer requires plpython3u" met at the sql/50 layer only
`sql/50` is plpython3u-free (grep = 0), but `theodb.control` still `requires plpython3u` because `sql/60` (NL)
is plpython3u — that is **M19** scope, explicitly. **By design, documented** (the plan goal scopes M18 to
"zero plpython3u in sql/50"; M19 owns the `requires` removal). The CHANGELOG accurately says "não **usa** mais
plpython3u" (does not *use*), not "não requer". **No code change** — awareness item for the release.

## LOW (fixed)

- **DOM-04** — empty-string `model` arg not treated as falsy (Python `model or …` parity). **Fixed:**
  `model.filter(|s| !s.is_empty())` in `resolve_chat_cfg` (`ai.generate('hi','')` now falls back like plpython3u).
- **DOM-05 / TESTS-M18-02** — JSON-null `content` produced "unexpected shape" instead of "empty completion"
  (same SQLSTATE 38000, different message). **Fixed:** `chat()` now matches `Value::String(non-empty)` /
  `String("")|Null` → "empty completion" / else → "unexpected shape" (message parity); covered by
  `test_ai_edge.py::test_null_content_is_empty_completion`.
- **ARCH-01 / DOM-10 / TESTS-M18-04** — `pg.rs` errcontext hardcoded `"theodb.embed"` mislabeled ai.* errors
  (ADR-C, a committed plan task). **Fixed:** neutral context `"theodb"` (the message already carries the real
  function name; oracle never asserts the context). Minimal, accurate for both embed + ai.
- **W-05** — migration header "no-op on fresh install" wording. **Verified accurate** (the regenerated 1.0 base
  has zero plpython3u ai.*, so the fresh-extension-path drop no-ops; W-05 read a stale gitignored local
  `theodb--1.0.sql`). **Clarified** the comment + added the **DOM-03** in-place upgrade ordering note.

## LOW (accepted, no change — documented rationale)

- **ARCH-02** — SSRF scheme guard + GUC resolution duplicated across `resolve_cfg`/`resolve_chat_cfg`.
  Defensible under the Rule-of-3 (2 instances, distinct GUC namespaces); flagged to extract a shared
  `resolve_endpoint_cfg` when M19 adds a 3rd caller or any IP-level SSRF rule lands.
- **W-06** — the 5 chat externs have no Rust `#[pg_test]` (only embed does). Integration coverage is present
  (`test_ai_sql.py` 33/33 + `test_ai_edge.py`); the Rust `#[pg_test]` harness cannot run in the build
  container anyway (`initdb cannot run as root`, M17 precedent). Test-depth asymmetry, not a wiring gap.
- **TESTS-M18-03** — batch error-message tail formatted JSON vs Python repr; substrings + SQLSTATE preserved.
- **F2/F3** — plan label inaccuracies ("36/36" vs 33+3-skipped; "31-test embed oracle" vs 18/20). Cosmetic;
  every AC oracle resolves to a real test.

## Edge-case coverage

EC parity (system prompts byte-identical — verified vs `git show 84b64956:sql/50`), parse parity (DOM-06
counterexamples: `1.2.3`→1.2, `-3.5`, leading-punct→22023, fence-strip, JSON-null→NULL), security (SSRF
http(s)+no-redirect+api_key-no-leak preserved, REVOKE union == original). New `test_ai_edge.py` adds the
previously-untested edges.

## Cross-validation summary

All 4 tasks (T0.1–T3.1) PASS; Coverage Matrix 11/11; `milestone_id: M18` in the plan frontmatter (release flip
will work); no fabricated AC oracle; no silent divergence. ROADMAP M18 DoD: (1) parity ✓, (2) REVOKE/SSRF ✓,
(3) AI-layer plpython3u-free at sql/50 ✓ (requires-removal = M19, documented).

## Quality gates summary

- Full suite vs `theo-db:m18`: **69 passed, 3 skipped** (opt-in real-OpenAI) — `test_ai_sql.py` 33 +
  `test_ai_edge.py` 4 + `test_ai_retirement.py` 3 + embed oracle 20 + `test_extension_install.py` 9. Zero flaky
  (the pre-existing stub thread-safety flake fixed via `request_queue_size=256`).
- `cargo clippy --features pg17 -- -D warnings`: clean (re-verified after the review fixes). `ruff`: clean.
  `cargo pgrx install --release`: compiles. `/code-quality`: PASS.
- Benchmark: Rust `ai._chat` 1.447±0.269 ms vs plpython3u 1.995±0.059 ms — no regression (I/O-bound).
- Fresh install theodb 1.2; zero `LANGUAGE plpython3u` in sql/50; wiring triad complete per new symbol.

## Handoff decision

**READY_TO_MERGE.** No unresolved BLOCKER/HIGH; both MEDIUM and the substantive LOW findings fixed and
re-verified; the accepted LOWs carry documented rationale.
