#!/usr/bin/env python3
"""M46 — hardened high-recall re-measurement of theodb_hnsw (measurement-first, ADR-2 of the M46 plan).

Reuses the M45 rigorous Pareto driver (`run_m45_pareto.run`) verbatim (Rule 9 — no harness rebuild) and
layers the *hardening* the M45 ef>=200 point lacked:

- **median + trimmed mean** over `runs >= 5` per operating point — the noisy dev-box ef>=200 point (where
  M45 saw ef=400 measure faster than ef=200, physically impossible → contended box) is summarized by its
  median and its outlier-trimmed mean, not just mean±std, so a single stall does not distort the signal.
- **recall-neutral gate**: baseline-vs-post recall MUST be identical per ef (the change only re-sizes/reuses
  allocations — if recall moves 1 point it is a BUG, not an optimization) and the QPS-std band SHOULD shrink.
- **pages_read** (optional): when the postgres process runs with `THEODB_SCAN_PROFILE=1`, `traverse` logs
  `pages_read` per scan; the driver parses it from `docker logs` when a container name is given. Identical
  pages_read before/after is the deterministic proof the visit order is unchanged. Absent → recorded `None`.

The claim-bearing arithmetic (`harden_frontier`, `recall_neutral_compare`) is pure stdlib — unit-testable
without a container (mirrors `m45_pareto`). Only `run_hardened` touches the DB, by delegating to M45.

Usage (per image, back-to-back same session — EC-4):
  python3 benchmarks/run_m46_highrecall.py --hdf5 .datasets/sift-128-euclidean.hdf5 \
      --nq 500 --runs 5 --port 5474 --tag baseline --out benchmarks/artifacts/m46-highrecall-qps-baseline.json
  # (rebuild the image with the M46 change, restart container, then)
  python3 benchmarks/run_m46_highrecall.py ... --tag post --out benchmarks/artifacts/m46-highrecall-qps-post.json
  python3 benchmarks/run_m46_highrecall.py --compare baseline.json post.json --write-doc
"""
from __future__ import annotations

import argparse
import json
import statistics
import subprocess
from pathlib import Path

_REPO = Path(__file__).resolve().parent.parent

# theodb_hnsw is the index under measurement; pgvector stays as the parity reference from the M45 frontier.
_THEODB = "theodb_hnsw"


# ----------------------------- pure post-processing (no DB, unit-testable) -----------------------------

def _trimmed_mean(xs):
    """Mean after dropping ONE min and ONE max (the single-stall / single-turbo outliers). For < 3 samples
    there is nothing to trim → plain mean. Never raises on an empty list (returns 0.0 — a handled edge)."""
    if not xs:
        return 0.0
    if len(xs) < 3:
        return round(statistics.mean(xs), 1)
    trimmed = sorted(xs)[1:-1]
    return round(statistics.mean(trimmed), 1)


def harden_frontier(points):
    """Add `qps_median` + `qps_trimmed_mean` to each operating point, derived from its per-run `qps_runs`.

    Leaves the M45 mean±std intact (additive — the artifact keeps full transparency). A point without
    `qps_runs` (e.g. a legacy frontier) falls back to its `qps_mean` for both, flagged `hardened=False`.
    """
    out = []
    for p in points:
        runs = p.get("qps_runs")
        q = dict(p)
        if runs:
            q["qps_median"] = round(statistics.median(runs), 1)
            q["qps_trimmed_mean"] = _trimmed_mean(runs)
            q["hardened"] = True
        else:
            q["qps_median"] = p.get("qps_mean")
            q["qps_trimmed_mean"] = p.get("qps_mean")
            q["hardened"] = False
        q.setdefault("pages_read", None)  # filled by attach_pages_read when available; else honestly None
        out.append(q)
    return out


def _variance_pct(point):
    """QPS relative variance (std/mean) as a percent, or None when mean is 0 (guarded)."""
    mean = point.get("qps_mean") or 0.0
    if mean == 0:
        return None
    return round(100.0 * (point.get("qps_std") or 0.0) / mean, 1)


