# Blueprint: m144-remediation

**Slug:** `m144-remediation`
**Cycle:** discover (phase 4 — /discover-execute) → feeds `/to-plan m144`
**Created:** 2026-07-23
**Sources:** pgvector, postgres (contrib + backend), citus, paradedb (under `.claude/knowledge-base/references/`) + in-repo prior art (M137 upgrade harness, M122 dead-letter, M143 parquet REVOKE).

> Citation convention: reference peers are cited under `.claude/knowledge-base/references/{project}/...` (the physical location; the plan text drops the `.claude/` prefix). In-repo paths are cited relative to the repo root. Every path below was verified with `test -f` before citing. No performance claim is made without methodology; behavior claims cite exact `file:line`.

## Context

Origem: loop-code-review full de `theodb_rs/` (2026-07-23, `.claude/knowledge-base/audits/theodb-rs-code-review-2026-07-23.md`, 100 findings, 0 CRITICAL). Três HIGH atingem o binário shipado (tabela abaixo). Este blueprint levanta, contra extensões PG maduras (pgvector/pg_trgm/citus/postgres-core/paradedb), **como estruturar** cada correção alinhado ao SOTA (Rule 9), para o `/to-plan m144` nascer sem retrabalho e sem workaround.

## Objective

Recomendar, com citação a `file:line` de peers e do repo, o padrão exato para (a) o script de upgrade `1.1.0→1.2.0`, (b) o hardening de `symqg_spike_bench`, e (c) a propagação de erro no delete do vectorizer — cada um com oráculo de teste identificado.

## Problem restated (3 shipped-binary HIGH findings)

| # | Fix | Root cause (verified) |
|---|---|---|
| A | Upgrade script `1.1.0→1.2.0` for the M143 lakehouse surface | The 1.0.0→1.1.0 script is full-schema, but the M143 lakehouse surface (`public.read_parquet`/`write_parquet`/`olap`) was added to the Rust + fresh-install only. `theodb_rs/theodb_rs.control:5` pins `default_version = '1.1.0'`; a 1.0.0 installer running `ALTER EXTENSION theodb_rs UPDATE` never receives it. |
| B | `symqg_spike_bench` PUBLIC-executable arbitrary fs-read | Created at `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql:340` with **no** REVOKE. The mass-REVOKE loop at `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql:1105-1112` matches only `n.nspname='theodb_rs' AND p.proname ~ '^_vectorizer_'`; `symqg_spike_bench` is unqualified (`public`) and unmatched. Its body does `std::fs::read(path)` at `theodb_rs/src/bench_symqg.rs:12` and `:28`, reachable from the `#[pg_extern]` at `theodb_rs/src/bench_symqg.rs:47-48`. |
| C | Swallowed delete (PII stays searchable) | `theodb_rs/src/vectorizer.rs:460` and `:469` — `let _ = Spi::run_with_args(...)` on both delete arms. A failed delete is still marked `done` by the worker at `theodb_rs/src/vectorizer.rs:917`, leaving the embedding queryable. |

---

## Coverage Corner 1 — Integration Tests

Answers **Q4** (upgrade-totality harness) and **Q5** (negative ACL test).

### Q4 — Harness that proves `ALTER EXTENSION UPDATE` is total and does not break the cluster

The authoritative oracle already exists in-repo: `scripts/test-upgrade.sh` (M137/F4). Its shape is exactly what the M144 DoD bullet 1 needs — install old → upgrade → validate against a fresh install:

