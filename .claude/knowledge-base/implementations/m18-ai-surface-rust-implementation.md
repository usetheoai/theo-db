---
slug: m18-ai-surface-rust
milestone_id: M18
date: 2026-06-30
verdict: IMPLEMENTATION_COMPLETE
plan: .claude/knowledge-base/plans/m18-ai-surface-rust-plan.md
---

# Implementation summary — M18: `ai.*` generative surface plpython3u → Rust/pgrx

Ports the 5 plpython3u generative functions (`ai._chat`, `ai.if`, `ai.analyze_sentiment`, `ai.rank`,
`ai.generate_batch`) to Rust/pgrx (theodb_rs), reusing the M17 HTTP client, at proven parity. ROADMAP-v2 M18
(depends on M17). milestone_id: M18 → `cycle-release` flips the ROADMAP checkbox post-merge.

## Evidence (against the rebuilt image `theo-db:m18`)

- **Fresh install lands clean at theodb 1.2**: `theodb|1.2`, `theodb_rs|1.0.0`. The `ai.*` surface is
  Rust-backed (`ai._chat`/`if`/`analyze_sentiment`/`rank`/`generate_batch` are LANGUAGE sql wrappers over
  `theodb_rs._ai_*`); the keepers `ai.generate`/`summarize`/`_agg_summ_final` are plpgsql (late-bound).
- **Full suite 69 passed / 3 skipped** (test_ai_sql 33 + test_ai_edge 4 + test_ai_retirement 3 + embed 20 + extension_install 9). **`test_ai_sql.py` 33/33 offline PASS** (3 real-OpenAI tests skipped — opt-in) — full parity: generate / if
  / analyze_sentiment / rank / summarize / agg_summarize / generate_batch + every malformed / SSRF / empty /
  bad-shape / REVOKE / round-trip-count case.
- **Embed regression: 20/20 PASS** (`test_embed_sql` 10 + `test_embed_failure_scenarios` 3 + `test_embed_batch`
  7) — the `http.rs` extraction did NOT regress the embed client.
- **`test_ai_retirement.py` 3/3 PASS** — the REAL 1.1→1.2 upgrade path (v0.17-shaped install with plpython3u
  ai.* members + an SQL `ai.generate` depending on `ai._chat`): the migration re-defines the keepers as
  plpgsql (breaking the dependency), DROPs all 5 legacy plpython3u ai.*, then `CREATE EXTENSION theodb_rs`
  installs with no clash; the guard spares theodb_rs-owned ai.*; default_version=1.2.
- **`test_extension_install.py` PASS** — fresh CREATE both extensions; full ai.* surface present; extversion 1.2.
- **Zero `LANGUAGE plpython3u` in `sql/50`** (`grep -c` = 0).
- **Benchmark (`docs/benchmarks/m18-ai-rust-vs-plpython.md`):** Rust `ai._chat` 1.447±0.269 ms/call vs
  plpython3u `ai._chat_py` 1.995±0.059 ms/call — delta −0.547 ms/call; **no regression** (I/O-bound; not a
  perf claim).
- **`cargo clippy --features pg17 -- -D warnings`: (verified in CODE-QUALITY).** **`ruff`: clean.** **`cargo
  pgrx install --release` (image build): compiles.**
- **Known flaky (pre-existing, NOT M18):** `test_ai_sql.py::test_stub_counter_is_threadsafe` (a thread-safety
  test of the stub `tools/chat_server.py`) can `ConnectionResetError` under full-parallel-suite load (heavy
  concurrent ThreadingHTTPServers + DB create/drop); passes 3/3 in isolation and when `test_ai_sql.py` runs
  alone. It tests the stub, not the Rust port. Flagged for review.

## Tasks + wiring triad

| Task | Change | Caller (a) | Integration test (b) | Observability (c) |
|---|---|---|---|---|
| T0.1 | extract shared HTTP client → `http.rs` | `embed.rs` + `chat.rs` both use `crate::http::post_json` | embed 20-test oracle (no regression) | typed 38000; retry WARNING |
| T1.1 | `chat.rs` + 5 Rust externs + `ai.*` SQL wrappers | `ai.*` → `theodb_rs._ai_*` → `crate::chat` | `test_ai_sql.py` 33/33 | typed 22023/38000; retry WARNING; REVOKE |
| T2.1 | sql/50 plpython3u removed; keepers plpgsql; 1.1→1.2 retirement; default_version 1.2 | `ALTER EXTENSION theodb UPDATE` / fresh CREATE | `test_ai_retirement.py` 3/3 + `test_extension_install.py` | `RAISE NOTICE` on drop |
| T3.1 | `bench_chat.py` + report | `python3 bench_chat.py` | smoke (finite numbers) | the report |

## Coverage Matrix

11/11 requirements: ai._chat/if/analyze_sentiment/rank/generate_batch Rust+parity (T1.1); DRY http.rs no embed
regression (T0.1); REVOKE+SSRF+no-leak (T1.1); zero plpython3u in sql/50 (T2.1); keepers late-bound (T2.1);
1.1→1.2 retirement (T2.1); benchmark (T3.1).

## Commits

PLAN `eb4b974`; IMPLEMENT (this slice — http.rs/chat.rs/lib.rs/sql50/migration/control/tests/bench).

## Backward compatibility

`ai.*` signatures / RETURNS / VOLATILE unchanged; `ai._chat` SQL-callable (ai.generate/summarize/aggregate +
M19 nl depend on it); aggregate volatility ('v' finalfunc) preserved; embed surface unchanged.