def recall_neutral_compare(baseline_points, post_points):
    """Per-ef baseline-vs-post comparison for the recall-neutral verdict.

    Returns a list of dicts keyed by ef, each carrying: recall_baseline, recall_post, recall_identical
    (the load-bearing invariant — recall MUST NOT move), pages_read_identical (deterministic visit order,
    or None when pages_read was not captured on either side), qps_baseline_median, qps_post_median,
    qps_delta_pct (post vs baseline), variance_baseline_pct, variance_post_pct, variance_reduced.

    A missing ef on either side is a handled negative case (recorded present=False, not a crash).
    """
    by_ef_base = {p["ef"]: p for p in baseline_points}
    by_ef_post = {p["ef"]: p for p in post_points}
    rows = []
    for ef in sorted(set(by_ef_base) | set(by_ef_post)):
        b, a = by_ef_base.get(ef), by_ef_post.get(ef)
        if b is None or a is None:
            rows.append({"ef": ef, "present": False,
                         "reason": "missing on baseline" if b is None else "missing on post"})
            continue
        pr_b, pr_a = b.get("pages_read"), a.get("pages_read")
        pages_identical = (pr_b == pr_a) if (pr_b is not None and pr_a is not None) else None
        qb = b.get("qps_median", b.get("qps_mean")) or 0.0
        qa = a.get("qps_median", a.get("qps_mean")) or 0.0
        var_b, var_a = _variance_pct(b), _variance_pct(a)
        rows.append({
            "ef": ef, "present": True,
            "recall_baseline": b.get("recall"), "recall_post": a.get("recall"),
            "recall_identical": b.get("recall") == a.get("recall"),
            "pages_read_baseline": pr_b, "pages_read_post": pr_a, "pages_read_identical": pages_identical,
            "qps_baseline_median": round(qb, 1), "qps_post_median": round(qa, 1),
            "qps_delta_pct": round(100.0 * (qa - qb) / qb, 1) if qb else None,
            "variance_baseline_pct": var_b, "variance_post_pct": var_a,
            "variance_reduced": (var_a is not None and var_b is not None and var_a < var_b),
        })
    return rows


def recall_neutral_verdict(rows, high_recall_ef_floor=200, variance_target_pct=15.0):
    """Honest M46 verdict from the per-ef comparison (recall-neutral is the HARD gate).

    - If ANY present ef has recall_identical=False → `RECALL_REGRESSION` (a BUG — the change is not neutral).
    - Else if pages_read captured and any differ → `VISIT_ORDER_CHANGED` (also a bug signal).
    - Else classify the throughput/variance outcome at the high-recall band (ef >= floor):
        `THROUGHPUT_WIN`   — median QPS improved at every high-recall ef.
        `VARIANCE_WIN`     — QPS not clearly up, but high-recall variance dropped below `variance_target_pct`
                             (the plan's honest-negative alt: ~44% → <15%).
        `NEUTRAL`          — recall-neutral proven, no measurable throughput or variance improvement.
    Returns {verdict, reason, high_recall_efs}. Never raises; empty input → NEUTRAL/no-data.
    """
    present = [r for r in rows if r.get("present")]
    if not present:
        return {"verdict": "NEUTRAL", "reason": "no comparable ef points", "high_recall_efs": []}
    if any(not r["recall_identical"] for r in present):
        bad = [r["ef"] for r in present if not r["recall_identical"]]
        return {"verdict": "RECALL_REGRESSION", "reason": f"recall moved at ef={bad} — NOT recall-neutral (BUG)",
                "high_recall_efs": bad}
    checked = [r for r in present if r.get("pages_read_identical") is not None]
    if checked and any(not r["pages_read_identical"] for r in checked):
        bad = [r["ef"] for r in checked if not r["pages_read_identical"]]
        return {"verdict": "VISIT_ORDER_CHANGED", "reason": f"pages_read differ at ef={bad} — visit order moved",
                "high_recall_efs": bad}
    hi = [r for r in present if r["ef"] >= high_recall_ef_floor]
    if not hi:
        return {"verdict": "NEUTRAL", "reason": "recall-neutral proven; no high-recall ef in grid",
                "high_recall_efs": []}
    if all((r["qps_delta_pct"] or 0) > 0 for r in hi):
        return {"verdict": "THROUGHPUT_WIN",
                "reason": "recall-neutral AND median QPS up at every high-recall ef",
                "high_recall_efs": [r["ef"] for r in hi]}
    if all((r["variance_post_pct"] is not None and r["variance_post_pct"] < variance_target_pct) for r in hi):
        return {"verdict": "VARIANCE_WIN",
                "reason": f"recall-neutral AND high-recall variance below {variance_target_pct}% target",
                "high_recall_efs": [r["ef"] for r in hi]}
    return {"verdict": "NEUTRAL", "reason": "recall-neutral proven; no throughput or variance win",
            "high_recall_efs": [r["ef"] for r in hi]}


