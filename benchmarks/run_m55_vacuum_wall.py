#!/usr/bin/env python3
"""M55 — VACUUM-fold "wall" baseline: peak private RAM + EXCLUSIVE-lock duration + WAL volume of the
whole-index fold of `theodb_hnsw`, at 100k / 500k × 768d, projecting 1M via an O(N) linear model.

This is the MEASUREMENT baseline that motivates the M55 fold-incremental-vs-in-place decision (sibling of
M48's WAL characterization, which measured only WAL at small N/dim). Here we measure the three costs that
gate a 1M-scale whole-index fold:

  (1) **Peak private RSS (PRIMARY)** — a DEDICATED VACUUM backend's `Private_Dirty + Private_Clean` sampled
      from `/proc/<pid>/smaps_rollup` during the VACUUM (the Rust fold mallocs OUTSIDE Postgres memory
      contexts, so `maintenance_work_mem` does NOT bound this — that is the whole point). VmHWM is the
      SECONDARY ceiling (includes shared_buffers, so it is an upper bound not the private working set).
  (2) **EXCLUSIVE-lock duration** — a ~1ms observer poller over `pg_locks` measures how long the fold holds
      the advisory ExclusiveLock on the index (writes blocked). `lock_ms` is a LOWER bound (misses edges
      shorter than the poll interval); `wall_ms` (the VACUUM statement wall-clock) is the UPPER bound.
  (3) **WAL volume** — reuses M48's method: `pg_current_wal_lsn()` delta around the VACUUM → `pg_wal_lsn_diff`.

NOT a competitive claim (public-copy.md): characterization on one dev box, mean±std over >=3 runs, with the
1M figure reported EXPLICITLY as an O(N) PROJECTION (never measured). Reuses the M48 WAL method + M50 env/PG*
conventions + the M46 load-guard (Rule 9 — do not reinvent). The non-degenerate seeded-gaussian streaming
COPY loader (ADR 0012) keeps client RAM O(1) regardless of N. Needs a container (env
`THEODB_BENCH_CONTAINER`) whose `/proc/<pid>` the harness reads via `docker exec`.
"""
import argparse
import json
import os
import random
import statistics
import subprocess
import sys
import threading
import time

import psycopg2

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55492")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
CONTAINER = os.environ.get("THEODB_BENCH_CONTAINER", "theodb-bench")

DIM = 768
SEED = 42
SCALES = [100000, 250000]  # 500k+ nao cabe na dev box (RAM); 1M projetado O(N)
POST_INDEX_ROWS = 500  # rows inserted AFTER the index build → land in pending → trip the fold
# GUC that gates the fold: pending_pages > threshold. Set to 0 so the POST_INDEX_ROWS pending trips it.
# The exact name is confirmed at runtime (SHOW); a SHOW failure is logged as a WARN, not fatal.
GUC_VACUUM_THRESHOLD = "theodb.vacuum_pending_threshold"
RAM_SAMPLE_INTERVAL_S = 0.025  # ~25ms smaps_rollup sampling
LOCK_POLL_INTERVAL_S = 0.001   # ~1ms pg_locks poll
PROJECT_TO = 1_000_000


def _connect():
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD,
                         dbname="postgres", connect_timeout=15)
    c.autocommit = True
    return c


def _load_guard():
    """M46 lesson: a saturated box makes every measurement noise. Abort if load1 exceeds nproc/2."""
    load1 = os.getloadavg()[0]
    nproc = os.cpu_count() or 1
    if load1 > nproc / 2:
        sys.exit(f"load-guard: load1 {load1:.2f} > nproc/2 {nproc / 2:.1f} — box too busy for a stable "
                 f"benchmark (M46 lesson). Retry on a quiet box.")
    return {"load1": round(load1, 2), "nproc": nproc}