- **Scenario A** — `scripts/test-upgrade.sh:41-55`: create `up_fresh` via `CREATE EXTENSION theodb_rs` (fresh install snapshot), create `up_old` via `CREATE EXTENSION theodb_rs VERSION '$FROM_VER'` then `ALTER EXTENSION theodb_rs UPDATE TO '$TO_VER'` (`:48-49`), assert `extversion == TO_VER` (`:50-51`), then `diff` the two schema snapshots (`:53-54`). **Post-upgrade schema+ACL MUST byte-equal a fresh install** — this is the direct oracle for fix A. The snapshot oracle is `theodb_rs/sql/schema_snapshot.sql` (`:27`).
- **Scenario CONV** — `scripts/test-upgrade.sh:57-73`: ages the catalog by DROPping objects, then proves `ALTER EXTENSION ... UPDATE` restores them (`:69`), guarding against a *vacuous* pass (aged < fresh assertion at `:66-67`). This is what gives the oracle its power and is the pattern fix A's delta must satisfy.
- **Scenario IDEM** — `scripts/test-upgrade.sh:75-86`: runs the upgrade script twice, asserts 0 `ERROR` and unchanged snapshot — the delta script must be idempotent (all `CREATE OR REPLACE` / guarded `CREATE`).
- **Scenario B1** — `scripts/test-upgrade.sh:88-106`: new `.so` over old catalog **without** `ALTER EXTENSION` (the "apt upgrade and forget" user); asserts the server does not crash. Relevant because the lakehouse fns are the surface a partial upgrade exposes.

The harness is version-parameterized (`FROM_VER`/`TO_VER` at `scripts/test-upgrade.sh:28-29`), so M144 runs it unchanged with `FROM_VER=1.1.0 TO_VER=1.2.0`.

**Peer corroboration (EC-2 satisfied; ≥2 sources):** the "post-upgrade == fresh install" oracle is the same pattern PostgreSQL's own extension test uses. citus exercises the full upgrade chain in `.claude/knowledge-base/references/citus/src/test/regress/sql/multi_extension.sql` via repeated `ALTER EXTENSION citus UPDATE TO '<ver>'` steps (grep-confirmed match; the file is the canonical multi-version upgrade regression test). The in-repo header itself records the four scenarios were adapted from pg_durable + ParadeDB (`scripts/test-upgrade.sh:10-16`).

### Q5 — Negative ACL test: prove a fn is superuser-only (common role denied)

Canonical pattern — `has_function_privilege` returns the ACL truth without needing to trap an error, from `.claude/knowledge-base/references/postgres/contrib/pg_walinspect/sql/pg_walinspect.sql:100-145`:

- `CREATE ROLE regress_pg_walinspect;` (`:103`)
- Before any grant: `SELECT has_function_privilege('regress_pg_walinspect', 'pg_get_wal_record_info(pg_lsn)', 'EXECUTE'); -- no` (`:105-106`) — the **negative** assertion (expected `false` / `f`).
- After `GRANT`: the same probe returns `-- yes` (`:117-118`).

This is the oracle for fix B: after the M144 REVOKE, `has_function_privilege('<common_role>', 'public.symqg_spike_bench(text,bigint,bigint,integer)', 'EXECUTE')` MUST be `false`.

A second, error-observing style (for asserting the *runtime* denial, not just the catalog bit) is `.claude/knowledge-base/references/postgres/contrib/citext/sql/create_index_acl.sql:14-46`: create a minimal role, `REVOKE ALL ON FUNCTION ... FROM PUBLIC` (`:20-26`, `:35-37`), then attempt the operation under that role with `\set VERBOSITY sqlstate` (`:80`) to capture the SQLSTATE of the permission error deterministically (encoding-independent). Either style is acceptable for the DoD bullet 2 oracle; `has_function_privilege` is the simpler and preferred one.

---

## Coverage Corner 2 — Dependencies

Answers **Q6** (does the delete-error propagation need a new dependency?).

**Verdict: zero new dependency (parsimony rung 4 — reuse installed machinery).** The M122 dead-letter / bounded-retry pipeline already exists in `theodb_rs/src/vectorizer.rs`:

- Queue schema with `state IN ('pending','processing','failed')` and an `attempts` counter — `theodb_rs/src/vectorizer.rs:47-48`.
- `_vectorizer_mark_failed` decides recoverable-vs-terminal **in SQL** by `attempts` vs `max_attempts`: below the cap → back to `pending` (bounded retry), at/over the cap → `failed` (dead-letter) — `theodb_rs/src/vectorizer.rs:276-290` (comment `:271-275`).
- `_vectorizer_purge_dead_letters` trims the dead-letter tail — `theodb_rs/src/vectorizer.rs:635-641`; `_vectorizer_reap_orphans` reclaims crashed-in-`processing` jobs at the cap — `theodb_rs/src/vectorizer.rs:612-621`.
- The worker's `process_one` already routes failure into this machinery: it runs the job in a subtxn with `.expect(...)` (`theodb_rs/src/vectorizer.rs:896-905`); on `Ok(())` it calls `_vectorizer_mark_done` (`:916-918`), on `Err(cause)` it calls `_vectorizer_mark_failed` (`:924-926`).

