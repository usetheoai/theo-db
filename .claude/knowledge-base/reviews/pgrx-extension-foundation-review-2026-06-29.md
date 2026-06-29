# Review — pgrx-extension-foundation (M17)

**Date:** 2026-06-29
**Slug:** pgrx-extension-foundation · **Milestone:** M17 (ROADMAP-v2 / ADR 0006)
**Commits reviewed:** 7f41448 (discover), 71db26a (plan), 386d073 (impl), 8a9a5dc (docs), 03575f3 (review fixes)
**Verdict:** **READY_TO_MERGE**

## Scope

`theodb.embed` rewritten from plpython3u to TheoDB's own Rust pgrx extension `theodb_rs`, at proven
functional parity, with a reproducible latency benchmark. First own-Rust extension (ROADMAP-v2 foundation).

## Specialist agents + verdicts

| Agent | Focus | Verdict |
|---|---|---|
| architecture-reviewer | coexistence, schema ownership, DROP ordering, DIP, Docker caching | **ARCHITECTURE_OK** |
| security-reviewer | SSRF/no-redirect parity, typed errors, least-privilege, TLS, injection, CVE claims | **SECURITY_OK** |
| test-auditor / cross-validation | oracle coverage, bench harness, plan↔impl consistency, over-claiming | **TESTS_OK** (after fixes) |

## Findings + resolution

| # | Sev | Finding | Resolution |
|---|---|---|---|
| H1 | HIGH | Failure-scenarios (redirect-SSRF / 4xx / empty-content) claimed "exercised" (Coverage 100%) but had no test | **FIXED** — `benchmarks/tests/test_embed_failure_scenarios.py` (3 tests, green vs real image): redirect→internal NOT followed (38000), 4xx→38000, empty-content→384-dim. Suite now 18 green (10+3+5). |
| H2 | HIGH | Plan said 4xx→22023; code does 38000 (contradiction) | **FIXED** — plan v1.4 corrected throughout (INPUT→22023, HTTP failures→38000), each Failure-scenarios row now cites a real green test; re-scored SHIPPABLE_WITH_CAVEATS 83.2. |
| M1 | MEDIUM | `DROP EXTENSION theodb_rs` silently degrades `ai.hybrid_search_rrf` embed leg (late-bound, no pg_depend) | **RECORDED** as M19 transition debt (implementation summary § Transition-state debt). Inherent to the D1 split; not an M17 regression (embed did not exist standalone before). |
| M2 | MEDIUM | 4 Rust `#[pg_test]`s don't run (need pgrx-managed PG) | **ACCEPTED + disclosed** — superseded by the Python oracle against the real shipped extension (blueprint's authoritative gate); honestly recorded. |
| L1 | LOW | api-key out-of-band security guidance dropped from COMMENT | **FIXED** — restored (verified present in deployed function). |
| L2 | LOW | Stale Dockerfile comments (theodb.embed/plpython3u; ca-certificates) | **FIXED**. |
| L3 | LOW | Pre-existing destination-host SSRF (GUC can point at internal IP directly) — parity with baseline | **NOTED** as future hardening (host/IP denylist); unchanged from baseline; mitigated by REVOKE-from-PUBLIC + admin-configures-endpoint. |

## Hard gates (BLOCKER-level) — all pass

- ✅ Tests green on develop: 18 passed (10 oracle + 3 failure-scenarios + 5 bench-unit) vs `theo-db:m17`.
- ✅ No new secrets committed (api-key is a session GUC, never in code/logs; `.env` untouched).
- ✅ No direct commit to `main` (all on `develop`).
- ✅ No Co-Authored-By trailer on any M17 commit.
- ✅ CHANGELOG `[Unreleased]` updated (Added theodb_rs; Changed theodb.embed).

## Evidence

- **Parity (the gate):** `test_embed_sql.py` 10/10 + byte-identical `embed==embed_py` for same input.
- **Failure resilience:** `test_embed_failure_scenarios.py` 3/3 (SSRF-redirect, 4xx, empty-content).
- **Benchmark:** `docs/benchmarks/m17-embed-rust-vs-plpython.md` — Rust 13.92ms vs py 15.66ms/call (N=200, 5 runs); no regression; I/O-bound; no perf claim.
- **Quality:** cargo build 0 warnings; clippy 0 warnings; ruff clean; cargo audit 0 CVE.
- **deps-audit:** PASS_WITH_CAVEATS — rustls-webpki CVEs avoided via `https-native`; serde_cbor/paste unmaintained (pgrx-transitive, LOW).

## Verdict: READY_TO_MERGE

No BLOCKER; both HIGH findings resolved with real green tests + corrected docs; remaining MEDIUM/LOW are
accepted-and-disclosed transition debt or pre-existing parity items. Proceed to `/release` (v0.16.0, flip M17).