def _env_info(cur):
    """Reproducibility disclosure (analysis-golden-rule §3): host CPU, PG version, git SHA. Best-effort —
    a probe that fails is recorded as 'unknown' rather than aborting the run."""
    def _run(cmd):
        try:
            return subprocess.run(cmd, capture_output=True, text=True, timeout=5).stdout.strip()
        except Exception:  # noqa: BLE001 — disclosure is best-effort, never fatal
            return ""
    cpu = ""
    try:
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    try:
        cur.execute("SHOW server_version")
        pg = cur.fetchone()[0]
    except Exception:  # noqa: BLE001
        pg = "unknown"
    sha = _run(["git", "rev-parse", "--short", "HEAD"]) or "unknown"
    dirty = "-dirty" if _run(["git", "status", "--porcelain", "--untracked-files=no"]) else ""
    return {"cpu": cpu or "unknown", "pg_version": pg, "git_sha": sha + dirty, "container": CONTAINER}


# ---------------------------------------------------------------------------------------------------
# Non-degenerate seeded loader (ADR 0012): stream gaussian vectors line-by-line through a read()-only
# file-like into COPY, so client RAM stays O(1) regardless of N. NOT generate_series (colinear/degenerate).
# ---------------------------------------------------------------------------------------------------
def _vec_lines(start, end, dim, seed):
    """Yield COPY text-format lines `id\\t[v0,v1,...]\\n` for ids in [start, end), gaussian(0,1) seeded."""
    rnd = random.Random(seed)
    for i in range(start, end):
        vec = "[" + ",".join(f"{rnd.gauss(0, 1):.4f}" for _ in range(dim)) + "]"
        yield f"{i}\t{vec}\n"


class _CopyReader:
    """Minimal read()-only file-like over a line generator, for psycopg2.copy_expert. Buffers only the
    current chunk, so client-side memory is O(1) independent of N — the row stream is produced lazily."""

    def __init__(self, line_iter):
        self._it = line_iter
        self._buf = ""

    def read(self, size=-1):
        if size is None or size < 0:
            chunks = [self._buf]
            self._buf = ""
            chunks.extend(self._it)
            return "".join(chunks)
        while len(self._buf) < size:
            try:
                self._buf += next(self._it)
            except StopIteration:
                break
        out, self._buf = self._buf[:size], self._buf[size:]
        return out

    # psycopg2's copy_expert only calls read(); readline provided for defensiveness with other callers.
    def readline(self):
        while "\n" not in self._buf:
            try:
                self._buf += next(self._it)
            except StopIteration:
                break
        nl = self._buf.find("\n")
        if nl < 0:
            out, self._buf = self._buf, ""
            return out
        out, self._buf = self._buf[:nl + 1], self._buf[nl + 1:]
        return out


def _copy_vectors(cur, table, start, end, dim, seed):
    cur.copy_expert(f"COPY {table} (id, v) FROM STDIN", _CopyReader(_vec_lines(start, end, dim, seed)))


# ---------------------------------------------------------------------------------------------------
# /proc probes via `docker exec` — peak private RSS + VmHWM ceiling of the dedicated VACUUM backend.
# ---------------------------------------------------------------------------------------------------
def _smaps_private_kb(container, pid):
    """Private_Dirty + Private_Clean (KB) from /proc/<pid>/smaps_rollup — the private working set that
    excludes shared_buffers. Raises on any docker/read failure (the sampler catches + counts it)."""
    out = subprocess.run(["docker", "exec", container, "cat", f"/proc/{pid}/smaps_rollup"],
                         capture_output=True, text=True, timeout=5)
    if out.returncode != 0:
        raise RuntimeError(f"smaps_rollup exit {out.returncode}: {out.stderr.strip()[:80]}")
    priv = 0
    for line in out.stdout.splitlines():
        if line.startswith("Private_Dirty:") or line.startswith("Private_Clean:"):
            priv += int(line.split()[1])
    return priv


