# Blueprint — M1 Core + empacotamento

**Version:** 1.0 · **Date:** 2026-06-28 · **Slug:** m1-core-packaging · **Cycle:** discover
**Method:** empirical spike on the shipped `theo-db:dev` image + the upstream PostgreSQL test method.

## Context

M1 = "distribuição PostgreSQL-compatível completa em imagem container, com extensões pré-instaladas e a
suíte de compatibilidade do upstream passando." TheoDB does **NOT fork the engine** (ADR 0001) — the engine
is the unmodified PGDG `postgresql-17` (17.10) binary; we add extensions in their own namespaces. Much of M1
is already shipped by M0/M2; M1 formalizes it with evidence.

## Coverage Corner 1 — Integration Tests (the oracle)

- **DoD-1 oracle:** the upstream PG **17.10** regression suite (`src/test/regress`, ~220 core tests) run via
  `make installcheck` against the TheoDB engine. Spike findings: `pg_regress` binary ships in the image
  (`/usr/lib/postgresql/17/lib/pgxs/src/test/regress/pg_regress`) but the test files + `regress.so` do not →
  build them from the matching source tag `REL_17_10`, matching the Debian feature flags (so expected outputs
  line up), and run against a throwaway TheoDB cluster. 100% pass = the versioned report.
- **DoD-2 oracle:** `CREATE EXTENSION` succeeds for the MVP extensions on a fresh container — measured:
  `vector 0.8.3`, `vectorscale 0.9.0`, `plpython3u 1.0`, `plpgsql 1.0`.

## Coverage Corner 2 — Dependencies (DoD-3, license)

- Core package = PGDG `postgresql-17` (PostgreSQL License) + `pgvector` (PostgreSQL License) + `pgvectorscale`
  (PostgreSQL License; Rust crate tree) + `plpython3u` (PostgreSQL) + `ca-certificates` (MPL-2.0/GPL-2+) +
  Debian system libs. **Zero AGPL** required. Spike: scanning `/usr/share/doc/*/copyright` for `Affero|AGPL`
  yields only `ca-certificates` — a **false positive** (the MPL tri-license prose merely *enumerates* AGPL; the
  package is GPL-2+/MPL-2.0, not AGPL). The pgvectorscale Rust crate tree is the one to sweep with cargo (the
  D1 pre-release obligation from M2).

## Coverage Corner 3 — Tools

- `pg_regress` + `make installcheck` (upstream); `docker` build of a throwaway `theo-db-regress` runner
  (`FROM theo-db:dev` so the engine-under-test IS the distribution); `pg_config --configure` to read the
  Debian build flags; `grep` over dpkg copyrights + `cargo` for crate licenses.

## Coverage Corner 4 — Techniques

- **installcheck against the distribution engine:** build `pg_regress`+`regress.so` from `REL_17_10`,
  `initdb` a fresh cluster with the TheoDB binaries, `make installcheck` with `--dlpath` to the build dir.
- **Feature-flag matching:** configure the source with the same `--with-*` surface (tcl/perl/python/pam/
  openssl/libxml/libxslt/uuid/gssapi/ldap/icu/nls) so the expected outputs match (no engine-behavior diffs).
- **No-fork inheritance:** because the engine binary is upstream-identical, a green suite confirms the
  repackaging did not regress core SQL — it is not re-litigating the engine, it is proving the package.

## Drawbacks & Risks

- Build-flag / locale mismatches can cause a handful of *expected-output* diffs unrelated to the engine —
  mitigate by matching the feature flags via `pg_config --configure`; document any residual env-diff honestly.
- Continuous green suite cost on each PG minor bump — mitigate: the runner is parameterized by `PG_TAG`.

## Unresolved Questions

- (none — the scope (regression report + extensions evidence + license sweep + tuning doc) is resolved by the
  spike; full TAP/`make check-world` and per-extension upstream suites are future hardening.)

## ADRs

- **ADR-1 — Prove DoD-1 via `make installcheck` from the matching source against the distribution engine**
  (not a fork-and-rerun). Rejected: (a) trust the PGDG build's own `make check` without local evidence — the
  DoD wants a versioned report on the distribution; (b) ship the test tree inside the production image — bloats
  it; the runner is a throwaway image instead.

## References

- `.claude/knowledge-base/references/supabase-postgres/` — SOTA peer for packaging Postgres + extensions.
- Upstream PostgreSQL `REL_17_10` `src/test/regress` (cloned in the throwaway runner).
- Spike evidence (this session): `pg_regress` present, extensions CREATE-able, AGPL scan clean.
