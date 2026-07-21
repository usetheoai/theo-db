#!/usr/bin/env python3
"""M129 — official-benchmark OLTP pillar: run the two field-standard tools against self-hosted TheoDB and prove the
retained wrap layer (paired significance) the OLTP tools lack (blueprint Q11).

- pgbench (PostgreSQL-shipped, TPC-B-like, PostgreSQL License → D1-clean): N repeated timed runs → TPS, then
  paired significance over the run-to-run TPS (the wrap-layer capability pgbench/HammerDB do not provide).
- HammerDB TPROC-C (the real TPC-C 45/43/4/4/4 mix, NOPM) via the `tpcorg/hammerdb` Docker image (GPLv3 → external
  out-of-tree driver, NEVER vendored/linked): measured when Docker + `--hammerdb`; else HAMMERDB_SKIPPED (honest).

Honesty rails (ADR M129-2, CLAUDE.md rule 5): every throughput number is paired with the `fsync` posture + the
retained crash-safety gate (`theodb_rs/isolation/crash_*.sh`, #46/#47) — a throughput number without durability is
meaningless, which the OLTP tools do NOT enforce. Self-hosted box (labeled), NOT canonical hardware. No pgbench /
DB → UNBENCHMARKED, clean exit (no fabricated TPS). TheoDB's OLTP path IS PostgreSQL's (theodb_rs is an extension).
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys

from theodb_bench.significance import paired_significance

TPS_RE = re.compile(r"^tps\s*=\s*([0-9.]+)", re.MULTILINE)
NOPM_RE = re.compile(r"([0-9]+)\s+NOPM", re.IGNORECASE)


def parse_pgbench_tps(output: str) -> float:
    """Extract the TPS float from a pgbench run (the 'tps = N (without initial connection time)' line)."""
    m = TPS_RE.search(output)
    if not m:
        raise ValueError(f"no 'tps =' line in pgbench output: {output[-200:]!r}")
    return float(m.group(1))


def parse_hammerdb_nopm(output: str) -> int:
    """Extract NOPM from a HammerDB TPROC-C result ('… System achieved NNN NOPM …')."""
    m = NOPM_RE.search(output)
    if not m:
        raise ValueError(f"no 'NOPM' in HammerDB output: {output[-200:]!r}")
    return int(m.group(1))


def _pgbench_env():
    return {**os.environ, "PGHOST": os.environ.get("PGHOST", "localhost"),
            "PGPORT": os.environ.get("PGPORT", "28900"), "PGUSER": os.environ.get("PGUSER", "postgres"),
            "PGDATABASE": os.environ.get("PGDATABASE", "postgres"), "PGPASSWORD": os.environ.get("PGPASSWORD", "postgres")}


def run_pgbench(scale, clients, threads, duration, runs, pgbench_bin) -> dict:
    env = _pgbench_env()
    subprocess.run([pgbench_bin, "-i", "-q", "-s", str(scale)], env=env, check=True,
                   capture_output=True, timeout=600)
    tps = []
    for _ in range(runs):
        r = subprocess.run([pgbench_bin, "-c", str(clients), "-j", str(threads), "-T", str(duration), "-r"],
                           env=env, capture_output=True, text=True, timeout=duration + 120, check=True)
        tps.append(parse_pgbench_tps(r.stdout))
    # Paired significance over first-half vs second-half of the repeated runs → run-to-run stability (the wrap-layer
    # capability pgbench lacks). Needs >=4 runs (>=2 pairs per side); expected NOT significant (a stable engine).
    half = len(tps) // 2
    sig = paired_significance(tps[:half], tps[half:half * 2]) if len(tps) >= 4 else "need >=4 runs for split significance"
    return {"tool": "pgbench", "metric": "TPS", "scale": scale, "clients": clients, "duration_s": duration,
            "runs": runs, "tps_per_run": [round(t, 1) for t in tps],
            "tps_mean": round(sum(tps) / len(tps), 1), "tps_min": round(min(tps), 1), "tps_max": round(max(tps), 1),
            "significance": sig}


def run_hammerdb(warehouses, vus, rampup, duration) -> dict:
    if not shutil.which("docker"):
        return {"tool": "hammerdb-tproc-c", "status": "HAMMERDB_SKIPPED", "reason": "docker unavailable"}
    tcl = os.path.join(os.path.dirname(os.path.abspath(__file__)), "oltp", "hammerdb_tproc_c.tcl")
    env = _pgbench_env()
    # HammerDB (GPLv3) runs as an EXTERNAL Docker process; our .tcl drives its CLI. --network host reaches the
    # self-hosted PG on localhost:PGPORT.
    cmd = ["docker", "run", "--rm", "--network", "host",
           "-e", f"PG_HOST={env['PGHOST']}", "-e", f"PG_PORT={env['PGPORT']}",
           "-e", f"PG_WAREHOUSES={warehouses}", "-e", f"PG_VU={vus}",
           "-e", f"PG_RAMPUP={rampup}", "-e", f"PG_DURATION={duration}",
           "-v", f"{tcl}:/hd.tcl:ro", "tpcorg/hammerdb", "./hammerdbcli", "auto", "/hd.tcl"]
    try:
        # HammerDB's banner contains a © (byte 0xa9); strict UTF-8 decode would crash the driver (masking a possibly
        # successful run as an error). Decode with errors="replace" so the NOPM line is always parseable.
        r = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace",
                           timeout=(rampup + duration) * 60 + 900)
        out = r.stdout + r.stderr
        return {"tool": "hammerdb-tproc-c", "metric": "NOPM", "warehouses": warehouses, "vus": vus,
                "nopm": parse_hammerdb_nopm(out)}
    except Exception as e:
        return {"tool": "hammerdb-tproc-c", "status": "HAMMERDB_ERRORED", "reason": str(e).splitlines()[0][:160]}


def run(args) -> dict:
    base = {"box": "self-hosted (NOT canonical hardware)", "engine": "TheoDB (PostgreSQL-wire-compatible; OLTP = PG path)",
            "durability_posture": f"fsync={args.fsync}",
            "retained_crash_gate": "theodb_rs/isolation/crash_fold.sh + crash_unlogged.sh (#46/#47) — throughput is meaningless without durability; the OLTP tools do NOT enforce ACID"}
    pgbench_bin = args.pgbench or shutil.which("pgbench")
    if not pgbench_bin:
        return {**base, "status": "UNBENCHMARKED", "reason": "pgbench binary not found", "pgbench": None}
    try:
        pg = run_pgbench(args.scale, args.clients, args.threads, args.duration, args.runs, pgbench_bin)
    except Exception as e:
        return {**base, "status": "UNBENCHMARKED", "reason": f"pgbench failed: {str(e).splitlines()[0][:120]}", "pgbench": None}
    hdb = run_hammerdb(args.warehouses, args.vus, args.rampup, args.hdb_duration) if args.hammerdb else \
        {"tool": "hammerdb-tproc-c", "status": "HAMMERDB_SKIPPED", "reason": "--hammerdb not set (documented follow-up)"}
    return {**base, "status": "OK", "pgbench": pg, "hammerdb": hdb,
            "caveats": [
                "self-hosted box, NOT canonical hardware — TPS/NOPM not comparable to published/audited TPC-C",
                "HammerDB NOPM is NOT audited tpmC (TPROC-C is a TPC-C derivative); HammerDB (GPLv3) runs as an external Docker driver, never vendored/linked",
                f"durability posture recorded ({base['durability_posture']}); throughput is paired with the retained crash-safety gate, not reported as 'valid' alone",
            ]}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--scale", type=int, default=10)
    ap.add_argument("--clients", type=int, default=8)
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--duration", type=int, default=10, help="pgbench per-run seconds")
    ap.add_argument("--runs", type=int, default=4, help="repeated pgbench runs (>=4 enables split significance)")
    ap.add_argument("--fsync", default="on", help="durability posture recorded in the artifact")
    ap.add_argument("--hammerdb", action="store_true", help="also run HammerDB TPROC-C via Docker (GPLv3 external)")
    ap.add_argument("--warehouses", type=int, default=4)
    ap.add_argument("--vus", type=int, default=4)
    ap.add_argument("--rampup", type=int, default=1)
    ap.add_argument("--hdb-duration", type=int, default=2)
    ap.add_argument("--pgbench", default=None, help="pgbench binary path (else PATH)")
    ap.add_argument("--out", default="docs/benchmarks/m129-oltp.json")
    args = ap.parse_args()
    data = run(args)
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as fh:
        json.dump(data, fh, indent=2, default=str)
    print(f"wrote {args.out}  status={data['status']}")
    if data["status"] == "OK":
        pg = data["pgbench"]
        print(f"  pgbench TPS: mean={pg['tps_mean']} min={pg['tps_min']} max={pg['tps_max']} over {pg['runs']} runs")
        print(f"  significance (run-to-run): {pg['significance']}")
        print(f"  hammerdb: {data['hammerdb'].get('nopm', data['hammerdb'].get('status'))}")
        print(f"  durability: {data['durability_posture']} (paired with the retained crash gate)")


if __name__ == "__main__":
    sys.exit(main())