def _vmhwm_kb(container, pid, warnings):
    """VmHWM (KB) from /proc/<pid>/status — the high-water mark of resident set (INCLUDES shared_buffers,
    so it is a CEILING, not the private working set). Best-effort."""
    try:
        out = subprocess.run(["docker", "exec", container, "cat", f"/proc/{pid}/status"],
                             capture_output=True, text=True, timeout=5)
        for line in out.stdout.splitlines():
            if line.startswith("VmHWM:"):
                return int(line.split()[1])
        warnings.append(f"VmHWM not found in /proc/{pid}/status")
    except Exception as e:  # noqa: BLE001
        warnings.append(f"VmHWM read failed: {str(e)[:100]}")
    return None


def _clear_refs(container, pid, warnings):
    """Reset VmHWM (echo 5 > clear_refs) so the post-VACUUM VmHWM reflects the fold, not prior peaks.
    Needs a shell for the redirect. Best-effort — a failure is a caveat, not a crash."""
    try:
        out = subprocess.run(
            ["docker", "exec", container, "sh", "-c", f"echo 5 > /proc/{pid}/clear_refs"],
            capture_output=True, text=True, timeout=5)
        if out.returncode != 0:
            warnings.append(f"clear_refs exit {out.returncode}: {out.stderr.strip()[:80]}")
            return False
        return True
    except Exception as e:  # noqa: BLE001
        warnings.append(f"clear_refs failed: {str(e)[:100]}")
        return False


class _RamSampler(threading.Thread):
    """Samples the VACUUM backend's private RSS at ~RAM_SAMPLE_INTERVAL_S, keeping the max seen."""

    def __init__(self, container, pid, interval=RAM_SAMPLE_INTERVAL_S):
        super().__init__(daemon=True)
        self.container, self.pid, self.interval = container, pid, interval
        self.peak_kb = 0
        self.samples = 0
        self.errors = 0
        self._stop_evt = threading.Event()

    def run(self):
        while not self._stop_evt.is_set():
            try:
                kb = _smaps_private_kb(self.container, self.pid)
                if kb > self.peak_kb:
                    self.peak_kb = kb
                self.samples += 1
            except Exception:  # noqa: BLE001 — a dropped sample is counted, never fatal
                self.errors += 1
            self._stop_evt.wait(self.interval)

    def stop(self):
        self._stop_evt.set()


class _LockPoller(threading.Thread):
    """Polls pg_locks (~LOCK_POLL_INTERVAL_S) on an OBSERVER connection for the fold's advisory
    ExclusiveLock on the index, recording first/last time it is seen held.

    NOTE: the advisory (classid, objsubid) mapping used here MUST be confirmed empirically against the
    fold's actual lock — if the fold takes a different lock shape, first_seen stays None and lock_ms is
    reported as null (honest gap), while wall_ms still bounds the duration from above."""

    def __init__(self, index_oid, interval=LOCK_POLL_INTERVAL_S):
        super().__init__(daemon=True)
        self.index_oid = index_oid
        self.interval = interval
        self.first_seen = None
        self.last_seen = None
        self.polls = 0
        self.errors = 0
        self._stop_evt = threading.Event()

    def run(self):
        try:
            conn = _connect()
        except Exception as e:  # noqa: BLE001
            self.errors += 1
            self._err = str(e)[:100]
            return
        cur = conn.cursor()
        while not self._stop_evt.is_set():
            try:
                cur.execute(
                    "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND mode='ExclusiveLock' "
                    "AND granted AND classid=%s AND objsubid=1", (self.index_oid,))
                held = cur.fetchone()[0] > 0
                now = time.perf_counter()
                if held:
                    if self.first_seen is None:
                        self.first_seen = now
                    self.last_seen = now
                self.polls += 1
            except Exception:  # noqa: BLE001
                self.errors += 1
            self._stop_evt.wait(self.interval)
        conn.close()

    def stop(self):
        self._stop_evt.set()

    @property
    def lock_ms(self):
        if self.first_seen is None or self.last_seen is None:
            return None
        return (self.last_seen - self.first_seen) * 1000.0