# ----------------------------- pages_read capture (best-effort, optional) -----------------------------

def parse_pages_read(log_text):
    """Parse the LAST `pages_read=N ef=M` per ef from a server-log blob (THEODB_SCAN_PROFILE=1 output).

    Returns {ef: pages_read}. Deterministic per (graph, ef, query-set); the LAST line per ef is taken so
    warmup passes do not matter. No match → empty dict (honest — the caller records None)."""
    out = {}
    for line in log_text.splitlines():
        if "theodb hnsw scan profile: pages_read=" not in line:
            continue
        try:
            frag = line.split("pages_read=", 1)[1]
            pr = int(frag.split()[0])
            ef = int(frag.split("ef=", 1)[1].split()[0])
            out[ef] = pr  # last wins
        except (ValueError, IndexError):
            continue  # malformed line → skip (never crash the driver on a log format drift)
    return out


def attach_pages_read(points, container):
    """Fill each point's `pages_read` from `docker logs {container}` (THEODB_SCAN_PROFILE=1). Best-effort:
    if docker is unavailable or the container did not log, leaves pages_read=None (honest)."""
    if not container:
        return points
    try:
        logs = subprocess.run(["docker", "logs", "--tail", "100000", container],
                              capture_output=True, text=True, timeout=30)
        by_ef = parse_pages_read((logs.stdout or "") + (logs.stderr or ""))
    except (subprocess.SubprocessError, OSError):
        by_ef = {}
    for p in points:
        p["pages_read"] = by_ef.get(p["ef"])
    return points


# ----------------------------- DB-touching (delegates to the M45 harness) -----------------------------

def run_hardened(port, hdf5, runs=5, ef_grid=(100, 200, 300, 400), container=None, **kw):
    """Run the M45 rigorous Pareto (>=5 runs, high-recall ef grid), then harden the theodb frontier with
    median + trimmed-mean + (optional) pages_read. Returns the full M45 report with an added
    `theodb_hardened` frontier."""
    from run_m45_pareto import run as m45_run  # local import → pure-logic tests need no DB deps
    res = m45_run(port=port, hdf5=hdf5, runs=runs, ef_grid=ef_grid, **kw)
    hardened = harden_frontier(res["frontier"][_THEODB])
    attach_pages_read(hardened, container)
    res["theodb_hardened"] = hardened
    res["hardening"] = {"runs": runs, "ef_grid": list(ef_grid), "container": container}
    return res


