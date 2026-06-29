# Discover Edge Case Review — pgrx-extension-foundation

Date: 2026-06-29
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/pgrx-extension-foundation-plan.md
Research questions analyzed: 6
Edge cases found: 3 (MUST FIX: 1, SHOULD TEST: 1, DOCUMENT: 1)

Paths verified: pgvectorscale `Cargo.toml`/`vectorscale.control`/`Makefile`/`src/lib.rs`/`src/access_method/mod.rs`
exist; `pg_extern`/`pg_schema`/`pgrx::prelude` present across `src/` (lib.rs + access_method/* + util/*);
`#[pg_test]` present (lib.rs + util/chain.rs + access_method/vacuum.rs). vectorchord `Cargo.toml` exists.

## MUST FIX

### EC-1: Q2/Q5 cite `docs.rs` for the HTTP crate, but only `github.com` is allowlisted
- **Affected question:** Q2, Q5 (techniques/deps)
- **Family:** Reference path / web allowlist
- **Scenario:** the HTTP-crate investigation planned WebFetch on `docs.rs`. `.claude/rules/discover-web-allowlist.txt`
  allows `github.com` (+ raw.githubusercontent, *.github.io, postgresql.org, …) but **not `docs.rs`/`crates.io`**.
  Fase A on docs.rs would be dropped by the web-source discipline → Q2/Q5 risk BLOCKED on a fixable source.
- **Impact:** the minimal-HTTP-crate decision (load-bearing for M17 — no in-repo HTTP example) could stall.
- **Suggested fix:** repoint Q2/Q5 web source to **`github.com`** — the candidate crates' repos (`neonphog/minreq`
  → actually `pkgw/minreq`/`algesten/ureq`) + their READMEs are on github; pgrx docs (`pgcentralfoundation/pgrx`)
  too. Read license + dep-tree from the github repo (`Cargo.toml`/README), not docs.rs.

## SHOULD TEST

### EC-2: coexistence — the new Rust extension must not clash with the current SQL-only `theodb`
- **Affected question:** Q1
- **Suggested halt-loop checkpoint:** the blueprint MUST state the coexistence mechanism — either (a) a
  separate extension name during transition (e.g. `theodb_rs`) that owns the Rust `theodb.embed`, or (b)
  migrating `theodb.embed` out of the SQL `theodb--1.0.sql` into the Rust extension while keeping the `theodb`
  name — and how `theodb.embed` is removed from the SQL bodies to avoid a duplicate-definition clash on
  `CREATE EXTENSION`. Without this, M17 could ship two conflicting `theodb.embed`.

## DOCUMENT

### EC-3: build is in Docker (scale-builder pattern), not local; disk now OK
- **Accepted risk:** `cargo pgrx init` compiles a PostgreSQL (~GB). Resolved: the build runs in a Docker stage
  mirroring `scale-builder` (which already compiles pgvectorscale successfully), and disk was freed to ~34 GB
  (from 7 GB) by pruning intermediate images + build cache. The blueprint/plan should note the build is
  image-side (reproducible), not local-toolchain-dependent.

## Summary

| Question | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|
| Q1 | 0 | 1 | 0 |
| Q2/Q5 | 1 | 0 | 0 |
| Q6 | 0 | 0 | 1 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT — 1 MUST FIX absorbed (docs.rs→github.com); 1 SHOULD-TEST added
(coexistence checkpoint); 1 DOCUMENT (Docker build + disk OK).
