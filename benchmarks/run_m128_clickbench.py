#!/usr/bin/env python3
"""M128 — official-benchmark COLUMNAR pillar: run the 43 ClickBench queries over `theodb_columnar` and prove the
retained byte-identical result A/B (columnar vs heap) — the correctness oracle ClickBench lacks (blueprint Q10/Q11).

ClickBench protocol (faithful): load the `hits` table, run each of the 43 queries 3× (cold = 1st after a cache
flush, hot = min of the 2 warm runs), record the raw [t1,t2,t3] triple per query into a ClickBench-format
results.json. PLUS the wrap layer: EXPLAIN each query (columnar CustomScan vs native), and for each query compare
the columnar result rows vs a heap copy — byte-identical or a loud divergence (ClickBench's `check` is a `SELECT 1`;
it validates NOTHING).

Honesty rails (ADR M128-2, CLAUDE.md rule 5): `hits` is CC-BY-NC-SA → CI-downloaded (streamed, subsampled),
NEVER vendored. Self-hosted box (labeled), NOT canonical c6a.4xlarge. No data → UNBENCHMARKED, clean exit.
A query unsupported by theodb_columnar is recorded ERRORED with its typed message — never silently skipped.
"""
import argparse
import json
import math
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
import urllib.request

import psycopg2

HITS_TSV_GZ = "https://datasets.clickhouse.com/hits_compatible/hits.tsv.gz"  # CC-BY-NC-SA — CI-only, never vendored
_UA = "Mozilla/5.0 (X11; Linux x86_64)"
HERE = os.path.dirname(os.path.abspath(__file__))
ENTRY = os.path.join(HERE, "clickbench", "theodb")


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "28900"),
        dbname=os.environ.get("PGDATABASE", "postgres"), user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"))


_CB_RAW = "https://raw.githubusercontent.com/ClickHouse/ClickBench/main/postgresql"  # CC-BY-NC-SA — CI-fetch, never vendor


def ensure_entry_sql() -> bool:
    """Fetch ClickBench's create.sql + queries.sql at runtime (CC-BY-NC-SA → CI-download, NOT vendored into our
    Apache tree, per the D1 gate). Adapts create.sql to `USING theodb_columnar`. Returns True on success."""
    create_p, queries_p = os.path.join(ENTRY, "create.sql"), os.path.join(ENTRY, "queries.sql")
    if os.path.isfile(create_p) and os.path.isfile(queries_p):
        return True
    try:
        os.makedirs(ENTRY, exist_ok=True)
        req = urllib.request.Request(f"{_CB_RAW}/queries.sql", headers={"User-Agent": _UA})  # noqa: S310
        with urllib.request.urlopen(req, timeout=60) as r:
            open(queries_p, "wb").write(r.read())
        req = urllib.request.Request(f"{_CB_RAW}/create.sql", headers={"User-Agent": _UA})  # noqa: S310
        with urllib.request.urlopen(req, timeout=60) as r:
            ddl = r.read().decode().rstrip()
        if not ddl.endswith(");"):
            raise ValueError("unexpected ClickBench create.sql ending (want '…);')")
        ddl = ddl[:-2] + ") USING theodb_columnar;\n"  # swap the final ');' for ') USING theodb_columnar;'
        open(create_p, "w").write(
            "-- Fetched from ClickBench (CC-BY-NC-SA) at runtime, NOT vendored — only the AM clause is ours.\n" + ddl)
        return True
    except Exception as e:
        print(f"  ClickBench SQL fetch failed: {e}")
        return False


def _load_queries():
    with open(os.path.join(ENTRY, "queries.sql")) as fh:
        return [q.strip() for q in fh.read().split("\n") if q.strip() and not q.strip().startswith("--")]


# ClickBench `hits` tem 99.997.497 linhas (número publicado pelo próprio ClickBench). Usado só para
# derivar o passo da amostragem sistemática — nunca para afirmar que rodamos a escala completa.
HITS_TOTAL_ROWS = 99_997_497

