#!/usr/bin/env python3
"""M56 — in-place tombstone DELETE-path cost vs the M55 whole-index fold "wall".

The deep-view (2026-07-07) measured the M55 baseline: the O(N) whole-index fold holds the advisory
EXCLUSIVE lock ~86 s at 100k×768d (total stall of vector queries) with ~1.44 GB peak private RSS
(`docs/benchmarks/m55-vacuum-wall.md`). M56 replaces the DELETE path with **in-place tombstones**
(`ambulkdelete` → `vacuum_delete_inplace`): each dead node is flagged on its own page under GenericXLog,
with **NO advisory EXCLUSIVE** and **NO O(N) rebuild**. The rare O(N) compaction (the M48 fold) runs ONLY
when tombstones exceed `theodb.hnsw_tombstone_compact_pct` of the graph.

This harness measures, at the SAME 100k×768d scale as M55 (apples-to-apples), the DELETE-path VACUUM in
two modes on one table:

  - **tombstone** — `hnsw_tombstone_compact_pct` set HIGH so the delete fraction does NOT trip compaction →
    the pure tombstone-only cost. EXPECTED: wall ≪ 86 s, peak private RSS O(#deleted) not O(N) (the sweep
    mallocs nothing proportional to N — it reads/writes one page at a time), and NO advisory ExclusiveLock
    (lock_ms is None — the POSITIVE result, i.e. queries never stall, NOT a measurement gap).
  - **compaction** — `hnsw_tombstone_compact_pct` set LOW so the same delete fraction trips the fold → the
    M55-like cost, reported to show what the RARE compaction still costs (unchanged from M55, by design:
    M56 makes it rare, not cheap).

Reuses the M55 machinery verbatim (loader, RAM sampler, lock poller, WAL delta, env/load-guard) via import
(Rule 9 — do not reinvent). NOT a competitive claim (`public-copy.md`): characterization on one dev box,
mean±std over >=3 runs; 1M is an O(N) PROJECTION for the compaction path only, never measured. Needs a
container (env `THEODB_BENCH_CONTAINER`) whose `/proc/<pid>` the harness reads via `docker exec`, running an
image built from the M56 code (the `deleted`/`version` element-tuple layout + the compaction GUC).
"""
import argparse
import json
import os
import statistics
import sys
import time

# Reuse the M55 harness machinery verbatim (Rule 9 / DRY) — same box, same loader, same probes.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from run_m55_vacuum_wall import (  # noqa: E402
    _clear_refs,
    _connect,
    _copy_vectors,
    _env_info,
    _load_guard,
    _mem_available_gb,
    _LockPoller,
    _RamSampler,
    _vmhwm_kb,
)

DIM = 768
SEED = 42
SCALES = [100000]  # apples-to-apples with the M55 baseline (100k×768d); 250k optional via --scales
DELETE_FRAC = 0.10  # delete 10% of rows → creates dead heap tuples for ambulkdelete to sweep
COMPACT_GUC = "theodb.hnsw_tombstone_compact_pct"
# tombstone mode: pct HIGH so 10% deleted does NOT exceed it → NO compaction (pure tombstone sweep).
# compaction mode: pct LOW so 10% deleted DOES exceed it → the M48 fold runs (the rare path).
PCT_NO_COMPACT = 90
PCT_FORCE_COMPACT = 5
M55_BASELINE_WALL_MS_100K = 86000.0  # the measured M55 fold stall at 100k×768d (docs/benchmarks/m55-*.md)
PROJECT_TO = 1_000_000


def _ram_gate(n):
    """Refuse a scale that would risk OOM (same heuristic budget as M55: index build + a fold working set)."""
    need_gb = (n / 100000.0) * 1.3 + 1.0
    avail = _mem_available_gb()
    ok = avail is not None and avail >= need_gb
    return ok, round(need_gb, 2), (round(avail, 2) if avail is not None else None)


def _set_guc(cur, name, value, warnings):
    try:
        cur.execute(f"SET {name} = {value}")
        return True
    except Exception as e:  # noqa: BLE001 — a missing GUC is a WARN, not a fabricated pass
        warnings.append(f"SET {name}={value} failed: {str(e)[:120]}")
        return False


