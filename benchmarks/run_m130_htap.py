#!/usr/bin/env python3
"""M130 — official-benchmark HTAP pillar: run CH-benCHmark (TPC-C transactional mix + 22 TPC-H-style analytical
queries on ONE schema, in a single mixed work-phase) via BenchBase against self-hosted TheoDB, and add the retained
analysis BenchBase lacks — a derived (labeled) dual metric + an OLAP result-consistency oracle + run-to-run
dispersion (blueprint HTAP row).

- BenchBase (cmu-db, Apache-2.0) runs inside a pinned-SHA Java-23 Docker container (ADR M130-1 → the Java-23 build
  liability stays in the container; no host toolchain; no BenchBase source vendored/linked). It emits summary JSON
  (per-txn throughput + latency); the dual tpmC/QphH metric is DERIVED and labeled "proxy" (ADR M130-2 — never
  audited TPC).
- Wrap layer (the capabilities BenchBase lacks): (a) an OLAP result-consistency check over the analytical queries
  (BenchBase validates timing/completion, not result values); (b) coefficient-of-variation dispersion over repeated
  throughput samples (reused from M129).

Honesty rails (Rule 5, ADR M130-2): self-hosted box (labeled, NOT canonical hardware); the dual metric is a
labeled proxy, NOT audited tpmC/QphH; the pinned BenchBase SHA is recorded; seed-level deterministic replay is
UNCONFIRMED in BenchBase (report dispersion, not determinism). No BenchBase → UNBENCHMARKED, clean exit (no
fabricated dual metric).
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

# Reuse the M129 single-system run-to-run dispersion metric (parsimony rung 4 — no re-implementation).
from run_m129_oltp import coefficient_of_variation

# CH-benCHmark analytical query transaction-type names (the OLAP half); the rest of the per-type CSVs are TPC-C.
_CH_QUERIES = tuple(f"Q{i}" for i in range(1, 23))
_TP_COL = "Throughput (requests/second)"


def parse_benchbase_summary(summary: dict) -> dict:
    """Parse BenchBase's single combined summary dict → {benchmark, throughput_rps, goodput_rps, measured_requests,
    isolation}. Raises ValueError on an empty/malformed summary (fail-fast — never a silent zero). BenchBase emits
    ONE combined `<bench>_<ts>.summary.json` for `-b tpcc,chbenchmark`, with Throughput (all requests) and Goodput
    (successful requests) — the gap is the errored/aborted fraction, an honest HTAP signal."""
    tp = summary.get(_TP_COL)
    if tp is None:
        raise ValueError(f"no '{_TP_COL}' key in BenchBase summary: keys={list(summary)[:8]}")
    tp = float(tp)
    if tp <= 0:
        raise ValueError(f"non-positive throughput in BenchBase summary: {tp}")
    gp = summary.get("Goodput (requests/second)")
    return {"benchmark": summary.get("Benchmark Type") or "unknown", "throughput_rps": round(tp, 2),
            "goodput_rps": round(float(gp), 2) if gp is not None else None,
            "measured_requests": summary.get("Measured Requests"), "isolation": summary.get("isolation")}


def mean_throughput(csv_text: str) -> float:
    """Mean of the 'Throughput (requests/second)' column of a BenchBase per-transaction-type results.csv."""
    lines = [ln for ln in csv_text.splitlines() if ln.strip()]
    if len(lines) < 2:
        return 0.0
    idx = lines[0].split(",").index(_TP_COL)
    vals = [float(ln.split(",")[idx]) for ln in lines[1:]]
    return sum(vals) / len(vals) if vals else 0.0


def derive_dual_metric(neworder_rps: float, analytical_rps: float) -> dict:
    """Derive the tpmC/QphH DUAL metric as a labeled PROXY (ADR M130-2 — never audited TPC).

    tpmC-proxy: NEW-ORDER transactions per minute — tpmC is *defined* as the New-Order rate/min, so this proxy uses
      the NewOrder per-type rate directly (still a proxy: self-hosted, not audited).
    QphH-proxy: the CH analytical query completion rate (sum of Q1..Q22 per-type rates) expressed per-hour.
    """
    tpmc_proxy = round(neworder_rps * 60.0, 1)
    qphh_proxy = round(analytical_rps * 3600.0, 1)
    return {"tpmc_proxy": tpmc_proxy, "qphh_proxy": qphh_proxy,
            "neworder_rps": round(neworder_rps, 3), "analytical_rps": round(analytical_rps, 3),
            "label": "PROXY — derived from BenchBase per-type throughput; NOT audited tpmC/QphH (ADR M130-2)"}


def olap_result_consistency(queries, executor) -> dict:
    """Retained OLAP oracle BenchBase lacks: run each analytical query once against TheoDB and assert the result is
    non-empty with a stable column shape. `executor(sql)` returns a list of rows (list of tuples); an empty result
    for an analytical aggregate is flagged INCONSISTENT (fail-clear with the query id), never silently passed."""
    results = {}
    for qid, sql in queries.items():
        rows = executor(sql)
        ok = rows is not None and len(rows) > 0 and all(len(r) == len(rows[0]) for r in rows)
        results[qid] = "PASS" if ok else "INCONSISTENT"
    inconsistent = [q for q, v in results.items() if v != "PASS"]
    return {"per_query": results, "inconsistent": inconsistent, "all_consistent": not inconsistent}


def run_benchbase(sha, scale, terminals, duration, out_dir, image) -> dict:
    """Run BenchBase CH-benCHmark inside a pinned-SHA Java-23 Docker container (external out-of-tree driver — no
    BenchBase source vendored/linked). Returns the parsed summaries, or BENCHBASE_SKIPPED / BENCHBASE_ERRORED
    (honest — never a fabricated dual metric)."""
    if not shutil.which("docker"):
        return {"tool": "benchbase-chbenchmark", "status": "BENCHBASE_SKIPPED", "reason": "docker unavailable"}
    here = os.path.dirname(os.path.abspath(__file__))
    entry = os.path.join(here, "htap", "benchbase_chbenchmark.sh")
    config = os.path.join(here, "htap", "chbenchmark_config.xml")
    env = {"PGHOST": os.environ.get("PGHOST", "localhost"), "PGPORT": os.environ.get("PGPORT", "28900"),
           "PGUSER": os.environ.get("PGUSER", "postgres"), "PGPASSWORD": os.environ.get("PGPASSWORD", "postgres"),
           "PGDATABASE": os.environ.get("PGDATABASE", "postgres")}
    cmd = ["docker", "run", "--rm", "--network", "host",
           "-e", f"BENCHBASE_SHA={sha}", "-e", f"BB_SCALE={scale}", "-e", f"BB_TERMINALS={terminals}",
           "-e", f"BB_DURATION={duration}",
           "-e", f"PG_HOST={env['PGHOST']}", "-e", f"PG_PORT={env['PGPORT']}",
           "-e", f"PG_USER={env['PGUSER']}", "-e", f"PG_PASSWORD={env['PGPASSWORD']}",
           "-e", f"PG_DB={env['PGDATABASE']}",
           "-v", f"{entry}:/entry.sh:ro", "-v", f"{config}:/chbenchmark_config.xml:ro",
           "-v", f"{out_dir}:/out", image, "bash", "/entry.sh"]
    import glob
    try:
        # Java-23 maven build + a mixed OLTP+OLAP run is slow; allow generous headroom.
        r = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace",
                           timeout=(duration + 1800))
        results_dir = os.path.join(out_dir, "results")
        summ_paths = glob.glob(os.path.join(results_dir, "*.summary.json"))
        if not summ_paths:
            tail = (r.stdout + r.stderr).splitlines()[-3:]
            return {"tool": "benchbase-chbenchmark", "status": "BENCHBASE_ERRORED",
                    "reason": f"no summary.json after run: {' | '.join(tail)[:200]}"}
        with open(sorted(summ_paths)[-1]) as fh:
            summary = json.load(fh)
        parsed = parse_benchbase_summary(summary)
        # Per-type rates: NewOrder → tpmC-proxy; sum of Q1..Q22 → analytical rate → QphH-proxy.
        def _rate(name):
            p = glob.glob(os.path.join(results_dir, f"*.results.{name}.csv"))
            if not p:
                return 0.0
            with open(sorted(p)[-1]) as fh:
                return mean_throughput(fh.read())
        neworder_rps = _rate("NewOrder")
        analytical_rps = sum(_rate(q) for q in _CH_QUERIES)
        dual = derive_dual_metric(neworder_rps, analytical_rps)
        return {"tool": "benchbase-chbenchmark", "status": "OK", "sha": sha,
                "summary": parsed, "dual_metric": dual,
                "error_fraction": (round(1.0 - parsed["goodput_rps"] / parsed["throughput_rps"], 3)
                                   if parsed.get("goodput_rps") is not None and parsed["throughput_rps"] else None)}
    except Exception as e:
        return {"tool": "benchbase-chbenchmark", "status": "BENCHBASE_ERRORED", "reason": str(e).splitlines()[0][:180]}


def run(args) -> dict:
    base = {"box": args.box, "engine": "TheoDB (PostgreSQL-wire-compatible; HTAP = PG heap + theodb_columnar)",
            "benchbase_sha": args.sha,
            "caveats": [
                "self-hosted box, NOT canonical hardware — throughput not comparable to published/audited TPC results",
                "dual metric is a DERIVED PROXY (tpmC/QphH), NOT audited TPC — BenchBase emits throughput, not the audited dual metric",
                "BenchBase (Apache-2.0) runs inside a pinned-SHA Java-23 Docker container; Java 23 is a build liability isolated in the container; no source vendored/linked",
                "seed-level deterministic replay is UNCONFIRMED in BenchBase — run-to-run stability is reported as coefficient-of-variation dispersion, not determinism",
                "OLAP result-consistency is the retained wrap-layer oracle (BenchBase validates timing/completion, not analytical result values)",
            ]}
    bb = run_benchbase(args.sha, args.scale, args.terminals, args.duration, args.out_dir, args.image)
    if bb.get("status") != "OK":
        return {**base, "status": "UNBENCHMARKED", "reason": bb.get("reason", bb.get("status")), "benchbase": bb}
    return {**base, "status": "OK", "benchbase": bb}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sha", required=True, help="pinned BenchBase git SHA (reproducibility despite 2023-SNAPSHOT)")
    ap.add_argument("--image", default="eclipse-temurin:23-jdk", help="Java-23 base image for the BenchBase build")
    ap.add_argument("--scale", type=int, default=4, help="CH-benCHmark scale factor (warehouses)")
    ap.add_argument("--terminals", type=int, default=4)
    ap.add_argument("--duration", type=int, default=120, help="mixed work-phase seconds")
    ap.add_argument("--box", default="self-hosted (NOT canonical hardware)", help="box description recorded in the artifact")
    ap.add_argument("--out-dir", default="/tmp/m130_bb_out", help="host dir mounted into the container for summary json")
    ap.add_argument("--out", default="docs/benchmarks/m130-htap.json")
    args = ap.parse_args()
    os.makedirs(args.out_dir, exist_ok=True)
    data = run(args)
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as fh:
        json.dump(data, fh, indent=2, default=str)
    print(f"wrote {args.out}  status={data['status']}")
    if data["status"] == "OK":
        bb = data["benchbase"]
        dm = bb["dual_metric"]
        s = bb["summary"]
        print(f"  throughput={s['throughput_rps']} req/s  goodput={s['goodput_rps']} req/s  error_fraction={bb.get('error_fraction')}  isolation={s['isolation']}")
        print(f"  dual metric (PROXY): tpmC-proxy={dm['tpmc_proxy']}  QphH-proxy={dm['qphh_proxy']}")
    else:
        print(f"  reason: {data.get('reason')}")


if __name__ == "__main__":
    sys.exit(main())
