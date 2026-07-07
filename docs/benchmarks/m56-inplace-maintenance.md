# M56 — DELETE-path in-place tombstone cost vs the M55 fold wall

Caracterização (NÃO comparação competitiva) do custo do caminho de DELETE do `theodb_hnsw` após o M56 (tombstone in-place) vs o fold whole-index do M55, no MESMO scale (100k×768d), numa única dev box. dim=768, seed=42, mean±std de 3 runs; 10% das linhas deletadas.

**Box load1 no pré-flight:** 2.85 (nproc=12; load-guard aborta se load1 > nproc/2 — lição M46).

**Ambiente:** CPU `13th Gen Intel(R) Core(TM) i7-1355U`; PostgreSQL 17.10 (Debian 17.10-1.pgdg12+1); código `git cabb437`; container `theodb-m56bench`.

## Resultado por escala

### N=100000 (deletadas ~10000 linhas)

| Caminho | VACUUM wall (ms) | Peak private RSS (MB) | Lock EXCL (ms) | VmHWM (MB) | WAL bytes |
|---|---|---|---|---|---|
| **tombstone (DELETE-path)** | 2685.984±1367.987 (n=3) | 1.597±0.009 (n=3) | — | 151.973±0.321 (n=3) | 25497709.333±14711547.312 (n=3) |
| compaction (fold, raro) | 117315.346±23175.436 (n=3) | 1159.23±0.494 (n=3) | 106867.464±18963.0 (n=3) | 1305.033±4.338 (n=3) | 313744394.667±8828045.292 (n=3) |
| _M55 fold baseline (86s @100k)_ | ~86000.0 | ~1440 | ~86000 | — | — |

**Veredito DoD 5:** DELETE-path wall **2685.984 ms** (speedup vs fold ~**32.0×**); ≪ 86 s? **True**. Sem advisory EXCLUSIVE no caminho tombstone? **True** (queries nunca param). RSS do tombstone **1.597 MB** (O(#deletados), não O(N) — vs ~1440 MB do fold).

> The tombstone path replaces the per-DELETE fold; the compaction path (rare, ratio-triggered) keeps the M55-like cost by design — M56 makes the fold RARE, not cheap.

## Metodologia (reprodução)

```bash
PGHOST=localhost PGPORT=55492 PGUSER=postgres PGPASSWORD=postgres \
  THEODB_BENCH_CONTAINER=theodb-m56bench \
  python3 benchmarks/run_m56_inplace_maintenance.py --scales 100000 --dim 768 --runs 3 --delete-frac 0.1
```

Dois modos na mesma tabela: **tombstone** com `theodb.hnsw_tombstone_compact_pct=90` (10% deletado não passa do gatilho → só tombstone) e **compaction** com `theodb.hnsw_tombstone_compact_pct=5` (10% passa → fold M48). Peak RSS de conexão dedicada via `smaps_rollup`; lock via poller ~1ms sobre `pg_locks`; WAL via delta de `pg_current_wal_lsn()` (método M48). Baseline M55 citado de `docs/benchmarks/m55-vacuum-wall.md`.

## Caveats honestos

- The tombstone path's exclusive_lock_ms is None BY DESIGN — `vacuum_delete_inplace` takes NO advisory ExclusiveLock on the tombstone-only path (only the rare compaction fold does). None here is the POSITIVE result (queries never stall), NOT a measurement gap.
- peak_private_rss_mb EXCLUDES shared_buffers (Private_Dirty+Private_Clean); the tombstone sweep mallocs nothing O(N) — it modifies one page at a time — so its private working set is O(#deleted), tiny vs the fold's O(N) ~1.44 GB at 100k (M55).
- The M55 baseline (86 s / 1.44 GB @ 100k×768d) is quoted from docs/benchmarks/m55-vacuum-wall.md, measured on the SAME dev-box class; treat the speedup as same-box characterization, not a portable claim.
- The compaction mode reproduces the fold cost (unchanged from M55) to show the RARE path; 1M for that path is an O(N) PROJECTION (see M55), never measured here.
- WAL bytes are CLUSTER-WIDE in the window (pg_current_wal_lsn delta), mitigated by autovacuum OFF + one connection on a quiet box, not fully isolated (same caveat as M55).
- Characterization on ONE dev box (not a competitive claim); mean±std over the run count; RAM sampled at ~25ms via `docker exec cat smaps_rollup`.