# ---------------------------------------------------------------------------------------------------
# GUC confirmation + memory gate + one measured VACUUM.
# ---------------------------------------------------------------------------------------------------
def _confirm_guc(cur, warnings):
    """Confirm the fold-threshold GUC name at runtime. On autocommit each statement is its own tx, so a
    SHOW failure does not leave a broken tx — we WARN and proceed with the default behaviour."""
    try:
        cur.execute(f"SHOW {GUC_VACUUM_THRESHOLD}")
        return {"name": GUC_VACUUM_THRESHOLD, "confirmed": True, "default": cur.fetchone()[0]}
    except Exception as e:  # noqa: BLE001
        warnings.append(f"SHOW {GUC_VACUUM_THRESHOLD} failed: {str(e)[:100]} — proceeding without the GUC "
                        "(fold may fire on its own default threshold)")
        return {"name": GUC_VACUUM_THRESHOLD, "confirmed": False, "error": str(e)[:100]}


def _mem_available_gb():
    try:
        for line in open("/proc/meminfo"):
            if line.startswith("MemAvailable:"):
                return int(line.split()[1]) / (1024.0 * 1024.0)  # KB → GB
    except OSError:
        pass
    return None


def _ram_gate(n):
    """Refuse a scale that would risk OOM. Heuristic budget ~ (N/100k * 1.3 + 1.0) GB (index build +
    fold working set). Returns (ok, need_gb, avail_gb)."""
    need_gb = (n / 100000.0) * 1.3 + 1.0
    avail = _mem_available_gb()
    ok = avail is not None and avail >= need_gb
    return ok, round(need_gb, 2), (round(avail, 2) if avail is not None else None)