Therefore the fix is purely **local error propagation inside `_vectorizer_process_delete`** — make the delete diverge on SPI error (like the upsert path already does: `.unwrap_or_else(|e| crate::pg::err_input(...))` at `theodb_rs/src/vectorizer.rs:447-448`). The subtxn at `:896-905` then captures it, `outcome` becomes `Err`, and the *existing* M122 path marks the job failed/retries/dead-letters. No crate, no schema change, no new function.

---

## Coverage Corner 3 — Tools

Answers **Q7** (the exact command that proves the upgrade, and how M137 exercises it).

**Reproduction command (from the harness header `scripts/test-upgrade.sh:17-18`), version-bumped for M144:**

```
FROM_VER=1.1.0 TO_VER=1.2.0 \
PGINST=/root/.pgrx/18.4/pgrx-install PGPORT=28918 \
bash scripts/test-upgrade.sh
```

Exit 0 = all four scenarios passed; any failure aborts with the named cause (`scripts/test-upgrade.sh:20-21`, `set -euo pipefail`). The script builds the upgrade file path as `theodb_rs--$FROM_VER--$TO_VER.sql` under the pgrx install share dir (`scripts/test-upgrade.sh:76`), so the M144 delta script name (`theodb_rs--1.1.0--1.2.0.sql`) is what IDEM checks.

**Honest gap (UNVERIFIED-IN-CI):** grep of `.github/workflows/` for `test-upgrade` and `ALTER EXTENSION` returns **no** match — the seven workflows (`cassert-sql-safety.yml`, `ci-canary.yml`, `ci-failure-notify.yml`, `ci.yml`, `license-gate.yml`, `lint-rust.yml`, `schema-drift-gate.yml`) do **not** run the upgrade harness. `schema-drift-gate.yml` only guards the frozen snapshot (`.github/workflows/schema-drift-gate.yml:10` — "upgrade são snapshots congelados"), it does not exercise `ALTER EXTENSION UPDATE`. Consequence: today the upgrade is proven only by a manual droplet run (M137 evidence artifact `docs/benchmarks/m137-upgrade-chain.md`, referenced at `scripts/test-upgrade.sh:4`). M144 should treat "run `scripts/test-upgrade.sh` on the droplet and capture output" as the DoD bullet-1 evidence, and MAY (optional, out of blueprint scope) wire a CI job — but that is a tooling add, not a blocker for the fix.

---

## Coverage Corner 4 — Techniques

Answers **Q1** (delta-only upgrade script), **Q2** (REVOKE-after-CREATE for fs-reading fns), **Q3** (error propagation without swallowing).

### Q1 — Upgrade scripts are DELTA-ONLY (only new/changed objects), not full-schema

Confirmed against **two independent peers**:

- **pgvector** — `.claude/knowledge-base/references/pgvector/sql/vector--0.7.4--0.8.0.sql` is 27 lines total: only the *new* `array_to_sparsevec` overloads (`:4-14`) and their casts (`:16-26`), guarded by the `\quit` header (`:2`). It does **not** re-emit the base schema.
- **pg_trgm** — `.claude/knowledge-base/references/postgres/contrib/pg_trgm/pg_trgm--1.5--1.6.sql` is 11 lines: only two `ALTER OPERATOR FAMILY ... ADD OPERATOR` statements (`:6-10`) — the delta between 1.5 and 1.6.

This contrasts with the current in-repo `theodb_rs--1.0.0--1.1.0.sql` (1100+ lines, effectively full-schema via `CREATE OR REPLACE`). The SOTA pattern (Rule 9) for `1.1.0→1.2.0` is a **small delta file that emits only the M143 lakehouse objects + their REVOKEs**, nothing else.

