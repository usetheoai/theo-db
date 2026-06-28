# Packaging, extensions & tuning — TheoDB core (M1)

TheoDB is a **PostgreSQL-compatible distribution**: the engine is the unmodified PGDG `postgresql-17`
(17.10) binary (**no engine fork** — ADR 0001), packaged in a container with the MVP extensions
pre-installed and enableable via `CREATE EXTENSION`. This doc is the M1 deliverable: the extension suite,
joint tuning, and the evidence that the distribution passes the upstream regression suite with zero AGPL.

## Pre-installed extensions (DoD-2)

All ship in the image and are enableable on any database:

| Extension | Version | Enable | License |
|---|---|---|---|
| `vector` (pgvector) | 0.8.3 | `CREATE EXTENSION vector;` | PostgreSQL License |
| `vectorscale` (pgvectorscale, StreamingDiskANN) | 0.9.0 | `CREATE EXTENSION vectorscale CASCADE;` | PostgreSQL License |
| `plpython3u` | 1.0 | `CREATE EXTENSION plpython3u;` | PostgreSQL License |
| `plpgsql` | 1.0 | (default) | PostgreSQL License |

Verified on a fresh container: all four report their version via `pg_extension` after `CREATE EXTENSION`.

## Joint tuning (extensions together)

The extensions coexist without conflict (separate namespaces / access methods). Recommended baseline:

- **Vector search:** default index is **HNSW** (M2 evidence-driven decision — see `docs/decisions/m2-index-decision.md`).
  `SET hnsw.ef_search` trades recall for latency at query time; `maintenance_work_mem` ↑ speeds index builds.
- **DiskANN (pgvectorscale):** for high-dim / large-scale; `SET diskann.query_search_list_size` +
  `diskann.query_rescore` trade recall for latency (see the M2 benchmark sweep).
- **Embeddings (`theodb.embed`, plpython3u):** configure the model endpoint via the `theodb.embedding_*` GUCs
  (see `docs/sql-embeddings.md`); the call is synchronous — batch large jobs outside a single statement.
- **HA + backup:** when running under Patroni, `archive_mode`/`archive_command` live in the Patroni-managed
  `postgresql.parameters` (see `docs/operations/ha-backup-runbook.md`).

## Upstream regression suite (DoD-1)

The distribution passes the **PostgreSQL 17.10 upstream regression suite, 100%**:

```
# All 225 tests passed.
```

How it is produced (reproducible, throwaway image — never shipped):

```bash
docker build -f packaging/Dockerfile.regress -t theo-db-regress .   # builds pg_regress + regress.so from REL_17_10
docker run --rm theo-db-regress                                     # initdb a TheoDB cluster + make installcheck
```

`packaging/Dockerfile.regress` is `FROM theo-db:dev`, so the engine under test **is** the distribution
(PGDG 17.10 + our extensions). The source is the matching tag `REL_17_10`, configured with the same Debian
feature surface (tcl/perl/python/pam/openssl/libxml/libxslt/uuid/gssapi/ldap/icu/nls) so expected outputs
line up. Because the engine is not forked, a green suite confirms the **repackaging** did not regress core
SQL — it is not re-litigating the engine. Re-run on each PG minor bump via the `PG_TAG` build arg.

## License due-diligence — zero AGPL (DoD-3)

Confirmed permissive across the whole package (D1 — no AGPL in the distribution):

- **System packages (apt):** scanning `/usr/share/doc/*/copyright` for `Affero|AGPL` yields only
  `ca-certificates` — a **false positive** (the MPL tri-license prose *enumerates* AGPL; the package is
  GPL-2+/MPL-2.0). Zero AGPL-licensed apt packages.
- **pgvectorscale Rust crate tree (statically linked into `vectorscale.so`):** `cargo metadata` over the
  crate tree — **293 crates, 0 AGPL/Affero**. Distribution: MIT / Apache-2.0 / Unicode-3.0 / ISC / Zlib /
  Unlicense (the 2 "no-license-field" entries are pgvectorscale's own workspace crates — `vectorscale`,
  `pgvectorscale_derive` — under the project's PostgreSQL License, not AGPL).
- **Extensions:** pgvector / pgvectorscale / plpython3u — PostgreSQL License. (HA deps Patroni/pgBackRest —
  MIT — are not part of the core image; see the HA runbook.)

Net: the core package is **100% permissive, zero AGPL** — clear under PRD §11 (D1).

## Reproduce the M1 checks

```bash
# DoD-2 — extensions
docker run -d --name t -e POSTGRES_PASSWORD=postgres theo-db:dev && sleep 8
docker exec t psql -U postgres -c "CREATE EXTENSION vector; CREATE EXTENSION vectorscale CASCADE; CREATE EXTENSION plpython3u; \dx"
docker rm -f t
# DoD-1 — regression suite
docker build -f packaging/Dockerfile.regress -t theo-db-regress . && docker run --rm theo-db-regress
# DoD-3 — AGPL sweep (apt + Rust crates) — see commands above
```
