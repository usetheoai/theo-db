# Blueprint — M3 Minimal Migration (vanilla PostgreSQL → TheoDB)

**Version:** 1.0 · **Date:** 2026-06-27 · **Slug:** m3-minimal-migration · **Cycle:** discover
**Method:** empirical — every claim is backed by a live `pg_dump`/`pg_restore` run between a vanilla
`pgvector/pgvector:pg17` source and the `theo-db:dev` target (this session). No fabricated citations.

## Context

M3 (ROADMAP) requires a documented + tested entry path from vanilla PostgreSQL into TheoDB via standard
import/export, preserving vector data and indexes. TheoDB is wire-compatible with PostgreSQL (gate), so
the migration tooling is the **standard `pg_dump`/`pg_restore`** — no bespoke tool (Rule 9: don't reinvent).
The investigation question: *does the standard path preserve `vector` data + HNSW/IVFFlat indexes intact,
and what are the failure modes?*

## Coverage Corner 1 — Integration Tests

How to prove a migration is correct (the oracle):

- **Data integrity oracle:** `md5(string_agg(embedding::text, ',' ORDER BY id))` on source vs target.
  Measured identical: `227de9acfb3dc57de802827a7b19f4b4` (1000 rows, 97 distinct vectors) on BOTH the
  custom-format restore and the plain-format restore — bit-exact preservation.
- **Index-preservation oracle:** compare `pg_indexes` + the access method (`pg_am.amname`) per index.
  Measured on target after restore: `items_hnsw (hnsw)`, `items_ivf (ivfflat)`, `items_pkey (btree)`,
  `items_title (btree)` — all 4 survived.
- **Index-usability oracle:** `EXPLAIN` an ANN query with `enable_seqscan=off`. Measured: planner uses
  `Index Scan using items_ivf`; with ivfflat dropped, `Index Scan using items_hnsw` — both vector indexes
  are usable post-restore, not just present.
- The M3 smoke test automates exactly these three oracles end-to-end (source seed → dump → restore →
  assert). This is the DoD-2 artifact.

## Coverage Corner 2 — Dependencies

- **No new runtime dependency.** Migration uses `pg_dump`/`pg_restore`/`psql` — shipped with PostgreSQL
  client tools (already in both images). Rung 4 of the parsimony ladder: reuse what's installed.
- **Source must have `pgvector`** for a vector table to exist; a truly columnless "vanilla Postgres"
  (no pgvector) migrates trivially (no vector type involved). The meaningful case is Postgres+pgvector →
  TheoDB.
- **Version alignment:** source `pgvector` 0.8.3 == target 0.8.3 (measured). Same-or-older source version
  is the safe direction; see Drawbacks for the mismatch failure mode.

## Coverage Corner 3 — Tools

Two equivalent paths, both verified end-to-end:

- **Custom format (recommended):** `pg_dump -Fc -d <src> -f dump.fc` → `pg_restore --no-owner -d <dst> dump.fc`.
  Restore exit 0, integrity verified. Supports parallel restore (`-j`), selective restore, and `--section`.
- **Plain SQL:** `pg_dump -d <src> | psql -d <dst>`. Restore exit 0, same integrity. Simplest for small DBs,
  pipeable, human-readable.
- `--no-owner` avoids role-ownership errors when source roles don't exist on the target.

## Coverage Corner 4 — Techniques

- **Extension idempotency:** `pg_dump` emits `CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;`
  (measured). TheoDB pre-creates `vector` at init, so the restore's extension statement is a safe no-op —
  no conflict, no manual pre-step needed for `vector`.
- **Index recreation:** pgvector index DDL (`USING hnsw (embedding vector_l2_ops)`, `USING ivfflat ... WITH
  (lists=...)`) is plain index DDL in the dump; `pg_restore` recreates (and rebuilds) the index on the
  target — the graph/lists are rebuilt from the restored data, not copied, which is correct.
- **TheoDB value-add (post-migration):** after a vanilla migration, the user can `CREATE INDEX ... USING
  diskann` (pgvectorscale, M2) on the migrated table — the migration brings the data; TheoDB adds the
  advanced index. (Out of M3 scope to automate, but the guide should mention it.)

## Drawbacks & Risks

1. **Extension version mismatch (source newer than target).** If the source `vector` is newer, the dump may
   reference types/opclasses absent on the target → restore error. *Mitigation:* align/upgrade the target
   extension first; the guide documents checking `extversion` on both ends before migrating. Severity: MED.
2. **Large datasets / single-statement plain restore.** `pg_dump | psql` streams, but a huge `COPY` + index
   rebuild can be slow/blocking. *Mitigation:* custom format with `pg_restore -j N` (parallel) + restore
   indexes after data. Severity: LOW (M3 is "minimal"; streaming is documented, not automated).
3. **Role/ownership + ACLs.** Source-specific roles cause `pg_restore` ownership errors. *Mitigation:*
   `--no-owner` (+ `--no-acl` when needed). Severity: LOW.

## Unresolved Questions

- (none — the standard path is proven for the minimal scope; diskann-as-post-step and streaming-at-scale are
  explicitly future work, not M3.)

## ADRs

- **ADR — Use standard `pg_dump`/`pg_restore`, no bespoke migration tool.** Alternatives rejected:
  (a) a custom dump/restore utility — reinvents Postgres tooling (Rule 9 violation), more code to maintain;
  (b) logical replication — heavyweight for a one-shot minimal migration, out of M3 scope. Standard tooling
  is the wire-compatible, zero-new-dependency choice and is empirically proven to preserve vector data +
  indexes.

## References

- pgvector reference clone: `.claude/knowledge-base/references/pgvector/` (vector type + index AMs).
- Empirical evidence (this session): source `pgvector/pgvector:pg17`, target `theo-db:dev`, checksum match
  on both dump formats; the M3 smoke test reproduces it.