The exact CREATE surface the delta must add is already authored in the fresh-install Rust as `#[pg_extern]` functions: `olap` (`theodb_rs/src/parquet.rs:75-76`), `read_parquet` (`:121-122`), `write_parquet` (`:168-169`) — all landing in schema `public`.

### Q2 — REVOKE EXECUTE FROM public immediately after CREATE for filesystem-reading functions

PostgreSQL's canonical least-privilege template, from `.claude/knowledge-base/references/postgres/src/backend/catalog/system_functions.sql`:

- `REVOKE EXECUTE ON FUNCTION lo_import(text) FROM public;` (`:688`), `lo_import(text, oid)` (`:690`), `lo_export(oid, text)` (`:692`).
- `REVOKE EXECUTE ON FUNCTION pg_read_file(text) FROM public;` and its overloads (`:704-710`); `pg_read_binary_file(...)` overloads (`:712-718`); the `pg_ls_*dir()` family (`:694-702`).

The rule: **any function that reads the server filesystem is REVOKEd from `public` right after its definition.** `symqg_spike_bench` does exactly `std::fs::read` (`theodb_rs/src/bench_symqg.rs:12,28`) → it belongs in this class.

The in-repo M143 parquet surface already applies this idiom via pgrx `extension_sql!` with `requires` to order the REVOKE after the CREATE — `theodb_rs/src/parquet.rs:320-329`:

```
REVOKE ALL ON FUNCTION public.write_parquet(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.read_parquet(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.olap(text) FROM PUBLIC;
```

(`requires = [write_parquet, read_parquet, olap]` guarantees ordering — `theodb_rs/src/parquet.rs:328`). This is the exact template both for (i) the lakehouse REVOKEs the 1.2.0 delta must include and (ii) the symqg REVOKE (or its gate-out — see Recommendations).

### Q3 — Error propagation without swallowing (peer + in-repo idiom)

**Peer (paradedb, same pgrx stack, EC-1 corroboration):** `.claude/knowledge-base/references/paradedb/pg_search/src/index/writer/index.rs` returns `Result<...>` and propagates with `?` throughout — `new -> Result<Self>` (`:44`), `with_id` propagating `SegmentWriter::for_segment(...)?` (`:48-50`), `add_document -> Result<()>` (`:58`, `:63`), `finalize -> Result<Segment>` with `self.writer.finalize()?` (`:84-86`). paradedb never `let _ =`-swallows a fallible write; it bubbles via `?` (or `.map_err(...)?`, e.g. `.claude/knowledge-base/references/paradedb/pg_search/src/index/directory/mvcc.rs:183`). This is the idiomatic Rust/pgrx pattern the delete fix should mirror.

**In-repo authoritative pattern (the sibling that already does it right):** the upsert path diverges on SPI failure so the worker traps it — `theodb_rs/src/vectorizer.rs:447-448`:

```
Spi::run_with_args(&upd_q, &[vec_text.into(), source_pk.into()])
    .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer upsert failed: {e:?}")));
```

The delete arms at `theodb_rs/src/vectorizer.rs:460` and `:469` must adopt this same shape (propagate the `Spi` error via `err_input`/`?` instead of `let _ =`), so the subtxn at `:896-905` converts it into the `Err` branch → `mark_failed` → M122 dead-letter.

---

## Cross-cutting Comparison

| Fix | SOTA peer pattern (cited) | In-repo prior art (cited) | Recommendation |
|---|---|---|---|
| A — upgrade `1.1.0→1.2.0` | delta-only scripts: pgvector `vector--0.7.4--0.8.0.sql:4-26`, pg_trgm `pg_trgm--1.5--1.6.sql:6-10` | `scripts/test-upgrade.sh` (M137 harness) | delta-only + bump `default_version` |
| B — `symqg_spike_bench` PUBLIC | `system_functions.sql:688,704` REVOKE-after-CREATE; `parquet.rs:320-329` in-repo REVOKE | mass-REVOKE loop `theodb_rs--1.0.0--1.1.0.sql:1105-1112` (misses it) | gate-out (Cargo feature); REVOKE = fallback |
| C — swallowed delete | paradedb `pg_search/src/index/writer/index.rs:44-86` propagates via `?` | upsert sibling `vectorizer.rs:447-448` → subtxn `:896-905` → M122 dead-letter `:276-290` | propagate SPI `Result`, don't mark done |