# --- M164 harness guards (pure, env-injected → unit-testable with no DB and no real box; plan ADR-1) ------------------

def sample_is_fresh(rows_in_file: int, n_rows: int, tol: int = 1) -> bool:
    """B1 guard: True iff the cached sample's row count matches the requested size. Both samplers head-cap at `n`, and
    the dataset has only `HITS_TOTAL_ROWS` rows, so a request materializes exactly `min(n_rows, HITS_TOTAL_ROWS)` rows —
    the freshness target is CLAMPED to that, otherwise a legit `--n 100000000` (n > dataset ⇒ file has 99,997,497 rows)
    would be judged permanently stale and re-stream ~100 GB every invocation (M162 pain #2, cross-validation HIGH). A
    smaller count (the M162 false-100M: a 1M cache served as 100M) or a materially larger one is stale and MUST be
    re-materialized. `tol` is defensive slack (a trailing-newline miscount); the head cap already pins the exact count."""
    if rows_in_file <= 0 or n_rows <= 0:
        return False
    target = min(n_rows, HITS_TOTAL_ROWS)
    return target <= rows_in_file <= target + tol


def plan_shows_agg_pushdown(plan_text: str) -> bool:
    """True iff the plan used the AGG pushdown specifically. MUST key on `theodb_columnar_agg`, NOT a generic
    `Custom Scan`: the projection provider `theodb_columnar_project` defaults ON and renders a `Custom Scan` under
    EVERY columnar scan, so a broad `"Custom Scan" in plan` reports True even when the agg pushdown DECLINED — the very
    false-green B2 exists to catch (council-benchmark HIGH-2)."""
    return "theodb_columnar_agg" in plan_text


def classify_ab(agg_routed, result_ab_identical) -> str:
    """B2 guard: classify one query's A/B outcome by whether the AGG pushdown actually routed (`agg_routed`, from
    `plan_shows_agg_pushdown` — NOT the broad columnar-customscan boolean, which the always-on projection scan makes
    ~always True) —
    - 'diverged'         : results differ → a real correctness bug (routed or not);
    - 'routed_identical' : agg pushdown routed AND byte-identical → a MEANINGFUL pushdown-correctness pass;
    - 'declined_trivial' : byte-identical but the agg pushdown DECLINED (ran native over columnar storage) → proves the
                           storage round-trip, NOT the pushdown (the M162 SORTED/`diverged=0` false-green);
    - 'n/a'              : routing or identity unknown (query errored / not measured)."""
    if result_ab_identical is False:
        return "diverged"
    if agg_routed is None or result_ab_identical is None:
        return "n/a"
    return "routed_identical" if agg_routed else "declined_trivial"


def decide_ab_verdict(agg: bool, ab_routed_identical: int, ab_diverged: int) -> tuple:
    """B2 verdict: pick the A/B headline + the `no_pushdown_exercised` flag. A DIVERGENCE OUTRANKS a trivial run so a
    real pushdown correctness bug is never masked behind a "nothing was exercised" message (council-benchmark/cross-val
    MEDIUM). `no_pushdown_exercised` fires ONLY when `--agg` routed nothing AND there is no divergence to report."""
    no_pushdown_exercised = bool(agg) and ab_routed_identical == 0 and ab_diverged == 0
    if ab_diverged != 0:
        return "DIVERGENCE — pushdown bug", False
    if no_pushdown_exercised:
        return ("TRIVIAL — --agg set but 0 queries routed the agg pushdown (ON declined; A/B proves storage "
                "round-trip, not pushdown)"), True
    return "byte-identical (columnar==heap)", False