def _measure_scale_once(n, dim, seed, container, warnings):
    """Build the index over N rows, trip the fold with POST_INDEX_ROWS pending rows, then VACUUM under a
    RAM sampler + lock poller + WAL delta. Returns one measurement dict (or {'error': ...})."""
    conn = _connect()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    table = f"m55bench_{n}"
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    # autovacuum OFF so the WAL delta is dominated by the FOLD, not background vacuum noise.
    cur.execute(f"ALTER TABLE {table} SET (autovacuum_enabled=false)")

    # main streaming load (0..N) → build index (fold target = whole index over N)
    _copy_vectors(cur, table, 0, n, dim, seed)
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING theodb_hnsw (v theodb_hnsw_l2_ops)")
    cur.execute(f"ANALYZE {table}")

    # confirm + zero the threshold so any pending trips the fold; then insert post-index pending rows.
    guc = _confirm_guc(cur, warnings)
    try:
        cur.execute(f"SET {GUC_VACUUM_THRESHOLD} = 0")
    except Exception as e:  # noqa: BLE001
        warnings.append(f"SET {GUC_VACUUM_THRESHOLD}=0 failed: {str(e)[:100]}")
    _copy_vectors(cur, table, n, n + POST_INDEX_ROWS, dim, seed + 1)

    cur.execute(f"SELECT '{table}_idx'::regclass::oid")
    index_oid = int(cur.fetchone()[0])

    # dedicated backend for the VACUUM (so pid/smaps is exactly the fold's backend)
    vconn = _connect()
    vcur = vconn.cursor()
    vcur.execute("SELECT pg_backend_pid()")
    pid = int(vcur.fetchone()[0])

    poller = _LockPoller(index_oid)
    sampler = _RamSampler(container, pid)
    poller.start()
    sampler.start()
    time.sleep(0.05)  # let both threads warm up before the VACUUM

    clear_ok = _clear_refs(container, pid, warnings)  # reset VmHWM

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
    except Exception as e:  # noqa: BLE001 — record the failure honestly; do NOT fabricate a number
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
        result.update({"ram_samples": sampler.samples, "ram_sample_errors": sampler.errors})
        return result
    return {
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


# ---------------------------------------------------------------------------------------------------
# Aggregation + O(N) projection to 1M.
# ---------------------------------------------------------------------------------------------------
_AGG_FIELDS = ["peak_private_rss_mb", "vmhwm_mb", "exclusive_lock_ms", "wall_ms", "wal_bytes"]


def _agg(runs):
    """mean±std±n per metric across the (successful) runs of one scale."""
    out = {}
    for field in _AGG_FIELDS:
        vals = [r[field] for r in runs if isinstance(r, dict) and r.get(field) is not None]
        if vals:
            out[field] = {"mean": round(statistics.mean(vals), 3),
                          "std": round(statistics.pstdev(vals), 3) if len(vals) > 1 else 0.0,
                          "n": len(vals)}
    return out


def _linfit(xs, ys):
    """Least-squares y = a*x + b. Returns (a, b) or None when x has no spread."""
    m = len(xs)
    mx = sum(xs) / m
    my = sum(ys) / m
    denom = sum((x - mx) ** 2 for x in xs)
    if denom == 0:
        return None
    a = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denom
    b = my - a * mx
    return a, b


def _project_1M(measured):
    """Project each metric to N=PROJECT_TO from the MEASURED scales via a linear O(N) model.

    `measured` is a list of (n, agg_dict). With >=2 points → least-squares fit. With exactly 1 point →
    proportional (through-origin) O(N) extrapolation, flagged n_points=1. NEVER reported as measured."""
    points = [(n, a) for (n, a) in measured if a]
    if not points:
        return {"projected": True, "n_points": 0, "note": "no measured scale — cannot project"}
    out = {"projected": True, "model": "linear O(N) from measured scales",
           "n_points": len(points), "target_n": PROJECT_TO, "metrics": {}}
    for field in _AGG_FIELDS:
        xs, ys = [], []
        for n, a in points:
            if field in a:
                xs.append(float(n))
                ys.append(a[field]["mean"])
        if len(xs) >= 2:
            fit = _linfit(xs, ys)
            if fit is None:
                continue
            a_coef, b_coef = fit
            out["metrics"][field] = {"value_at_1M": round(a_coef * PROJECT_TO + b_coef, 3),
                                     "slope_per_row": a_coef, "intercept": round(b_coef, 3),
                                     "n_points": len(xs)}
        elif len(xs) == 1:
            n0, y0 = xs[0], ys[0]
            out["metrics"][field] = {"value_at_1M": round(y0 * (PROJECT_TO / n0), 3),
                                     "slope_per_row": y0 / n0, "intercept": 0.0,
                                     "n_points": 1, "note": "single-point through-origin O(N) extrapolation"}
    return out


CAVEATS = [
    "1M is a PROJECTION (linear O(N) from the measured scales), NEVER measured — do not report it as fact.",
    "maintenance_work_mem does NOT bound peak_private_rss: the Rust fold mallocs OUTSIDE Postgres memory "
    "contexts, so the working set is not capped by the PG knob — that is exactly why M55 measures it.",
    "peak_private_rss_mb EXCLUDES shared_buffers (Private_Dirty+Private_Clean); VmHWM is the CEILING "
    "(includes shared pages) — treat VmHWM as an upper bound, not the private working set.",
    "exclusive_lock_ms is a LOWER bound (a ~1ms poller misses lock edges shorter than the interval); "
    "wall_ms (the VACUUM statement wall-clock) is the UPPER bound. The truth is between them.",
    "WAL bytes are CLUSTER-WIDE in the window (pg_current_wal_lsn delta), not scoped to the index — "
    "mitigated by autovacuum OFF on the table + a single connection on a quiet box, but not isolated.",
    "The advisory (classid=index_oid, objsubid=1) ExclusiveLock mapping MUST be confirmed empirically "
    "against the fold's real lock; if it does not match, lock_ms is null and only wall_ms bounds it.",
    "Characterization on ONE dev box (not a competitive claim); mean±std over the run count; RAM sampled "
    "at ~25ms via `docker exec cat smaps_rollup` (the exec overhead is the effective sampling floor).",
]


def run(scales, dim, seed, runs, container):
    load = _load_guard()
    conn = _connect()
    cur = conn.cursor()
    env = _env_info(cur)
    conn.close()

    warnings = []
    scale_out = {}
    measured_for_projection = []
    for n in scales:
        ok, need_gb, avail_gb = _ram_gate(n)
        if not ok:
            scale_out[str(n)] = {"skipped": True,
                                 "reason": f"RAM gate: need ~{need_gb} GB, MemAvailable "
                                           f"{avail_gb} GB — skipping to avoid OOM"}
            continue
        run_results = []
        for _ in range(runs):
            run_results.append(_measure_scale_once(n, dim, seed, container, warnings))
        agg = _agg(run_results)
        errs = [r["error"] for r in run_results if isinstance(r, dict) and "error" in r]
        scale_out[str(n)] = {"measured": True, "ram_gate": {"need_gb": need_gb, "avail_gb": avail_gb},
                             "runs_raw": run_results, "agg": agg, "errors": errs}
        if agg:
            measured_for_projection.append((n, agg))

    projection = _project_1M(measured_for_projection)
    return {
        "milestone": "M55", "dim": dim, "seed": seed, "runs": runs,
        "scales_requested": scales, "post_index_rows": POST_INDEX_ROWS,
        "guc_vacuum_threshold": GUC_VACUUM_THRESHOLD,
        "load": load, "env": env,
        "scales": scale_out, "projection_1M": projection,
        "warnings": warnings, "caveats": CAVEATS,
    }


def _write_md(data, path):
    def cell(agg, field):
        c = agg.get(field) if agg else None
        return f"{c['mean']}±{c['std']} (n={c['n']})" if c else "—"

    lines = [
        "# M55 — VACUUM-fold wall baseline (peak RAM · EXCLUSIVE lock · WAL)", "",
        "Caracterização (NÃO comparação competitiva) do custo do fold whole-index do índice `theodb_hnsw` "
        f"numa única dev box. dim={data['dim']}, seed={data['seed']}, mean±std de {data['runs']} runs. "
        "1M é **projeção O(N)**, não medido.", "",
        f"**Box load1 no pré-flight:** {data['load']['load1']} (nproc={data['load']['nproc']}; load-guard "
        "aborta se load1 > nproc/2 — lição M46).", "",
        f"**Ambiente:** CPU `{data['env'].get('cpu', 'unknown')}`; PostgreSQL "
        f"{data['env'].get('pg_version', 'unknown')}; código `git {data['env'].get('git_sha', 'unknown')}`; "
        f"container `{data['env'].get('container', 'unknown')}`.", "",
        "## Escalas medidas", "",
        "| Escala (N) | Peak private RSS (MB) | VmHWM ceiling (MB) | Lock EXCL (ms, lower) | "
        "VACUUM wall (ms, upper) | WAL bytes |",
        "|---|---|---|---|---|---|",
    ]
    for n in data["scales_requested"]:
        s = data["scales"].get(str(n), {})
        if s.get("skipped"):
            lines.append(f"| {n} | SKIPPED — {s.get('reason', '')} | | | | |")
            continue
        agg = s.get("agg", {})
        lines.append(f"| {n} | {cell(agg, 'peak_private_rss_mb')} | {cell(agg, 'vmhwm_mb')} | "
                     f"{cell(agg, 'exclusive_lock_ms')} | {cell(agg, 'wall_ms')} | "
                     f"{cell(agg, 'wal_bytes')} |")

    proj = data["projection_1M"]
    lines += ["", "## Projeção 1M (O(N) — NÃO medido)", ""]
    if proj.get("metrics"):
        lines.append(f"Modelo: **{proj.get('model', 'linear O(N)')}** sobre {proj.get('n_points')} "
                     f"escala(s) medida(s). Alvo N={proj.get('target_n')}.")
        if proj.get("n_points") == 1:
            lines.append("> ⚠️ Apenas 1 ponto medido — extrapolação proporcional (através da origem), "
                         "confiança baixa.")
        lines += ["", "| Métrica | Valor projetado @ 1M | slope/linha | intercepto | pontos |",
                  "|---|---|---|---|---|"]
        for field, m in proj["metrics"].items():
            lines.append(f"| {field} | {m['value_at_1M']} | {m['slope_per_row']:.6g} | "
                         f"{m['intercept']} | {m['n_points']} |")
    else:
        lines.append(f"Projeção indisponível: {proj.get('note', 'sem escalas medidas')}.")

    if data["warnings"]:
        lines += ["", "## Warnings do run", ""]
        lines += [f"- {w}" for w in data["warnings"]]

    lines += ["", "## Metodologia (reprodução)", "", "```bash",
              "# Container theodb com /proc acessível via docker exec (nome em THEODB_BENCH_CONTAINER):",
              "PGHOST=localhost PGPORT=55492 PGUSER=postgres PGPASSWORD=postgres \\",
              f"  THEODB_BENCH_CONTAINER={data['env'].get('container', 'theodb-bench')} \\",
              f"  python3 benchmarks/run_m55_vacuum_wall.py --scales {','.join(map(str, data['scales_requested']))} "
              f"--dim {data['dim']} --runs {data['runs']}", "```", "",
              f"Loader: streaming COPY de vetores gaussianos seeded (ADR 0012, não-degenerado). "
              f"Fold acionado com `{data['guc_vacuum_threshold']}=0` + {data['post_index_rows']} linhas "
              "pós-índice (pending > threshold) → `VACUUM <table>`. Peak RSS de conexão dedicada via "
              "`smaps_rollup`; lock via poller ~1ms sobre `pg_locks`; WAL via delta de `pg_current_wal_lsn()` "
              "(método M48).", "",
              "## Caveats honestos", ""]
    lines += [f"- {c}" for c in data["caveats"]]
    lines.append("")
    with open(path, "w") as f:
        f.write("\n".join(lines))


def _parse_scales(raw):
    return [int(x) for x in raw.split(",") if x.strip()]


def main():
    ap = argparse.ArgumentParser(description="M55 VACUUM-fold wall baseline (peak RAM · lock · WAL).")
    ap.add_argument("--scales", type=_parse_scales,
                    default=os.environ.get("M55_SCALES", ",".join(map(str, SCALES))))
    ap.add_argument("--dim", type=int, default=int(os.environ.get("M55_DIM", str(DIM))))
    ap.add_argument("--seed", type=int, default=int(os.environ.get("M55_SEED", str(SEED))))
    ap.add_argument("--runs", type=int, default=int(os.environ.get("M55_RUNS", "3")))
    ap.add_argument("--out-json", default="benchmarks/artifacts/m55-vacuum-wall.json")
    ap.add_argument("--out-md", default="wiki/benchmarks/m55-vacuum-wall.md")
    args = ap.parse_args()

    scales = args.scales if isinstance(args.scales, list) else _parse_scales(args.scales)
    data = run(scales, args.dim, args.seed, args.runs, CONTAINER)

    os.makedirs(os.path.dirname(args.out_json), exist_ok=True)
    with open(args.out_json, "w") as f:
        json.dump(data, f, indent=2)
    _write_md(data, args.out_md)
    print(f"wrote {args.out_json} and {args.out_md} "
          f"(scales={scales}, dim={args.dim}, runs={args.runs})")
    for n in scales:
        s = data["scales"].get(str(n), {})
        if s.get("skipped"):
            print(f"  N={n}: SKIPPED — {s.get('reason', '')}")
        else:
            agg = s.get("agg", {})
            rss = agg.get("peak_private_rss_mb", {}).get("mean")
            wal = agg.get("wal_bytes", {}).get("mean")
            lock = agg.get("exclusive_lock_ms", {}).get("mean")
            print(f"  N={n}: peak_rss={rss} MB, wal={wal} B, lock={lock} ms")
    if data["projection_1M"].get("metrics"):
        rss1m = data["projection_1M"]["metrics"].get("peak_private_rss_mb", {}).get("value_at_1M")
        print(f"  1M PROJECTED (O(N), not measured): peak_rss≈{rss1m} MB")


if __name__ == "__main__":
    main()