Todos os três convergem para prior art existente — zero dep nova, zero reinvenção (Rule 9).

---

## ADRs

### D1 — `symqg_spike_bench`: GATE-OUT (remove from the shipped SQL surface), do not merely REVOKE

**Context.** `symqg_spike_bench` is experimental spike/benchmark code (`theodb_rs/src/bench_symqg.rs`, "spike"/"bench" in the name; reads a SIFT dataset dir off the server filesystem via `std::fs::read`). It is PUBLIC-executable today (`theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql:340`, no REVOKE) — a filesystem-read primitive exposed to every role on the shipped binary.

**Decision.** Gate it behind a Cargo feature (e.g. `#[cfg(feature = "spike_bench")]` on the `#[pg_extern]` at `theodb_rs/src/bench_symqg.rs:47`) so it is **absent from the default-built `.so` and from the generated SQL surface entirely**, rather than shipping-and-REVOKEing.

**Alternatives considered.**
- *(a) REVOKE-only* (extend the pattern of `theodb_rs/src/parquet.rs:320-329` / `system_functions.sql:704`): correct and sufficient for security, but keeps a dataset-reading spike primitive in the production surface where a future GRANT re-opens it. Rejected as the primary fix because the surface should not exist in a shipped DB.
- *(b) Gate-out (chosen)*: smallest shipped attack surface, aligns with "Esforço ≠ Complexidade / accidental complexity elimination" (CLAUDE.md) — a benchmark spike is not product surface. If the function must remain shipped for any reason, fall back to (a) REVOKE as the documented mitigation.

**Consequence for the oracle.** With gate-out, the DoD bullet-2 test asserts the function **does not exist** for a common role (`has_function_privilege` errors on unknown function → assert absence, or assert `symqg_spike_bench` not in `pg_proc`). With the REVOKE fallback, the `has_function_privilege(... 'EXECUTE') = false` oracle (`pg_walinspect.sql:105-106`) applies.

### D2 — Upgrade discipline: delta-only `1.1.0→1.2.0`, bump `default_version`, prove with `test-upgrade.sh`

**Context.** The M143 lakehouse surface reaches only fresh installs; the frozen upgrade chain omits it (`theodb_rs.control:5` still `1.1.0`).

**Decision.** Author `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` as a **delta-only** script (Q1 pattern: pgvector `vector--0.7.4--0.8.0.sql`, pg_trgm `pg_trgm--1.5--1.6.sql`) emitting exactly the three lakehouse CREATEs (`parquet.rs:75,121,168`) + their three REVOKEs (`parquet.rs:320-329`); bump `default_version = '1.2.0'`; gate with the existing harness (`scripts/test-upgrade.sh`, `FROM_VER=1.1.0 TO_VER=1.2.0`), whose Scenario A oracle proves post-upgrade == fresh install.

**Alternative rejected.** Re-emitting the full schema in the delta (current 1.0.0→1.1.0 style) — larger diff, higher drift risk, and against the two-peer SOTA (Q1). Not chosen.

---

## Recommendations for M144

Per-fix, explicit and implementable without rework. Every citation below resolves on disk.