def _measure_delete_once(n, dim, seed, container, delete_frac, compact_pct, mode, warnings):
    """Build the index over N rows, DELETE `delete_frac` of them, then VACUUM under a RAM sampler + lock
    poller + WAL delta with the compaction GUC set for `mode`. Returns one measurement dict (or {'error'})."""
    conn = _connect()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = f"m56bench_{mode}_{n}"
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    cur.execute(f"ALTER TABLE {table} SET (autovacuum_enabled=false)")  # WAL/RSS dominated by our VACUUM

    _copy_vectors(cur, table, 0, n, dim, seed)
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING theodb_hnsw (v theodb_hnsw_l2_ops)")
    cur.execute(f"ANALYZE {table}")

    # Confirm the compaction GUC exists (honest gap if not) — the whole tombstone-vs-fold split hinges on it.
    guc = {"name": COMPACT_GUC, "mode": mode, "pct": compact_pct}
    try:
        cur.execute(f"SHOW {COMPACT_GUC}")
        guc["confirmed"] = True
        guc["default"] = cur.fetchone()[0]
    except Exception as e:  # noqa: BLE001
        guc["confirmed"] = False
        guc["error"] = str(e)[:120]
        warnings.append(f"SHOW {COMPACT_GUC} failed: {str(e)[:120]} — image may predate M56 (fold will run)")

    # DELETE a deterministic prefix → dead heap tuples for ambulkdelete's callback to report.
    cutoff = int(n * delete_frac)
    cur.execute(f"DELETE FROM {table} WHERE id < {cutoff}")
    n_deleted = cur.rowcount

    cur.execute(f"SELECT '{table}_idx'::regclass::oid")
    index_oid = int(cur.fetchone()[0])

    # Dedicated backend for the VACUUM (so pid/smaps is exactly the ambulkdelete backend). The compaction
    # GUC MUST be set on THIS session — ambulkdelete reads it in the VACUUM backend, not the loader's.
    vconn = _connect()
    vcur = vconn.cursor()
    # Load theodb_rs in THIS backend so its GUCs (registered in _PG_init) are recognized by the SET below —
    # a fresh connection has not yet loaded the library (unlike the loader conn, which did via CREATE INDEX).
    try:
        vcur.execute("LOAD 'theodb_rs'")
    except Exception as e:  # noqa: BLE001
        warnings.append(f"LOAD theodb_rs failed on VACUUM conn: {str(e)[:120]}")
    _set_guc(vcur, COMPACT_GUC, compact_pct, warnings)
    vcur.execute("SELECT pg_backend_pid()")
    pid = int(vcur.fetchone()[0])

    poller = _LockPoller(index_oid)  # sees the advisory EXCLUSIVE only if the fold (compaction) runs
    sampler = _RamSampler(container, pid)
    poller.start()
    sampler.start()
    time.sleep(0.05)
    clear_ok = _clear_refs(container, pid, warnings)

    result = {}
    try:
        vcur.execute("SELECT pg_current_wal_lsn()")
        lsn0 = vcur.fetchone()[0]
        t0 = time.perf_counter()
        vcur.execute(f"VACUUM {table}")
        wall_ms = (time.perf_counter() - t0) * 1000.0
        vcur.execute("SELECT pg_current_wal_lsn()")
        lsn1 = vcur.fetchone()[0]
        vcur.execute("SELECT pg_wal_lsn_diff(%s, %s)", (lsn1, lsn0))
        wal_bytes = int(vcur.fetchone()[0])
    except Exception as e:  # noqa: BLE001 — record the failure honestly; never fabricate a number
        result = {"error": f"VACUUM failed: {str(e)[:150]}"}
        wall_ms = wal_bytes = None
    finally:
        sampler.stop()
        poller.stop()
        sampler.join(timeout=3)
        poller.join(timeout=3)

    vmhwm_kb = _vmhwm_kb(container, pid, warnings)
    vconn.close()
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    conn.close()

    if "error" in result:
        result.update({"mode": mode, "n_deleted": n_deleted, "ram_samples": sampler.samples})
        return result
    return {
        "mode": mode,
        "n_deleted": n_deleted,
        "peak_private_rss_mb": round(sampler.peak_kb / 1024.0, 2) if sampler.peak_kb else None,
        "vmhwm_mb": round(vmhwm_kb / 1024.0, 2) if vmhwm_kb else None,
        "exclusive_lock_ms": round(poller.lock_ms, 3) if poller.lock_ms is not None else None,
        "wall_ms": round(wall_ms, 3),
        "wal_bytes": wal_bytes,
        "ram_samples": sampler.samples,
        "ram_sample_errors": sampler.errors,
        "lock_polls": poller.polls,
        "clear_refs_ok": clear_ok,
        "guc": guc,
    }


