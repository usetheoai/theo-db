"""Benchmark orchestration: dataset -> ground-truth -> build index -> measure -> report.

Accepts an injected `db` (any object with the VectorDB interface) so it is testable without a
container (DIP, blueprint ADR D1). Emits a reproducible JSON + markdown report to wiki/benchmarks/
— the artifact of the M2 measurement-first gate (ADR 0002).
"""
from __future__ import annotations

import datetime
import json
import platform
import subprocess
from pathlib import Path

from .dataset import load_hdf5_full, load_hdf5_subsample, make_dataset
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
    if config.get("full_train"):
        # M32 >=1M scale path: full train + exact GT from the HDF5 `neighbors` (no 10^10 brute force).
        if not hdf5:
            raise ValueError("full_train requires hdf5_path")
        corpus, queries, gt_dist = load_hdf5_full(
            hdf5, config["n_queries"], config["seed"], config["k"], config["metric"]
        )
        gt_source = "exact GT recomputed from the ANN-Benchmarks `neighbors` ids in the float32 contract (neighbors-GT)"
    elif hdf5:
        corpus, queries = load_hdf5_subsample(hdf5, config["n"], config["n_queries"], config["seed"])
        _, gt_dist = brute_force_ground_truth(corpus, queries, config["k"], config["metric"])
        gt_source = "exact float32 brute-force ground-truth"
    else:
        corpus, queries = make_dataset(config["n"], config["dim"], config["n_queries"], config["seed"])
        _, gt_dist = brute_force_ground_truth(corpus, queries, config["k"], config["metric"])
        gt_source = "exact float32 brute-force ground-truth"
    dim = corpus.shape[1]  # real datasets define their own dim; trust the data, not config
    n_actual = corpus.shape[0]  # trust the data for N too (full_train ignores config["n"])

    db.ensure_extension()
    db.create_table(config["table"], dim)
    db.load_vectors(config["table"], corpus)

    results = []
    for spec in config["index_specs"]:
        build_ms = db.build_index(spec["ddl"]) * 1000.0
        # Isolate this spec's measurement: drop every OTHER spec's index so the planner cannot cross-use a
        # different index for this spec's queries. Critical when two specs are the same AM family on the same
        # column (e.g. theodb_ivfflat vs pgvector ivfflat) — otherwise the planner picks one arbitrarily and the
        # other spec's sweep flattens onto it (a wrong, un-isolated measurement). Specs are measured once in order,
        # so dropping already-measured / not-yet-built indexes here is safe.
        for other in config["index_specs"]:
            if other["index_name"] != spec["index_name"]:
                db.set_session(f'DROP INDEX IF EXISTS {other["index_name"]}')
        # A spec MAY cap its query count (`query_cap`): an O(N)-per-query index (e.g. theodb_hnsw's whole-blob
        # scan, ~4 s/query at 1M) would make the full query set take hours. Capping keeps the run tractable while
        # still yielding a valid recall/percentile sample; the cap is recorded in the params label for honesty.
        cap = spec.get("query_cap")
        if cap is not None and cap < 1:
            raise ValueError(f"query_cap must be >= 1, got {cap} (spec {spec['name']})")
        q_set = queries if cap is None else queries[:cap]
        gt_set = gt_dist if cap is None else gt_dist[:cap]
        for params in spec.get("sweep", [{}]):
            for stmt in params.get("session", []):
                db.set_session(stmt)
            # guard once: confirm the planner actually uses the index
            db.assert_index_used(config["table"], q_set[0], config["k"], config["metric"])

            # untimed warmup pass so the timed runs (and percentiles) describe a warm cache,
            # consistent with the best-of-N QPS (domain review M2).
            for q in q_set:
                db.query_topk(config["table"], q, config["k"], config["metric"])

            run_means, all_latency_ms, last_run_dists = [], [], None
            for _ in range(config["runs"]):
                lat, run_dists = [], []
                for q in q_set:
                    _ids, dists, t = db.query_topk(config["table"], q, config["k"], config["metric"])
                    lat.append(t)
                    run_dists.append(dists)
                run_means.append(sum(lat) / len(lat))
                all_latency_ms.extend(x * 1000.0 for x in lat)
                last_run_dists = run_dists

            recall = recall_at_k(gt_set, last_run_dists, config["k"])
            pct = latency_percentiles(all_latency_ms)
            label = params.get("label", "")
            if cap is not None:
                label = f"{label} [q={len(q_set)}]".strip()
            results.append(
                {
                    "index": spec["name"],
                    "params": label,
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
        "n": n_actual,
        "dim": dim,
        "n_queries": config["n_queries"],
        "k": config["k"],
        "metric": config["metric"],
        "runs": config["runs"],
        "dataset": config.get("dataset_label", f"synthetic-gaussian-{config['metric']}"),
        "host": platform.node(),
        "gt_source": gt_source,
        "methodology": (
            "index-forced (SET enable_seqscan=off, assert_index_used); recall is "
            f"distance-thresholded (ANN-Benchmarks) vs {gt_source}; "
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
        f"- recall@k is distance-thresholded (ANN-Benchmarks semantics); ground-truth is "
        f"{report.get('gt_source', 'exact float32 brute-force ground-truth')}.",
        "- `mean`/`std` are per-query latency dispersion within the timed sample (ms), not run-to-run variance; "
        "QPS is best-of-N over the runs.",
        "",
        "| index | params | recall@k | QPS | p50 ms | p95 ms | p99 ms | mean ms | std ms | build ms | index bytes |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in report["results"]:
        lines.append(
            f"| {r['index']} | {r['params']} | {r['recall_at_k']:.4f} | {r['qps']:.1f} | "
            f"{r['p50']:.3f} | {r['p95']:.3f} | {r['p99']:.3f} | {r.get('mean', 0.0):.3f} | "
            f"{r.get('std', 0.0):.3f} | {r['build_ms']:.1f} | {r['index_bytes']} |"
        )
    return "\n".join(lines) + "\n"
