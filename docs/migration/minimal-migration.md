# Minimal migration — vanilla PostgreSQL → TheoDB

Move an existing PostgreSQL + `pgvector` database into TheoDB using **standard `pg_dump` / `pg_restore`**.
TheoDB is wire-compatible with PostgreSQL, so there is **no special tool** — your vector data and your
HNSW / IVFFlat indexes come across intact. This is proven end-to-end by `migrate-smoke.sh` (and in CI).

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

```bash
psql -h SRC_HOST -U postgres -d SRC_DB -tAc \
  "SELECT md5(string_agg(embedding::text, ',' ORDER BY id)) FROM items;"
```

Keep this value — you will compare it on the target after the restore to prove the data is bit-exact.

## 2. Migrate

### Option A — custom format (recommended)

```bash
pg_dump -Fc -h SRC_HOST -U postgres -d SRC_DB -f db.dump
pg_restore --no-owner -h DST_HOST -U postgres -d DST_DB db.dump
```

`--no-owner` avoids ownership errors when source roles do not exist on the target. The custom format also
supports parallel restore for big databases: `pg_restore --no-owner -j 4 -d DST_DB db.dump`.

### Option B — plain SQL (simplest, pipeable)

```bash
pg_dump -h SRC_HOST -U postgres -d SRC_DB | psql -h DST_HOST -U postgres -d DST_DB
```

Both paths emit `CREATE EXTENSION IF NOT EXISTS vector` (idempotent — TheoDB already has it) and recreate
every index (`USING hnsw`, `USING ivfflat`, btree) by rebuilding it from the restored data.

## 3. Verify on the target

```bash
# data — must equal the source checksum from step 1
psql -h DST_HOST -U postgres -d DST_DB -tAc \
  "SELECT md5(string_agg(embedding::text, ',' ORDER BY id)) FROM items;"
# indexes — all preserved
psql -h DST_HOST -U postgres -d DST_DB -c "\d items"
```

The checksums must match exactly; the `\d items` output must list your HNSW / IVFFlat / btree indexes.

## 4. (Optional) Add TheoDB's advanced index after migrating

Once the data is in TheoDB you can add the pgvectorscale StreamingDiskANN index that vanilla PostgreSQL
does not have:

```sql
CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE;
CREATE INDEX items_diskann ON items USING diskann (embedding vector_cosine_ops);
```

(See `docs/decisions/m2-index-decision.md` — HNSW is TheoDB's default; DiskANN is for high-dimensional /
large-scale workloads.)

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `type "vector" does not exist` / opclass errors on restore | extension version mismatch (source newer) | upgrade the target extension first (step 0) |
| restore very slow / blocks on a large database | single-statement plain restore + index rebuild at escala | use custom format with parallel restore: `pg_restore --no-owner -j N` |
| `must be owner of …` / role errors | source roles/ownership absent on target | add `--no-owner` (and `--no-acl` if ACLs reference missing roles) |

## Reproduce it

`migrate-smoke.sh` automates exactly this flow (seed a vanilla source → `pg_dump -Fc` → `pg_restore
--no-owner` → assert checksum + indexes + HNSW usable) and runs in CI. `migrate-smoke-selftest.sh` proves
the integrity assert is real by corrupting one row and confirming the verification fails.