# Sizing estimates for the pre-flight (C). Published ClickBench `hits`: ~74.5 GiB uncompressed TSV ÷ 99,997,497 rows ≈
# **800 B/row** (GiB, not the 745 the decimal-GB gave — council-benchmark LOW). A load writes THREE things to disk, not
# just the sample file: the TSV sample on the cache FS, the `hits_heap` heap copy, and the columnar `hits`. Estimating
# only the sample (the prior bug) under-counts the real footprint the tables actually exhaust (council-benchmark MEDIUM),
# so the disk estimate sums all three. Heap tuple of 105 cols ≈ 1000 B/row w/ headers/alignment; columnar compresses ~8×
# (~150 B/row). Conservative on purpose: a false-BLOCK is safe (operator frees space), a false-PASS OOMs the box.
EST_SAMPLE_BYTES_PER_ROW = 800     # TSV sample on the cache filesystem
EST_HEAP_BYTES_PER_ROW = 1000      # hits_heap (105-col heap tuple, headers+alignment)
EST_COLUMNAR_BYTES_PER_ROW = 150   # hits (columnar, ~8× compressed)
EST_DISK_BYTES_PER_ROW = EST_SAMPLE_BYTES_PER_ROW + EST_HEAP_BYTES_PER_ROW + EST_COLUMNAR_BYTES_PER_ROW  # full footprint
EST_RAM_BYTES_PER_ROW = EST_HEAP_BYTES_PER_ROW + EST_COLUMNAR_BYTES_PER_ROW  # the in-DB working set queried under load
SAFE_DISK_FRACTION = 0.8      # BLOCK if the full on-disk footprint would exceed this fraction of free disk
RAM_HEADROOM_FRACTION = 0.7   # WARN (never block) if the in-DB working set exceeds this fraction of RAM


def preflight_sizing(n_rows: int, disk_free_bytes: int, ram_bytes: int) -> dict:
    """C guard: BLOCK a load whose full on-disk footprint (sample TSV + heap copy + columnar table) cannot fit a safe
    fraction of free disk (pure waste), but only WARN when the in-DB working set exceeds RAM headroom — larger-than-RAM
    is an INTENTIONAL TheoDB regime (plan ADR-2; the M162 100M run was deliberately larger-than-RAM). `n_rows` is
    CLAMPED to `HITS_TOTAL_ROWS` (the corpus can't yield more — council-benchmark HIGH-1) so a `--n 100M` request is
    sized against the real 99,997,497-row load, not an impossible 100M. Returns {block, warn, reasons, est_disk_bytes,
    est_ram_bytes}. Env (disk/RAM) is injected so the guard is deterministically unit-testable. NOTE (coarse net): it
    assumes the cache FS and PGDATA share the disk (the single-box benchmark case) and gives no credit for an
    already-materialized cache — both bias toward a (safe) false-BLOCK, never a (dangerous) false-PASS."""
    n = min(int(n_rows), HITS_TOTAL_ROWS)
    est_disk = n * EST_DISK_BYTES_PER_ROW
    est_ram = n * EST_RAM_BYTES_PER_ROW
    reasons: list[str] = []
    block = est_disk > SAFE_DISK_FRACTION * disk_free_bytes
    if block:
        reasons.append(f"disk: est full footprint {est_disk / 2**30:.1f} GiB (sample+heap+columnar) > "
                       f"{SAFE_DISK_FRACTION:.0%} of free {disk_free_bytes / 2**30:.1f} GiB — refusing (would not fit)")
    warn = est_ram > RAM_HEADROOM_FRACTION * ram_bytes
    if warn:
        reasons.append(f"ram: est in-DB working set {est_ram / 2**30:.1f} GiB > {RAM_HEADROOM_FRACTION:.0%} of RAM "
                       f"{ram_bytes / 2**30:.1f} GiB — larger-than-RAM run (intentional; WARN, not blocked)")
    return {"block": block, "warn": warn, "reasons": reasons,
            "est_disk_bytes": est_disk, "est_ram_bytes": est_ram}


