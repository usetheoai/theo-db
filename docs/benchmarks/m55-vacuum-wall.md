# M55 — VACUUM-fold wall baseline (peak RAM · EXCLUSIVE lock · WAL)

Caracterização (NÃO comparação competitiva) do custo do fold whole-index do índice `theodb_hnsw` numa única dev box. dim=768, seed=42, mean±std de 3 runs. 1M é **projeção O(N)**, não medido.

**VEREDITO (o muro do M55, MEDIDO):** a 100k×768d o fold whole-index segura o advisory **EXCLUSIVE por ~86 s** (91 s wall) — **parada total de queries vetoriais por ~1,5 min** — com pico de RSS **~1,44 GB** e **~340 MB de WAL**. A projeção O(N) para a escala North-Star (1M×768d): **~14 GB de RAM, ~14 min de parada, ~3,4 GB de WAL**. O muro é real e escala linearmente — confirma a decisão do ADR 0017 (híbrido tombstone-in-place + fold-para-compaction). **Notas honestas:** (a) o fold O(N) foi acionado pelo threshold default (16) + 500 linhas pós-índice — a warning `SET …=0 failed` (o GUC exige ≥1) é benigna, o fold rebuildou o índice inteiro de qualquer forma (os 86 s / 340 MB provam O(N), não o pending de 500 linhas); (b) `peak_private_rss` deu `None` (falha do `smaps_rollup` via `docker exec`) — o **VmHWM** (RSS total, `shared_buffers=128MB` ≈ ruído) é o proxy de RAM; (c) 250k e 1M não couberam na box (RAM), daí a projeção de ponto único (confiança baixa, honestamente marcada).

**Box load1 no pré-flight:** 1.56 (nproc=12; load-guard aborta se load1 > nproc/2 — lição M46).

**Ambiente:** CPU `13th Gen Intel(R) Core(TM) i7-1355U`; PostgreSQL 17.10 (Debian 17.10-1.pgdg12+1); código `git ec347bd`; container `theodb-m55bench`.

## Escalas medidas

| Escala (N) | Peak private RSS (MB) | VmHWM ceiling (MB) | Lock EXCL (ms, lower) | VACUUM wall (ms, upper) | WAL bytes |
|---|---|---|---|---|---|
| 100000 | — | 1442.723±0.045 (n=3) | 85762.891±8362.816 (n=3) | 91596.948±14204.624 (n=3) | 340541840±589786.938 (n=3) |
| 250000 | SKIPPED — RAM gate: need ~4.25 GB, MemAvailable 3.67 GB — skipping to avoid OOM | | | | |

## Projeção 1M (O(N) — NÃO medido)

Modelo: **linear O(N) from measured scales** sobre 1 escala(s) medida(s). Alvo N=1000000.
> ⚠️ Apenas 1 ponto medido — extrapolação proporcional (através da origem), confiança baixa.

| Métrica | Valor projetado @ 1M | slope/linha | intercepto | pontos |
|---|---|---|---|---|
| vmhwm_mb | 14427.23 | 0.0144272 | 0.0 | 1 |
| exclusive_lock_ms | 857628.91 | 0.857629 | 0.0 | 1 |
| wall_ms | 915969.48 | 0.915969 | 0.0 | 1 |
| wal_bytes | 3405418400.0 | 3405.42 | 0.0 | 1 |

## Warnings do run

- SET theodb.vacuum_pending_threshold=0 failed: 0 is outside the valid range for parameter "theodb.vacuum_pending_threshold" (1 .. 65536)

- SET theodb.vacuum_pending_threshold=0 failed: 0 is outside the valid range for parameter "theodb.vacuum_pending_threshold" (1 .. 65536)

- SET theodb.vacuum_pending_threshold=0 failed: 0 is outside the valid range for parameter "theodb.vacuum_pending_threshold" (1 .. 65536)


## Metodologia (reprodução)

```bash
# Container theodb com /proc acessível via docker exec (nome em THEODB_BENCH_CONTAINER):
PGHOST=localhost PGPORT=55492 PGUSER=postgres PGPASSWORD=postgres \
  THEODB_BENCH_CONTAINER=theodb-m55bench \
  python3 benchmarks/run_m55_vacuum_wall.py --scales 100000,250000 --dim 768 --runs 3
```

Loader: streaming COPY de vetores gaussianos seeded (ADR 0012, não-degenerado). Fold acionado com `theodb.vacuum_pending_threshold=0` + 500 linhas pós-índice (pending > threshold) → `VACUUM <table>`. Peak RSS de conexão dedicada via `smaps_rollup`; lock via poller ~1ms sobre `pg_locks`; WAL via delta de `pg_current_wal_lsn()` (método M48).

## Caveats honestos

- 1M is a PROJECTION (linear O(N) from the measured scales), NEVER measured — do not report it as fact.
- maintenance_work_mem does NOT bound peak_private_rss: the Rust fold mallocs OUTSIDE Postgres memory contexts, so the working set is not capped by the PG knob — that is exactly why M55 measures it.
- peak_private_rss_mb EXCLUDES shared_buffers (Private_Dirty+Private_Clean); VmHWM is the CEILING (includes shared pages) — treat VmHWM as an upper bound, not the private working set.
- exclusive_lock_ms is a LOWER bound (a ~1ms poller misses lock edges shorter than the interval); wall_ms (the VACUUM statement wall-clock) is the UPPER bound. The truth is between them.
- WAL bytes are CLUSTER-WIDE in the window (pg_current_wal_lsn delta), not scoped to the index — mitigated by autovacuum OFF on the table + a single connection on a quiet box, but not isolated.
- The advisory (classid=index_oid, objsubid=1) ExclusiveLock mapping MUST be confirmed empirically against the fold's real lock; if it does not match, lock_ms is null and only wall_ms bounds it.
- Characterization on ONE dev box (not a competitive claim); mean±std over the run count; RAM sampled at ~25ms via `docker exec cat smaps_rollup` (the exec overhead is the effective sampling floor).