_AGG_FIELDS = ["peak_private_rss_mb", "vmhwm_mb", "exclusive_lock_ms", "wall_ms", "wal_bytes"]


def _agg(runs):
    out = {}
    for field in _AGG_FIELDS:
        vals = [r[field] for r in runs if isinstance(r, dict) and r.get(field) is not None]
        if vals:
            out[field] = {"mean": round(statistics.mean(vals), 3),
                          "std": round(statistics.pstdev(vals), 3) if len(vals) > 1 else 0.0,
                          "n": len(vals)}
    return out


def _verdict(tomb_agg, comp_agg, n_deleted_mean):
    """DoD 5 gate: the DELETE-path (tombstone) stall must be ≪ the M55 fold (86 s) AND its RAM O(#deleted),
    not O(N). Returns a structured verdict; NEVER asserts superiority without the measured numbers."""
    v = {"m55_baseline_wall_ms_100k": M55_BASELINE_WALL_MS_100K}
    tw = tomb_agg.get("wall_ms", {}).get("mean") if tomb_agg else None
    tr = tomb_agg.get("peak_private_rss_mb", {}).get("mean") if tomb_agg else None
    tl = tomb_agg.get("exclusive_lock_ms", {}).get("mean") if tomb_agg else None
    v["tombstone_wall_ms"] = tw
    v["tombstone_peak_rss_mb"] = tr
    v["tombstone_exclusive_lock_ms"] = tl  # None ⇒ the fold's advisory EXCLUSIVE was NEVER taken (queries never stall)
    if tw is not None:
        v["speedup_vs_m55_fold"] = round(M55_BASELINE_WALL_MS_100K / tw, 1) if tw > 0 else None
        v["delete_path_much_less_than_86s"] = tw < (M55_BASELINE_WALL_MS_100K * 0.1)  # ≪ = at least 10× faster
    v["no_advisory_exclusive_in_tombstone_path"] = (tl is None)
    if comp_agg:
        v["compaction_wall_ms"] = comp_agg.get("wall_ms", {}).get("mean")
        v["compaction_peak_rss_mb"] = comp_agg.get("peak_private_rss_mb", {}).get("mean")
        v["compaction_exclusive_lock_ms"] = comp_agg.get("exclusive_lock_ms", {}).get("mean")
    v["note"] = ("The tombstone path replaces the per-DELETE fold; the compaction path (rare, ratio-triggered) "
                 "keeps the M55-like cost by design — M56 makes the fold RARE, not cheap.")
    return v


CAVEATS = [
    "The tombstone path's exclusive_lock_ms is None BY DESIGN — `vacuum_delete_inplace` takes NO advisory "
    "ExclusiveLock on the tombstone-only path (only the rare compaction fold does). None here is the "
    "POSITIVE result (queries never stall), NOT a measurement gap.",
    "peak_private_rss_mb EXCLUDES shared_buffers (Private_Dirty+Private_Clean); the tombstone sweep mallocs "
    "nothing O(N) — it modifies one page at a time — so its private working set is O(#deleted), tiny vs the "
    "fold's O(N) ~1.44 GB at 100k (M55).",
    "The M55 baseline (86 s / 1.44 GB @ 100k×768d) is quoted from docs/benchmarks/m55-vacuum-wall.md, "
    "measured on the SAME dev-box class; treat the speedup as same-box characterization, not a portable claim.",
    "The compaction mode reproduces the fold cost (unchanged from M55) to show the RARE path; 1M for that "
    "path is an O(N) PROJECTION (see M55), never measured here.",
    "WAL bytes are CLUSTER-WIDE in the window (pg_current_wal_lsn delta), mitigated by autovacuum OFF + one "
    "connection on a quiet box, not fully isolated (same caveat as M55).",
    "Characterization on ONE dev box (not a competitive claim); mean±std over the run count; RAM sampled at "
    "~25ms via `docker exec cat smaps_rollup`.",
]