def _ensure_sample(path: str, n_rows: int, strategy: str = "systematic") -> bool:
    """Stream hits.tsv.gz e materializa n_rows — nunca baixa os ~100 GB para disco.

    DUAS estratégias, com um viés real separando-as:

    - `head` (rápido): mantém as PRIMEIRAS n linhas. O `hits` está ordenado por EventTime, então isto é
      uma fatia temporal estreita, NÃO uma amostra. As cardinalidades de GROUP BY (UserID, WatchID…)
      ficam artificialmente baixas, o que **favorece** os nossos números — agregação sobre poucos grupos
      distintos é justamente o regime em que o pushdown vetorizado brilha. Serve para smoke-test, não
      para uma medição que se queira honesta.
    - `systematic` (default): mantém 1 linha a cada `k = HITS_TOTAL_ROWS / n_rows`, varrendo o arquivo
      inteiro. Cobre todo o intervalo temporal, então as cardinalidades se aproximam das reais. CUSTO
      HONESTO: precisa descomprimir o stream completo (~100 GB) mesmo para amostras pequenas — o `head`
      encerra cedo, este não. É o preço de não enviesar o resultado a nosso favor.
    """
    if os.path.isfile(path) and os.path.getsize(path) > 0:
        # M164 (B1): a non-empty cache is NOT proof it is the RIGHT size — the M162 false-100M was a 1M cache served
        # as 100M. Count its rows and re-materialize on a stale count (systematic off-by-one tolerated).
        try:
            wc = subprocess.run(["wc", "-l", path], capture_output=True, text=True, timeout=600, check=True)
            rows_in_file = int(wc.stdout.split()[0])
        except Exception:
            rows_in_file = -1
        if sample_is_fresh(rows_in_file, int(n_rows)):
            return True
        print(f"  cache {path} has {rows_in_file} rows, need {n_rows} — re-materializing (stale-count guard, M164)",
              flush=True)
    if strategy not in ("head", "systematic"):
        raise ValueError(f"estratégia de amostragem desconhecida: {strategy!r}")
    try:
        n = int(n_rows)
        if strategy == "head":
            print(f"  streaming {n} hits rows (HEAD — fatia temporal enviesada) → {path}", flush=True)
            inner = f"head -n {n}"
            timeout = 1800
        else:
            k = max(1, HITS_TOTAL_ROWS // n)
            print(f"  streaming {n} hits rows (SISTEMÁTICA 1-em-{k}, varre o arquivo inteiro) → {path}",
                  flush=True)
            # NR % k para espalhar ao longo do arquivo; head no fim é apenas a trava de contagem exata
            # (a divisão inteira pode render 1 linha a mais).
            inner = f"awk 'NR % {k} == 0' | head -n {n}"
            timeout = 21600  # varrer ~100 GB comprimidos leva horas em rede comum
        # `path` vem do CLI (--cache) → shlex.quote contra injeção de shell (CWE-78). `inner` só contém
        # inteiros (n, k) e literais fixos; `_UA`/`HITS_TSV_GZ` são constantes.
        cmd = f"curl -sL -A '{_UA}' '{HITS_TSV_GZ}' | zcat | {inner} > {shlex.quote(path)}"
        subprocess.run(["bash", "-c", cmd], check=True, timeout=timeout)
        return os.path.isfile(path) and os.path.getsize(path) > 0
    except Exception as e:
        print(f"  hits stream failed: {e}")
        return False


def _flush_caches():
    """ClickBench cold-run flush (best-effort; needs privilege). Falls back to a no-op if not permitted."""
    try:
        subprocess.run(["bash", "-c", "sync 2>/dev/null; echo 3 > /proc/sys/vm/drop_caches 2>/dev/null || true"],
                       timeout=30, check=False, stderr=subprocess.DEVNULL)
    except Exception:
        pass


def _run_once(cur, sql):
    t0 = time.perf_counter()
    cur.execute(sql)
    try:
        rows = cur.fetchall()
    except psycopg2.ProgrammingError:
        rows = None  # a statement with no result set
    return time.perf_counter() - t0, rows


def _canonical(rows):
    """Order-insensitive canonical form of a result set for the byte-identical A/B (ClickBench queries with an
    ORDER BY are deterministic; aggregates are single-row; sorting makes the compare order-independent)."""
    return sorted([tuple(str(c) for c in r) for r in (rows or [])])


def _bench_query(cur, conn, i, sql, assert_byte_identical) -> dict:
    """Mede UMA query do ClickBench: timing 3× (cold+hot), plano (CustomScan?) e o oráculo A/B
    byte-idêntico (columnar vs heap). Retorna o entry; os contadores são derivados dos entries em `run`."""
    entry = {"q": i, "sql": sql[:60]}
    # timing 3× (cold flush before run 1)
    try:
        triple = []
        for run_i in range(3):
            if run_i == 0:
                _flush_caches()
            dt, _rows = _run_once(cur, sql)
            triple.append(round(dt, 4))
        entry["timings"] = triple
        entry["cold"], entry["hot"] = triple[0], min(triple[1], triple[2])
    except Exception as e:
        conn.rollback() if not conn.autocommit else None
        entry["error"] = str(e).splitlines()[0][:120]
        return entry
    # plan: columnar CustomScan vs native
    try:
        cur.execute("EXPLAIN (FORMAT TEXT) " + sql)
        plan = "\n".join(r[0] for r in cur.fetchall())
        entry["columnar_customscan"] = "theodb_columnar_agg" in plan or "Custom Scan" in plan
        # M164 (B2/HIGH-2): a SEPARATE, agg-SPECIFIC routing signal. `columnar_customscan` above is broad and ~always
        # True (the always-on projection scan renders a `Custom Scan` under every columnar query), so it CANNOT tell an
        # agg pushdown from a declined agg over a projection scan. `classify_ab` MUST consume THIS one.
        entry["columnar_agg_routed"] = plan_shows_agg_pushdown(plan)
    except Exception:
        entry["columnar_customscan"] = None
        entry["columnar_agg_routed"] = None
    # byte-identical result A/B: columnar vs heap. Strip the trailing `LIMIT N` first: ClickBench's
    # `... ORDER BY count DESC LIMIT 10` has many tied counts on a subsample, so the LIMIT cut picks an
    # ARBITRARY-but-valid 10 among the ties — a legitimate scan-order difference, NOT a storage bug. Comparing
    # the FULL (unlimited) deterministic aggregation is the real columnar-storage correctness oracle.
    ab_sql = re.sub(r"\s+LIMIT\s+\d+\s*;?\s*$", "", sql.rstrip().rstrip(";"))
    try:
        cur.execute(ab_sql)
        rc = _canonical(cur.fetchall())
        cur.execute(ab_sql.replace("hits", "hits_heap"))
        rh = _canonical(cur.fetchall())
        r = assert_byte_identical({j: rc[j] for j in range(len(rc))}, {j: rh[j] for j in range(len(rh))}) \
            if len(rc) == len(rh) else {"identical": False, "diverged": abs(len(rc) - len(rh))}
        entry["result_ab_identical"] = r["identical"]
    except Exception as e:
        entry["result_ab_identical"] = None
        entry["result_ab_note"] = str(e).splitlines()[0][:80]
    return entry


def run(args) -> dict:
    base = {"dataset": "clickbench-hits", "n_rows": args.n, "sample_strategy": args.sample,
            "box": "self-hosted (NOT canonical c6a.4xlarge)",
            "protocol": "3 runs/query: cold=1st (cache-flushed), hot=min-of-2"}
    if not ensure_entry_sql():
        return {**base, "status": "UNBENCHMARKED", "reason": "ClickBench create/queries SQL unavailable", "queries": None}
    sample = os.path.join(args.cache, "hits_sample.tsv")
    os.makedirs(args.cache, exist_ok=True)
    # M164 (C): pre-flight sizing — refuse a load that cannot fit disk; warn (never block) on larger-than-RAM.
    disk_free = shutil.disk_usage(args.cache).free
    ram_bytes = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    sizing = preflight_sizing(args.n, disk_free, ram_bytes)
    if sizing["block"]:
        return {**base, "status": "UNBENCHMARKED", "reason": "pre-flight sizing refused: " + "; ".join(sizing["reasons"]),
                "sizing": sizing, "queries": None}
    for w in sizing["reasons"]:
        print(f"  WARN sizing: {w}", flush=True)
    if not _ensure_sample(sample, args.n, args.sample):
        return {**base, "status": "UNBENCHMARKED", "reason": "hits dataset unavailable", "queries": None}

    conn = _conn()
    conn.autocommit = True
    cur = conn.cursor()
    # Build the columnar `hits` (create.sql) + a heap copy `hits_heap` for the result A/B.
    with open(os.path.join(ENTRY, "create.sql")) as fh:
        create_sql = fh.read()
    cur.execute("DROP TABLE IF EXISTS hits CASCADE")
    cur.execute("DROP TABLE IF EXISTS hits_heap CASCADE")
    cur.execute(create_sql)
    cur.execute(create_sql.replace("USING theodb_columnar", "").replace("CREATE TABLE hits", "CREATE UNLOGGED TABLE hits").replace("hits", "hits_heap"))
    # theodb_columnar bulk-load path is INSERT-SELECT (as the columnar_*_ab.py benchmarks do), NOT direct COPY —
    # COPY FROM STDIN into the columnar TAM hangs on the 105-col wide row (recorded honestly; see the doc). So:
    # COPY the sample into the heap copy (fast), then INSERT INTO hits SELECT * FROM hits_heap (columnar writer).
    with open(sample) as fh:
        cur.copy_expert("COPY hits_heap FROM STDIN WITH (FORMAT text)", fh)
    cur.execute("INSERT INTO hits SELECT * FROM hits_heap")  # SQL estático (sem interpolação) — sem regra ruff a suprimir
    # enable_columnar_agg default OFF = run over columnar STORAGE via PG's native executor; --agg turns the
    # vectorized-aggregate CustomScan pushdown ON. M131 fixed #135, which previously made `--agg` unusable on the
    # real 105-col hits: EXPLAIN of a query with ORDER BY <aggregate> (Q16/Q33) recursed forever in ruleutils'
    # `resolve_special_varno` deparse. NOTE the corrected diagnosis — it was NOT a planner hang, NOT O(cols^2), and
    # NOT width/TEXT-related (the queries always EXECUTED fine); `statement_timeout` could not interrupt it because
    # it happened during plan PRINTING. Both modes keep the byte-identical A/B oracle
    # (`docs/benchmarks/m131-columnar-agg-accelerated.md`).
    cur.execute(f"SET theodb.enable_columnar_agg = {'on' if args.agg else 'off'}")
    cur.execute("SET max_parallel_workers_per_gather = 0")
    # Per-query ceiling: a query the columnar path cannot complete in time is recorded ERRORED (honest per-query
    # status, plan failure-scenario) rather than hanging the whole run — ClickBench itself has no such guard.
    cur.execute(f"SET statement_timeout = '{int(args.query_timeout_s) * 1000}'")

    queries = _load_queries()
    from theodb_bench.regression import assert_byte_identical  # reuse the M127 byte-identical comparator

    results = []
    for i, sql in enumerate(queries):
        entry = _bench_query(cur, conn, i, sql, assert_byte_identical)
        results.append(entry)
        print(f"  q{i:>2} cold={entry.get('cold','ERR')} hot={entry.get('hot','ERR')} "
              f"cs={entry.get('columnar_customscan')} ab={entry.get('result_ab_identical')}", flush=True)
    cur.close()
    conn.close()

    # Contadores derivados dos entries (uma passada — mais simples que contar dentro do loop).
    ok = [e for e in results if "timings" in e]
    errored = sum(1 for e in results if "error" in e)
    customscan = sum(1 for e in results if e.get("columnar_customscan"))
    ab_pass = sum(1 for e in results if e.get("result_ab_identical") is True)
    ab_diverged = sum(1 for e in results if e.get("result_ab_identical") is False)
    # M164 (B2): split the A/B pass by whether the ON arm actually routed. A `--agg` run whose queries all decline is a
    # false-green — the byte-identical verdict is trivial (native-over-columnar vs heap), proving nothing about the
    # pushdown. `ab_routed_identical` is the MEANINGFUL count; `no_pushdown_exercised` flags the trivial run loudly.
    # keyed on the agg-SPECIFIC signal (HIGH-2): a query with only the always-on projection scan is `declined_trivial`.
    ab_class = [classify_ab(e.get("columnar_agg_routed"), e.get("result_ab_identical")) for e in results]
    ab_routed_identical = sum(1 for c in ab_class if c == "routed_identical")
    ab_declined_trivial = sum(1 for c in ab_class if c == "declined_trivial")
    ab_verdict, no_pushdown_exercised = decide_ab_verdict(args.agg, ab_routed_identical, ab_diverged)
    geomean = None
    if ok:
        geomean = round(math.exp(sum(math.log(max(e["hot"], 1e-6)) for e in ok) / len(ok)), 5)
    return {
        **base, "status": "OK", "n_queries": len(queries), "queries_ok": len(ok), "queries_errored": errored,
        "columnar_customscan_count": customscan, "hot_geomean_s": geomean,
        "result_ab": {"pass": ab_pass, "diverged": ab_diverged,
                      "routed_identical": ab_routed_identical, "declined_trivial": ab_declined_trivial,
                      "no_pushdown_exercised": no_pushdown_exercised, "verdict": ab_verdict},
        "queries": results,
        "caveats": [
            "self-hosted box, NOT canonical AWS c6a.4xlarge — QPS/timings not leaderboard-comparable (ADR M128-2)",
            f"hits subsampled to {args.n} rows (full 99.9M is the operational follow-up); CC-BY-NC-SA, CI-only, never vendored",
            "byte-identical result A/B (columnar vs heap) is the TheoDB-owned correctness oracle ClickBench lacks",
        ],
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--n", type=int, default=1_000_000, help="hits subsample rows")
    ap.add_argument("--sample", choices=["systematic", "head"], default="systematic",
                    help="systematic (default): 1-em-k varrendo o arquivo inteiro — cobre todo o intervalo "
                         "temporal, cardinalidades realistas, custa o stream completo (~100 GB). "
                         "head: as primeiras n linhas — rapido, mas fatia temporal ENVIESADA que infla o "
                         "ganho do pushdown (poucos grupos distintos). Use head so para smoke-test.")
    ap.add_argument("--cache", default="benchmarks/.cache")
    ap.add_argument("--query-timeout-s", type=int, default=60, help="per-query ceiling; slow query -> ERRORED")
    ap.add_argument("--agg", action="store_true", help="enable the vectorized columnar-agg CustomScan pushdown (the M131 fix for #135 removed the EXPLAIN deparse hang; default OFF = storage path)")
    ap.add_argument("--out", default="docs/benchmarks/m128-clickbench-columnar.json")
    args = ap.parse_args()
    data = run(args)
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as fh:
        json.dump(data, fh, indent=2)
    # also drop the ClickBench-format results.json (raw timing triples) into the entry dir
    if data["status"] == "OK":
        with open(os.path.join(ENTRY, "template.json")) as fh:
            tpl = json.load(fh)
        tpl["result"] = [e.get("timings", []) for e in data["queries"]]
        with open(os.path.join(ENTRY, "results.json"), "w") as fh:
            json.dump(tpl, fh, indent=2)
    print(f"wrote {args.out}  status={data['status']}")
    if data["status"] == "OK":
        print(f"  queries ok={data['queries_ok']}/{data['n_queries']} errored={data['queries_errored']} "
              f"columnar_customscan={data['columnar_customscan_count']} hot_geomean={data['hot_geomean_s']}s")
        print(f"  result A/B: {data['result_ab']['verdict']} (pass={data['result_ab']['pass']}, diverged={data['result_ab']['diverged']})")
    # Fail-loud (rules/error-handling.md): o processo tem de sair != 0 quando o run não completou OU o
    # oráculo A/B byte-idêntico detectou divergência — senão um CI encadeado no exit status prosseguiria
    # sobre um bug de correção do pushdown.
    if data["status"] != "OK":
        return 2
    if data["result_ab"]["diverged"] != 0:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