**(a) Upgrade script `1.1.0 → 1.2.0` — DELTA-ONLY.**
Create `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` containing only the new M143 lakehouse objects and their REVOKEs — nothing else:
- `CREATE FUNCTION public.olap(text) ...`, `public.read_parquet(text) ...`, `public.write_parquet(text,text) ...` — mirroring the fresh-install externs (`theodb_rs/src/parquet.rs:75-76,121-122,168-169`).
- `REVOKE ALL ON FUNCTION public.{write_parquet,read_parquet,olap} FROM PUBLIC;` — copied verbatim from `theodb_rs/src/parquet.rs:322-324`.
- Bump `default_version = '1.2.0'` in `theodb_rs/theodb_rs.control:5`.
- Pattern authority: pgvector `.claude/knowledge-base/references/pgvector/sql/vector--0.7.4--0.8.0.sql:4-26` + pg_trgm `.claude/knowledge-base/references/postgres/contrib/pg_trgm/pg_trgm--1.5--1.6.sql:6-10` (delta-only, ≥2 peers).
- Oracle: `scripts/test-upgrade.sh` Scenario A + CONV + IDEM (`:41-86`), run `FROM_VER=1.1.0 TO_VER=1.2.0`.

**(b) `symqg_spike_bench` — GATE-OUT (recommended), REVOKE as fallback.**
Recommendation: gate the `#[pg_extern]` at `theodb_rs/src/bench_symqg.rs:47` behind a Cargo feature so it is **removed from the shipped `.so` and SQL surface entirely** — it is experimental spike code that reads arbitrary server files (`theodb_rs/src/bench_symqg.rs:12,28`) and has no place in a production surface (ADR-1). Rationale: smallest shipped attack surface; a benchmark spike is accidental product surface, not essential complexity (CLAUDE.md "Esforço ≠ Complexidade"). If it must remain shipped, apply the REVOKE template from `theodb_rs/src/parquet.rs:322-324` / PostgreSQL `.claude/knowledge-base/references/postgres/src/backend/catalog/system_functions.sql:688,704` (`REVOKE EXECUTE ON FUNCTION ... FROM PUBLIC` right after CREATE — the fs-reading-function least-privilege rule). Do **not** rely on the existing `^_vectorizer_` mass loop (`theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql:1105-1112`) — it does not match this function.
- Oracle: negative ACL via `has_function_privilege('<common_role>', 'public.symqg_spike_bench(text,bigint,bigint,integer)', 'EXECUTE') = false` (gate-out → assert function absent), pattern `.claude/knowledge-base/references/postgres/contrib/pg_walinspect/sql/pg_walinspect.sql:105-118`.

**(c) Swallowed delete — PROPAGATE the SPI `Result`; let the existing M122 dead-letter handle terminal failure.**
In `_vectorizer_process_delete` (`theodb_rs/src/vectorizer.rs:455-470`) replace both `let _ = Spi::run_with_args(...)` (`:460`, `:469`) with the diverge-on-error shape the upsert sibling already uses — `.unwrap_or_else(|e| crate::pg::err_input(...))` (`theodb_rs/src/vectorizer.rs:447-448`) or `?`/`ereport`. On error the job MUST NOT be marked `done`: the worker's subtxn (`:896-905`) then yields `Err`, routing to `_vectorizer_mark_failed` (`:924-926`) → bounded retry / dead-letter (`:276-290`). **Zero new dependency** (Corner 2) — the machinery is entirely present (M122). Peer idiom corroboration: paradedb `?`-propagation in `.claude/knowledge-base/references/paradedb/pg_search/src/index/writer/index.rs:44-86`.
- Oracle: a regression test where the delete SQL fails (e.g. dropped/renamed target col) asserts the job ends `failed` (not `done`) and the embedding is not left populated — reusing the existing dead-letter test scaffolding (`theodb_rs/src/vectorizer.rs:1204-1254`).

### Honest gaps / caveats

- **No performance claim is made** in this blueprint — all findings are structural/behavioral, cited by `file:line`. (No UNBENCHMARKED perf assertions were needed.)
- **CI does not run the upgrade harness today** (Corner 3, Q7) — DoD bullet-1 evidence is a manual droplet run of `scripts/test-upgrade.sh`. Wiring it into `.github/workflows/` is an optional tooling follow-up, out of this blueprint's scope.
- **EC-1 (Q3)** used both a peer (paradedb) and the in-repo authoritative sibling — the delete fix does not depend on paradedb exposing an identical SPI-delete handler.
- **EC-2 (Q4)** answered primarily by in-repo `scripts/test-upgrade.sh`, corroborated by citus `multi_extension.sql` — not blocked.