def run(scales, dim, seed, runs, container, delete_frac):
    load = _load_guard()
    conn = _connect()
    cur = conn.cursor()
    env = _env_info(cur)
    conn.close()

    warnings = []
    scale_out = {}
    for n in scales:
        ok, need_gb, avail_gb = _ram_gate(n)
        if not ok:
            scale_out[str(n)] = {"skipped": True,
                                 "reason": f"RAM gate: need ~{need_gb} GB, MemAvailable {avail_gb} GB"}
            continue
        tomb_runs, comp_runs = [], []
        for _ in range(runs):
            tomb_runs.append(_measure_delete_once(n, dim, seed, container, delete_frac,
                                                  PCT_NO_COMPACT, "tombstone", warnings))
            comp_runs.append(_measure_delete_once(n, dim, seed, container, delete_frac,
                                                  PCT_FORCE_COMPACT, "compaction", warnings))
        tomb_agg, comp_agg = _agg(tomb_runs), _agg(comp_runs)
        n_del = statistics.mean([r["n_deleted"] for r in tomb_runs if r.get("n_deleted")]) if tomb_runs else 0
        scale_out[str(n)] = {
            "measured": True, "ram_gate": {"need_gb": need_gb, "avail_gb": avail_gb},
            "delete_frac": delete_frac, "n_deleted_mean": n_del,
            "tombstone": {"runs_raw": tomb_runs, "agg": tomb_agg,
                          "errors": [r["error"] for r in tomb_runs if isinstance(r, dict) and "error" in r]},
            "compaction": {"runs_raw": comp_runs, "agg": comp_agg,
                           "errors": [r["error"] for r in comp_runs if isinstance(r, dict) and "error" in r]},
            "verdict": _verdict(tomb_agg, comp_agg, n_del),
        }
    return {
        "milestone": "M56", "dim": dim, "seed": seed, "runs": runs, "scales_requested": scales,
        "delete_frac": delete_frac, "compact_guc": COMPACT_GUC,
        "pct_no_compact": PCT_NO_COMPACT, "pct_force_compact": PCT_FORCE_COMPACT,
        "load": load, "env": env, "scales": scale_out, "warnings": warnings, "caveats": CAVEATS,
    }


