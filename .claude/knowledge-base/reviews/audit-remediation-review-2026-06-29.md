# Review: audit-remediation

**Date:** 2026-06-29
**Reviewers (spawned agents):** 5 — architecture, tests, wiring, cross-validation, domain (data-plane/PostgreSQL/pgvector/SSRF)
**Diff base:** `444e7b9..develop` (8 implementation commits)
**Findings:** 1 BLOCKER, 3 MEDIUM, 6 LOW, many INFO
**Initial verdict:** NEEDS_FIXES → **after fixes: READY_TO_MERGE**

## BLOCKER (fixed)

### D-01/D-02 — `test_extension_install.py` red on the branch
- Found by: domain + (predicted by) the Final-Phase gate.
- `test_extension_installs_full_surface` did `CREATE EXTENSION theodb CASCADE` only and asserted
  `theodb.embed` present + `extversion == "1.0"`. Two defects: (a) **pre-existing since M17** — post-M17
  `theodb.embed` lives in `theodb_rs` (not pulled by `theodb` CASCADE), so the embed assertion was already
  failing at the base `444e7b9` (an M17 oversight that slipped earlier reviews); (b) **slice-caused** — the
  `default_version` bump to 1.1 made the `== "1.0"` assertion wrong.
- **Fix:** the test now installs BOTH extensions (`theodb` + `theodb_rs`, the shipped pair), asserts the full
  surface (incl. the new `theodb.embed_batch` + `theodb.import_pinecone_chunked`), and pins `extversion == "1.1"`.
- **Verified:** `test_extension_install.py` 9/9 green; full suite **41/41 green** against `theo-db:audit-rem`.

## MEDIUM (fixed)

### F1/D-04 — CHANGELOG documented the wrong guard signature
- The `[Unreleased] § Changed` entry said `to_regprocedure('theodb.embed(text)')` (1-arg) — exactly the
  false-positive form D5 rejected. Code is correct (`(text, text)`). **Fixed** the CHANGELOG text (public contract).

### D-03/T-COVERAGE-002 — retirement test didn't exercise the REAL upgrade path
- `test_upgrade_drops_plpython_embed` seeded a free-standing function + ran the bare delta, not
  `ALTER EXTENSION theodb UPDATE` on a `theodb`-MEMBER function (the actual v0.x scenario). The drop-then-no-clash
  claim was inferred, not proven end-to-end. **Fixed:** added `test_real_upgrade_path_drops_member_embed_then_theodb_rs_installs_clean`
  — fresh DB, `CREATE EXTENSION theodb VERSION '1.0'` → seed a plpython3u embed as a member → `ALTER EXTENSION
  theodb UPDATE TO '1.1'` (drops it in extension-update context) → `CREATE EXTENSION theodb_rs` succeeds with
  no clash + the Rust embed present. **Verified green.**

## LOW (addressed)

- **ARCH-05** — retry path had no observability (wiring-triad pillar c). **Added** `pg::warn` (Rust `pgrx::warning!`)
  + `plpy.warning` in `ai._chat` on every transient retry (HTTP status / connection error + attempt count;
  api_key never in the message). Verified the no-leak/bounds retry tests still pass.
- **F2** — `is_recoverable_status` comment said "5xx irrecoverable" while 502/503 are recoverable. **Reworded** to "other 5xx (500/504)".
- **T-SMELL-004** — redundant `assert hits <= 3` after `assert hits == 3`. **Removed.**

## LOW (accepted, documented — no change)

- **T-FLAKE-003** — scripted-server hit counters are serial-only (class-level dict). The suite is the serial
  integration runner; flagged for the flakiness register if `-n auto` is ever adopted.
- **T-FLAKE-005** — `_free_port()` bind-0/close/reuse TOCTOU; idiomatic, low incidence.
- **WIRING-INFO-2 / cargo pgrx test** — the Rust `#[pg_test]`s cannot run in the build container
  (`initdb: cannot be run as root`, a PostgreSQL safety constraint). Their behaviors are double-covered by the
  Python oracle against the real image (the project's documented gate, `lib.rs`). Not a code defect.

## Edge-case coverage

EC-1 (empty→empty vector[]), EC-2 (retry bounds + api_key no-leak), EC-3 (index-aligned out-of-order + dups) —
all covered by `test_embed_batch.py` + `test_retry.py`. Both edge AND negative cases asserted with specific
typed SQLSTATEs (22023/38000/0A000), per `testing.md §4.1`.

## Cross-validation summary

All 6 plan tasks fully-implemented (T0.1–T5.1); Coverage Matrix 11/11 actionable findings; every Acceptance
Criterion's `pytest ...::test_X` oracle resolves to a real test (no fabricated oracle); no silent plan
divergence (D1–D5 match the code, incl. the D5 `(text, text)` resolution).

## Quality gates summary

- Full embed/AI + install + retirement suite: **41/41 PASS** vs `theo-db:audit-rem` (rebuilt).
- `cargo clippy --features pg17 -- -D warnings`: clean (re-verified after the observability change).
- `ruff check`: clean. `cargo pgrx install --release`: compiles. `/code-quality`: PASS.
- Benchmark (CTO data): N→1 collapse N=8 2.93×, N=32 7.49×, N=128 7.99× (mean±std, 5 runs).
- Wiring triad: every new symbol has caller + integration test + observability (typed errors / RAISE NOTICE / retry WARNING / benchmark).

## Spawned agents (audit trail)

architecture · tests · wiring · cross-validation · domain-data — findings inline above.

## Handoff decision

**READY_TO_MERGE.** The BLOCKER and all three MEDIUM findings are resolved and re-verified against the rebuilt
image; the LOW findings are fixed or accepted with documented rationale. No unresolved BLOCKER/HIGH remains.
