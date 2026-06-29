# Minimal migration — vanilla PostgreSQL → TheoDB

Move an existing PostgreSQL + `pgvector` database into TheoDB using **standard `pg_dump` / `pg_restore`**.
TheoDB is wire-compatible with PostgreSQL, so there is **no special tool** — your vector data and your
HNSW / IVFFlat indexes come across intact. This is proven end-to-end by `scripts/migrate-smoke.sh` (and in CI).

> Use a `pg_dump`/`pg_restore` client whose version is **>= your source server version** (a PG16 client
> cannot dump a PG17 server). The repo smoke runs the tools *inside* the PG17 containers to sidestep this.

## 0. Pre-flight — check the extension versions

A restore fails if the source uses a `vector` newer than the target. Compare first:

```bash
psql -h SRC_HOST -U postgres -d SRC_DB -tAc "SELECT extversion FROM pg_extension WHERE extname='vector';"
psql -h DST_HOST -U postgres -d DST_DB -tAc "SELECT extversion FROM pg_extension WHERE extname='vector';"
```

If the source is newer, upgrade the target's extension (`ALTER EXTENSION vector UPDATE;`) before migrating.
TheoDB ships pgvector `0.8.3`.

## 1. Capture a baseline checksum on the source (integrity oracle)

This hashes the **whole row** (id + title + embedding), ordered by id — so any change to any column is caught:

```bash
psql -h SRC_HOST -U postgres -d SRC_DB -tAc \
  "SELECT md5(string_agg(id::text || '|' || title || '|' || embedding::text, ',' ORDER BY id)) FROM items;"
```

Keep this value — you compare it on the target after the restore; equal checksums prove every row came
across unchanged.

## 2. Migrate

### Option A — custom format (recommended)

```bash
pg_dump -Fc -h SRC_HOST -U postgres -d SRC_DB -f db.dump
pg_restore --no-owner --exit-on-error -h DST_HOST -U postgres -d DST_DB db.dump
```

`--exit-on-error` makes `pg_restore` **fail fast** instead of skipping past errors and exiting 0 (a silent
partial restore). `--no-owner` avoids ownership errors when source roles do not exist on the target. For
big databases, custom format also supports parallel restore: `pg_restore --no-owner --exit-on-error -j 4 -d DST_DB db.dump`.

### Option B — plain SQL (simplest, pipeable)

```bash
pg_dump -h SRC_HOST -U postgres -d SRC_DB | psql -h DST_HOST -U postgres -d DST_DB -v ON_ERROR_STOP=1
```

`-v ON_ERROR_STOP=1` makes `psql` abort (non-zero) on the first error rather than tolerating a partial load.
Both paths emit `CREATE EXTENSION IF NOT EXISTS vector` (idempotent) and recreate every index
(`USING hnsw`, `USING ivfflat`, btree) by rebuilding it from the restored data. The `vector` extension is
installed into the target database by the restore itself (TheoDB ships the extension binary, so the
`CREATE EXTENSION` always succeeds).

## 3. Verify on the target

```bash
# data — must equal the source checksum from step 1 (whole-row hash)
psql -h DST_HOST -U postgres -d DST_DB -tAc \
  "SELECT md5(string_agg(id::text || '|' || title || '|' || embedding::text, ',' ORDER BY id)) FROM items;"
# indexes — definitions (kind + opclass) preserved
psql -h DST_HOST -U postgres -d DST_DB -c "\d items"
```

The checksums must match exactly; the `\d items` output must list your HNSW / IVFFlat / btree indexes with
the same access methods and opclasses.

## 4. (Optional) Add TheoDB's advanced index after migrating

Once the data is in TheoDB you can add the pgvectorscale StreamingDiskANN index that vanilla PostgreSQL
does not have (match the opclass to your distance metric — the example table is `vector_l2_ops`):

```sql
CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE;
CREATE INDEX items_diskann ON items USING diskann (embedding vector_l2_ops);
```

(See `docs/decisions/m2-index-decision.md` — HNSW is TheoDB's default; DiskANN is for high-dimensional /
large-scale workloads.)

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `type "vector" does not exist` / opclass errors on restore | extension version mismatch (source newer) | upgrade the target extension first (step 0) |
| restore very slow / blocks on a large database | single-statement plain restore + index rebuild at escala | use custom format with parallel restore: `pg_restore --no-owner --exit-on-error -j N` |
| `must be owner of …` / role errors | source roles/ownership absent on target | add `--no-owner` (and `--no-acl` if ACLs reference missing roles) |

## Reproduce it

Bring up a vanilla source + a TheoDB target, then run the smoke:

```bash
docker run -d --name m3-src -e POSTGRES_PASSWORD=postgres pgvector/pgvector:pg17
docker run -d --name m3-dst -e POSTGRES_PASSWORD=postgres theo-db:dev
bash scripts/migrate-smoke.sh            # seed → pg_dump -Fc → pg_restore → assert rows + checksum + index defs + HNSW/IVFFlat usable
bash scripts/migrate-smoke-selftest.sh   # proves the assert is real: corrupt 1 row → verification fails
bash scripts/migrate-doc-check.sh        # proves this guide's commands match the smoke
docker rm -f m3-src m3-dst
```

The migration-smoke CI job runs exactly these against fresh containers.