def _render_compare_md(rows, verdict):
    """Render the baseline-vs-post recall-neutral verdict table (the T3.1 artifact)."""
    lines = [
        "# M46 — theodb_hnsw high-recall hot-path hygiene: baseline vs post (recall-neutral)",
        "",
        f"**Verdict: {verdict['verdict']}** — {verdict['reason']}",
        "",
        "> Baseline and post measured back-to-back in the same session on the same box (EC-4) — dev-box noise",
        "> affects both equally. Recall-neutral is the HARD gate: if recall moves 1 point it is a BUG.",
        "",
        "| ef | recall (base→post) | recall-neutral | pages_read | QPS median base→post | Δ% | var% base→post |",
        "|---:|:---|:---:|:---:|:---|---:|:---|",
    ]
    for r in rows:
        if not r.get("present"):
            lines.append(f"| {r['ef']} | — | — | — | {r.get('reason','')} | — | — |")
            continue
        if r["pages_read_identical"] is None:
            pr = "n/a"
        elif r["pages_read_identical"]:
            pr = "identical"
        else:
            pr = "DIFFER"
        lines.append(
            f"| {r['ef']} | {r['recall_baseline']}→{r['recall_post']} | "
            f"{'✓' if r['recall_identical'] else '✗ BUG'} | {pr} | "
            f"{r['qps_baseline_median']}→{r['qps_post_median']} | {r['qps_delta_pct']} | "
            f"{r['variance_baseline_pct']}%→{r['variance_post_pct']}% |")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--hdf5", default=None)
    ap.add_argument("--n", type=int, default=50000)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--nq", type=int, default=500)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--seed", type=int, default=2026)
    ap.add_argument("--ef-grid", default="100,200,300,400")
    ap.add_argument("--port", type=int, default=5474)
    ap.add_argument("--container", default=None, help="container name for pages_read capture (THEODB_SCAN_PROFILE=1)")
    ap.add_argument("--tag", default="run", help="baseline | post")
    ap.add_argument("--out", default=None, help="write the hardened JSON here")
    ap.add_argument("--no-full-train", action="store_true")
    ap.add_argument("--compare", nargs=2, metavar=("BASELINE", "POST"), default=None)
    ap.add_argument("--write-doc", action="store_true")
    args = ap.parse_args()

    if args.compare:
        base = json.loads(Path(args.compare[0]).read_text())
        post = json.loads(Path(args.compare[1]).read_text())
        b_pts = base.get("theodb_hardened") or harden_frontier(base["frontier"][_THEODB])
        a_pts = post.get("theodb_hardened") or harden_frontier(post["frontier"][_THEODB])
        rows = recall_neutral_compare(b_pts, a_pts)
        verdict = recall_neutral_verdict(rows)
        print(f"\n=== M46 recall-neutral verdict: {verdict['verdict']} — {verdict['reason']} ===")
        for r in rows:
            if r.get("present"):
                print(f"  ef={r['ef']:>4} recall {r['recall_baseline']}→{r['recall_post']} "
                      f"neutral={r['recall_identical']} QPSmed {r['qps_baseline_median']}→{r['qps_post_median']} "
                      f"Δ={r['qps_delta_pct']}% var {r['variance_baseline_pct']}%→{r['variance_post_pct']}%")
        if args.write_doc:
            out = _REPO / "docs" / "benchmarks"
            out.mkdir(parents=True, exist_ok=True)
            (out / "m46-highrecall-qps.json").write_text(
                json.dumps({"rows": rows, "verdict": verdict}, indent=2))
            (out / "m46-highrecall-qps.md").write_text(_render_compare_md(rows, verdict))
            print(f"\nwrote {out / 'm46-highrecall-qps.json'} + .md")
        return 0

    ef_grid = tuple(int(x) for x in args.ef_grid.split(","))
    res = run_hardened(args.port, args.hdf5, runs=args.runs, ef_grid=ef_grid, container=args.container,
                       n=args.n, dim=args.dim, nq=args.nq, seed=args.seed, full_train=not args.no_full_train)
    print(f"\n=== M46 hardened high-recall ({args.tag}, runs={args.runs}) ===")
    for p in res["theodb_hardened"]:
        print(f"  ef={p['ef']:>4} recall={p['recall']:.4f} qps_mean={p['qps_mean']}±{p['qps_std']} "
              f"median={p['qps_median']} trimmed={p['qps_trimmed_mean']} pages_read={p['pages_read']}")
    if args.out:
        Path(args.out).write_text(json.dumps(res, indent=2))
        print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
