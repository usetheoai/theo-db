"""Benchmark orchestration: dataset -> ground-truth -> build index -> measure -> report.

Accepts an injected `db` (any object with the VectorDB interface) so it is testable without a
container (DIP, blueprint ADR D1). Emits a reproducible JSON + markdown report to docs/benchmarks/
— the artifact of the M2 measurement-first gate (ADR 0002).
"""
from __future__ import annotations

import datetime
import json
import platform
import subprocess
from pathlib import Path

from .dataset import load_hdf5_subsample, make_dataset
from .metrics import latency_percentiles, qps_best_of_n
from .recall import brute_force_ground_truth, recall_at_k


def _git_sha() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], stderr=subprocess.DEVNULL
        ).decode().strip()
    except Exception:  # noqa: BLE001 — sha is best-effort provenance
        return "unknown"


def run_benchmark(config: dict, db, out_dir) -> dict:
    """Run the benchmark for every index spec / param sweep and write the report. Returns the report dict."""
    hdf5 = config.get("hdf5_path")
    if hdf5:
        corpus, queries = load_hdf5_subsample(hdf5, config["n"], config["n_queries"], config["seed"])
    else:
        corpus, queries = make_dataset(config["n"], config["dim"], config["n_queries"], config["seed"])
    dim = corpus.shape[1]  # real datasets define their own dim; trust the data, not config
    _, gt_dist = brute_force_ground_truth(corpus, queries, config["k"], config["metric"])

    db.ensure_extension()
    db.create_table(config["table"], dim)
    db.load_vectors(config["table"], corpus)

    results = []
    for spec in config["index_specs"]:
        build_ms = db.build_index(spec["ddl"]) * 1000.0
        for params in spec.get("sweep", [{}]):
            for stmt in params.get("session", []):
                db.set_session(stmt)
            # guard once: confirm the planner actually uses the index
            db.assert_index_used(config["table"], queries[0], config["k"], config["metric"])

            # untimed warmup pass so the timed runs (and percentiles) describe a warm cache,
            # consistent with the best-of-N QPS (domain review M2).
            for q in queries:
                db.query_topk(config["table"], q, config["k"], config["metric"])

            run_means, all_latency_ms, last_run_dists = [], [], None
            for _ in range(config["runs"]):
                lat, run_dists = [], []
                for q in queries:
                    _ids, dists, t = db.query_topk(config["table"], q, config["k"], config["metric"])
                    lat.append(t)
                    run_dists.append(dists)
                run_means.append(sum(lat) / len(lat))
                all_latency_ms.extend(x * 1000.0 for x in lat)
                last_run_dists = run_dists

            recall = recall_at_k(gt_dist, last_run_dists, config["k"])
            pct = latency_percentiles(all_latency_ms)
            results.append(
                {
                    "index": spec["name"],
                    "params": params.get("label", ""),
                    "recall_at_k": recall,
                    "qps": qps_best_of_n(run_means),
                    "build_ms": build_ms,
                    "index_bytes": db.index_size_bytes(spec["index_name"]),
                    **pct,
                }
            )

    report = {
        "sha": _git_sha(),
        "date": datetime.date.today().isoformat(),
        "seed": config["seed"],
        "n": config["n"],
        "dim": dim,
        "n_queries": config["n_queries"],
        "k": config["k"],
        "metric": config["metric"],
        "runs": config["runs"],
        "dataset": config.get("dataset_label", f"synthetic-gaussian-{config['metric']}"),
        "host": platform.node(),
        "methodology": (
            "index-forced (SET enable_seqscan=off, assert_index_used); recall is "
            "distance-thresholded (ANN-Benchmarks) vs exact float32 brute-force ground-truth; "
            "QPS = 1/best-of-N mean per-query latency (warm)"
        ),
        "results": results,
    }
    _write_report(report, Path(out_dir))
    return report


def artifact_stem(report: dict) -> str:
    """Filename stem for a report: real datasets carry their label; synthetic keeps the legacy name."""
    label = report.get("dataset", "")
    if label and not label.startswith("synthetic-gaussian"):
        return f"{report['date']}-{label}"
    return f"{report['date']}-pgvector-{report['metric']}"


def _write_report(report: dict, out_dir: Path) -> tuple[Path, Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    stem = artifact_stem(report)
    json_path = out_dir / f"{stem}.json"
    md_path = out_dir / f"{stem}.md"
    json_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    md_path.write_text(_render_markdown(report), encoding="utf-8")
    return json_path, md_path


def _render_markdown(report: dict) -> str:
    lines = [
        f"# TheoDB vector benchmark — {report['date']}",
        "",
        f"- **commit:** `{report['sha']}` · **seed:** {report['seed']} · **dataset:** "
        f"{report.get('dataset', 'synthetic')} (n={report['n']} dim={report['dim']} "
        f"metric={report['metric']}) · **k:** {report['k']} · **runs (best-of-N):** {report['runs']}",
        "- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is exact brute-force.",
        "",
        "| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | build ms | index bytes |",
        "|---|---|---|---|---|---|---|---|---|",
    ]
    for r in report["results"]:
        lines.append(
            f"| {r['index']} | {r['params']} | {r['recall_at_k']:.4f} | {r['qps']:.1f} | "
            f"{r['p50']:.3f} | {r['p95']:.3f} | {r['p99']:.3f} | {r['build_ms']:.1f} | {r['index_bytes']} |"
        )
    return "\n".join(lines) + "\n"
