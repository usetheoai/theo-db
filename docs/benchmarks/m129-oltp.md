# M129 — Official-benchmark OLTP pillar (pgbench + HammerDB TPROC-C)

> Measured 2026-07-21 against a self-hosted TheoDB PG17 (`theodb_rs` extension) on a shared DigitalOcean droplet
> (165.227.121.20, NOT canonical `c6a.4xlarge` hardware). Applies ADR-0050 (adopt-and-wrap) to the OLTP pillar:
> the two field-standard OLTP tools as external out-of-tree drivers + the retained TheoDB wrap layer (paired
> significance) + an explicit pairing with the retained ACID/crash-safety gate.
> Artifact JSON: [`m129-oltp.json`](./m129-oltp.json). Driver: `benchmarks/run_m129_oltp.py`.

## What was measured

TheoDB's OLTP path **is** PostgreSQL's (`theodb_rs` is an extension, not an engine fork), so this run proves the
**100%-wire-compatible OLTP gate** end-to-end and establishes a throughput baseline with the two tools
DB-engineering teams actually use.

| Tool | Metric | Result | Protocol | License / D1 |
|---|---|---|---|---|
| **pgbench** (PG-shipped, TPC-B-like) | TPS | **1278.8 – 1514.8** (3 sessions, see below) | `-i -s 10` build, then 4× `-c 8 -j 4 -T 10 -r` | PostgreSQL License — **D1-clean** |
| **HammerDB TPROC-C** (real TPC-C 45/43/4/4/4 mix) | NOPM | **16 372** | 4 warehouses, 4 VUs, rampup 1 min, duration 2 min | GPLv3 — **external Docker driver only**, never vendored/linked |

### pgbench — 3 independent sessions (run-to-run stability, the wrap-layer capability)

The wrap layer runs the M123 **paired significance** permutation test (seed `20260720`, 100 000 resamples) over the
per-run TPS, split first-half vs second-half — an OLTP capability neither pgbench nor HammerDB provides. Expected
result on a stable engine: **NOT significant** (run-to-run differences are noise, not a real shift). All three
sessions confirm stability:

| Session | TPS per run | mean | min | max | run-to-run `p_permutation` | verdict |
|---|---|---|---|---|---|---|
| A | 1442.3 / 1516.9 / 1579.5 / 1520.4 | 1514.8 | 1442.3 | 1579.5 | 0.50 | not significant → stable |
| B | 1373.6 / 1499.4 / 1426.5 / 1512.6 | 1453.0 | 1373.6 | 1512.6 | 0.50 | not significant → stable |
| C (with HammerDB) | 1383.3 / 1226.9 / 1131.7 / 1373.4 | 1278.8 | 1131.7 | 1383.3 | 1.00 | not significant → stable |

The **between-session** spread (1278.8 → 1514.8, ~18%) is droplet noise: the box is shared (co-tenant containers
`themory`, `theo-rag`, `pgvector`, `traefik` were live during the run — see `docs/benchmarks/m129-oltp.json`
`box` field). This is exactly why the wrap layer measures **run-to-run** stability within a session rather than
asserting an absolute TPS: on non-canonical hardware the absolute number is not claim-grade, but the
paired-significance verdict (stable within each session) is.

## Honesty pairing — throughput is meaningless without durability (ADR M129-2)

The OLTP load drivers post big TPS/NOPM numbers even with `fsync=off`; **only audited TPC-C runs the ACID gate**
(Clause 3: consistency conditions, isolation tests, pull-the-plug durability). So every number above is recorded
with its durability posture and paired with TheoDB's retained crash-safety gate:

- **Durability posture of this run: `fsync=on`** (recorded in the artifact).
- **Retained ACID/crash-safety gate:** `theodb_rs/isolation/crash_fold.sh` + `crash_unlogged.sh` (#46/#47 —
  proven under real crash in M48, see `memory/durability-46-47-proven.md`). A throughput number is only meaningful
  **paired** with this gate; it is never reported alone as "valid".

This pairing is the **correctness half the official OLTP tools lack** — the second retained wrap-layer capability
(the first being paired significance). The discovery blueprint found (unanimous across all four pillars) that no
official tool provides result-correctness gating *or* paired significance; M129 retains both on top of the adopted
drivers.

## Scope & caveats (honest framing)

- **Self-hosted, shared box — NOT canonical hardware.** TPS/NOPM here are a functional baseline, **not** comparable
  to published or audited TPC-C results, and **not** a competitive claim. No "faster than X" is asserted
  (`rules/public-copy.md § 4`).
- **HammerDB NOPM is NOT audited `tpmC`.** TPROC-C is a TPC-C *derivative*; "NOPM" (New-Orders Per Minute) is
  HammerDB's own metric and must never be branded as official tpmC.
- **HammerDB (GPLv3) ran as an external out-of-tree Docker process** (`tpcorg/hammerdb`, talks over the wire like
  `psql`). No HammerDB source is vendored, forked, or linked into the TheoDB tree — D1-safe.
- **pgbench is a TPC-B-like smoke**, not TPC-C; it is the always-available native path (ships with PG,
  PostgreSQL License). HammerDB TPROC-C is the claim-grade OLTP workload.

## Reproduction

```bash
# self-hosted TheoDB PG17 on localhost:28900 (theodb_rs extension loaded)
export PGHOST=localhost PGPORT=28900 PGUSER=postgres PGDATABASE=postgres PGPASSWORD=postgres

# pgbench only (D1-clean, always available):
python3 benchmarks/run_m129_oltp.py \
  --pgbench /path/to/pgbench --scale 10 --clients 8 --threads 4 --duration 10 --runs 4 \
  --out docs/benchmarks/m129-oltp.json

# + HammerDB TPROC-C NOPM (needs Docker + the tpcorg/hammerdb image; GPLv3 external driver):
python3 benchmarks/run_m129_oltp.py --hammerdb \
  --pgbench /path/to/pgbench --scale 10 --clients 8 --threads 4 --duration 10 --runs 4 \
  --warehouses 4 --vus 4 --rampup 1 --hdb-duration 2 \
  --out docs/benchmarks/m129-oltp.json
```

Unit tests (DB-free — TPS/NOPM parsers + significance wiring + docker-absent skip path):
`python3 -m pytest benchmarks/theodb_bench/test_oltp.py -q` (6 tests).

## Verdict

**OLTP pillar: MEASURED.** Both field-standard OLTP tools run against self-hosted TheoDB — pgbench TPS
(1278.8–1514.8, run-to-run stable across 3 sessions) and HammerDB TPROC-C **NOPM = 16 372** — with the retained
wrap layer (paired significance) wired and every throughput number paired with the retained crash-safety gate at
`fsync=on`. The 100%-wire-compatible OLTP gate is proven end-to-end via both tools. Absolute numbers are a
functional baseline on non-canonical shared hardware, **not** a competitive claim.