def _write_md(data, path):
    def cell(agg, field):
        c = agg.get(field) if agg else None
        return f"{c['mean']}±{c['std']} (n={c['n']})" if c else "—"

    lines = [
        "# M56 — DELETE-path in-place tombstone cost vs the M55 fold wall", "",
        "Caracterização (NÃO comparação competitiva) do custo do caminho de DELETE do `theodb_hnsw` após o "
        "M56 (tombstone in-place) vs o fold whole-index do M55, no MESMO scale (100k×768d), numa única dev "
        f"box. dim={data['dim']}, seed={data['seed']}, mean±std de {data['runs']} runs; "
        f"{int(data['delete_frac'] * 100)}% das linhas deletadas.", "",
        f"**Box load1 no pré-flight:** {data['load']['load1']} (nproc={data['load']['nproc']}; load-guard "
        "aborta se load1 > nproc/2 — lição M46).", "",
        f"**Ambiente:** CPU `{data['env'].get('cpu', 'unknown')}`; PostgreSQL "
        f"{data['env'].get('pg_version', 'unknown')}; código `git {data['env'].get('git_sha', 'unknown')}`; "
        f"container `{data['env'].get('container', 'unknown')}`.", "",
        "## Resultado por escala", "",
    ]
    for n in data["scales_requested"]:
        s = data["scales"].get(str(n), {})
        if s.get("skipped"):
            lines += [f"### N={n} — SKIPPED", "", s.get("reason", ""), ""]
            continue
        ta, ca, vd = s["tombstone"]["agg"], s["compaction"]["agg"], s["verdict"]
        lines += [
            f"### N={n} (deletadas ~{int(s['n_deleted_mean'])} linhas)", "",
            "| Caminho | VACUUM wall (ms) | Peak private RSS (MB) | Lock EXCL (ms) | VmHWM (MB) | WAL bytes |",
            "|---|---|---|---|---|---|",
            f"| **tombstone (DELETE-path)** | {cell(ta, 'wall_ms')} | {cell(ta, 'peak_private_rss_mb')} | "
            f"{cell(ta, 'exclusive_lock_ms')} | {cell(ta, 'vmhwm_mb')} | {cell(ta, 'wal_bytes')} |",
            f"| compaction (fold, raro) | {cell(ca, 'wall_ms')} | {cell(ca, 'peak_private_rss_mb')} | "
            f"{cell(ca, 'exclusive_lock_ms')} | {cell(ca, 'vmhwm_mb')} | {cell(ca, 'wal_bytes')} |",
            f"| _M55 fold baseline (86s @100k)_ | ~{data['scales'][str(n)]['verdict']['m55_baseline_wall_ms_100k']} | ~1440 | ~86000 | — | — |",
            "",
            f"**Veredito DoD 5:** DELETE-path wall **{vd.get('tombstone_wall_ms')} ms** "
            f"(speedup vs fold ~**{vd.get('speedup_vs_m55_fold')}×**); "
            f"≪ 86 s? **{vd.get('delete_path_much_less_than_86s')}**. "
            f"Sem advisory EXCLUSIVE no caminho tombstone? **{vd.get('no_advisory_exclusive_in_tombstone_path')}** "
            f"(queries nunca param). RSS do tombstone **{vd.get('tombstone_peak_rss_mb')} MB** "
            "(O(#deletados), não O(N) — vs ~1440 MB do fold).", "",
            f"> {vd.get('note')}", "",
        ]

    if data["warnings"]:
        lines += ["## Warnings do run", ""] + [f"- {w}" for w in data["warnings"]] + [""]

    lines += ["## Metodologia (reprodução)", "", "```bash",
              f"PGHOST=localhost PGPORT=55492 PGUSER=postgres PGPASSWORD=postgres \\",
              f"  THEODB_BENCH_CONTAINER={data['env'].get('container', 'theodb-bench')} \\",
              f"  python3 benchmarks/run_m56_inplace_maintenance.py "
              f"--scales {','.join(map(str, data['scales_requested']))} --dim {data['dim']} "
              f"--runs {data['runs']} --delete-frac {data['delete_frac']}", "```", "",
              "Dois modos na mesma tabela: **tombstone** com "
              f"`{data['compact_guc']}={data['pct_no_compact']}` (10% deletado não passa do gatilho → só "
              f"tombstone) e **compaction** com `{data['compact_guc']}={data['pct_force_compact']}` (10% "
              "passa → fold M48). Peak RSS de conexão dedicada via `smaps_rollup`; lock via poller ~1ms sobre "
              "`pg_locks`; WAL via delta de `pg_current_wal_lsn()` (método M48). Baseline M55 citado de "
              "`docs/benchmarks/m55-vacuum-wall.md`.", "",
              "## Caveats honestos", ""]
    lines += [f"- {c}" for c in data["caveats"]] + [""]
    with open(path, "w") as f:
        f.write("\n".join(lines))


def _parse_scales(raw):
    if isinstance(raw, list):
        return raw
    return [int(x) for x in raw.split(",") if x.strip()]


def main():
    ap = argparse.ArgumentParser(description="M56 in-place tombstone DELETE-path cost vs the M55 fold wall.")
    ap.add_argument("--scales", type=_parse_scales,
                    default=os.environ.get("M56_SCALES", ",".join(map(str, SCALES))))
    ap.add_argument("--dim", type=int, default=int(os.environ.get("M56_DIM", str(DIM))))
    ap.add_argument("--seed", type=int, default=int(os.environ.get("M56_SEED", str(SEED))))
    ap.add_argument("--runs", type=int, default=int(os.environ.get("M56_RUNS", "3")))
    ap.add_argument("--delete-frac", type=float, default=float(os.environ.get("M56_DELETE_FRAC", str(DELETE_FRAC))))
    ap.add_argument("--out-json", default="docs/benchmarks/m56-inplace-maintenance.json")
    ap.add_argument("--out-md", default="docs/benchmarks/m56-inplace-maintenance.md")
    args = ap.parse_args()

    container = os.environ.get("THEODB_BENCH_CONTAINER", "theodb-bench")
    data = run(args.scales, args.dim, args.seed, args.runs, container, args.delete_frac)
    os.makedirs(os.path.dirname(args.out_json), exist_ok=True)
    with open(args.out_json, "w") as f:
        json.dump(data, f, indent=2)
    _write_md(data, args.out_md)
    print(json.dumps({"milestone": "M56", "scales": list(data["scales"].keys()),
                      "out_json": args.out_json, "out_md": args.out_md,
                      "verdicts": {k: v.get("verdict") for k, v in data["scales"].items()
                                   if isinstance(v, dict) and "verdict" in v}}, indent=2))


if __name__ == "__main__":
    main()
