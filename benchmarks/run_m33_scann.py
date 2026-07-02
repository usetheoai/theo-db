#!/usr/bin/env python3
"""M33 operator artifact — head-to-head vs the SOTA vector target (AlloyDB/ScaNN).

AlloyDB is a GCP-MANAGED service (its `alloydb_scann` index is proprietary, no local/CI run), so per the
M33 DoD's sanctioned fallback we benchmark against **ScaNN OSS** (`pip install scann`, Apache-2.0, Google
Research) — the open-source implementation of the exact algorithm AlloyDB's vector index is built on
(anisotropic vector quantization + SOAR; Guo et al. ICML 2020, arXiv:1908.10396). See the M33 plan
(.claude/knowledge-base/plans/m33-scann-headtohead-plan.md) ADR-1.

Fairness (matched methodology, no cherry-pick):
  * SAME SIFT1M (1M×128, Euclidean), SAME 1000-query subsample (seed 42), SAME neighbors-GT, SAME k=10 as
    the M34 theodb/pgvector run — reused verbatim from docs/benchmarks/m34-ivfflat-reloption.json (ADR-2).
  * ScaNN `num_leaves=1000` matches theodb/pgvector `lists=1000`; the `leaves_to_search` sweep {1,10,50,100}
    matches the theodb/pgvector `probes` sweep {1,10,50,100} — a partition-fraction-matched comparison.
  * recall is computed IDENTICALLY to theodb: recompute the EXACT Euclidean distance of ScaNN's returned
    ids from the corpus vectors (never trust ScaNN's quantized AH distances), then feed the SAME
    `theodb_bench.recall.recall_at_k` (distance-thresholded, ANN-Benchmarks) the theodb side used.

Honest caveat (analysis-golden-rule — mandatory): ScaNN is a pure IN-MEMORY ANN library (no persistence,
no transactions, no SQL); theodb_ivfflat is a persistent transactional PostgreSQL index. Raw-search speed
is apples-to-apples on the *algorithm* axis, apples-to-oranges on the *database* axis. The verdict states
both. A measured GAP is a valid, DoD-sanctioned outcome (ADR-3).

Repro:
  # 1. cache SIFT1M once (if absent):
  curl -fsSL -o benchmarks/.datasets/sift-128-euclidean.hdf5 http://ann-benchmarks.com/sift-128-euclidean.hdf5
  # 2. install the dev-only baseline lib (needs AVX2):
  pip install scann
  # 3. run (no DB needed — ScaNN is in-memory; theodb/pgvector numbers come from the M34 artifact):
  python3 benchmarks/run_m33_scann.py --n-queries 1000 --runs 3
"""
from __future__ import annotations

import argparse
import json
import resource
import time
from pathlib import Path

import numpy as np

from theodb_bench.dataset import load_hdf5_full
from theodb_bench.metrics import latency_percentiles, qps_best_of_n
from theodb_bench.recall import recall_at_k

_REPO = Path(__file__).resolve().parents[1]
_DEFAULT_HDF5 = str(_REPO / "benchmarks" / ".datasets" / "sift-128-euclidean.hdf5")
_M34_ARTIFACT = _REPO / "docs" / "benchmarks" / "m34-ivfflat-reloption.json"

# num_leaves matches the M34 theodb/pgvector lists=1000; the first 4 leaves_to_search points {1,10,50,100}
# are partition-fraction-matched to the theodb/pgvector probes sweep. The extra {200,400} let ScaNN reach the
# same high-recall band (>=0.99) theodb reaches, so the "QPS at recall>=0.99" head-to-head is well-defined —
# ScaNN is SOTA and easily crosses 0.99; under-sweeping it (capping at leaves=100) would be an unfair handicap.
_NUM_LEAVES = 1000
_LEAVES_TO_SEARCH_SWEEP = (1, 10, 50, 100, 200, 400)
_TRAINING_SAMPLE = 250_000  # ScaNN's design samples for partition training (its recommended SIFT config)
_PRE_REORDER = 256          # rescore the top-256 AH candidates with exact distance (ScaNN high-recall config)


def _peak_rss_mb() -> float:
    # ru_maxrss is KiB on Linux; the process peak (includes the in-memory corpus — stated in the caveat).
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024.0


def _exact_l2(query: np.ndarray, vecs: np.ndarray) -> np.ndarray:
    """Exact Euclidean distances (query→each vec), float32-value contract like theodb_bench.recall."""
    diff = query.astype(np.float64) - vecs.astype(np.float64)
    d2 = np.sum(diff * diff, axis=1)
    np.maximum(d2, 0.0, out=d2)
    return np.sqrt(d2)


def measure_scann(corpus, queries, gt_dist, k, runs):
    """Build a ScaNN index on the full corpus, then sweep leaves_to_search measuring recall@k / QPS /
    p50-p95-p99 / build-time / peak-RSS. Returns (rows, build_ms, peak_rss_mb, scann_version)."""
    import scann

    rss_before = _peak_rss_mb()
    t0 = time.perf_counter()
    searcher = (
        scann.scann_ops_pybind.builder(corpus, k, "squared_l2")
        .tree(num_leaves=_NUM_LEAVES, num_leaves_to_search=_LEAVES_TO_SEARCH_SWEEP[-1],
              training_sample_size=min(_TRAINING_SAMPLE, corpus.shape[0]))
        .score_ah(2, anisotropic_quantization_threshold=0.2)
        .reorder(_PRE_REORDER)
        .build()
    )
    build_ms = (time.perf_counter() - t0) * 1000.0
    peak_rss = max(_peak_rss_mb(), rss_before)

    n_q = queries.shape[0]
    rows = []
    for leaves in _LEAVES_TO_SEARCH_SWEEP:
        # recall (correctness) — single pass; recompute EXACT distance of the returned ids (honest recall)
        run_distances = []
        for i in range(n_q):
            idx, _ah = searcher.search(queries[i], final_num_neighbors=k,
                                       pre_reorder_num_neighbors=_PRE_REORDER, leaves_to_search=leaves)
            run_distances.append(_exact_l2(queries[i], corpus[idx]))
        recall = recall_at_k(gt_dist, run_distances, k)

        # latency (best-of-N runs) — per-query single search to mirror theodb's per-query SQL path
        run_mean_latencies_s = []
        all_samples_ms = []
        for _ in range(runs):
            samples_ms = []
            for i in range(n_q):
                t = time.perf_counter()
                searcher.search(queries[i], final_num_neighbors=k,
                                pre_reorder_num_neighbors=_PRE_REORDER, leaves_to_search=leaves)
                samples_ms.append((time.perf_counter() - t) * 1000.0)
            run_mean_latencies_s.append(float(np.mean(samples_ms)) / 1000.0)
            all_samples_ms = samples_ms  # keep the last run's per-query samples for percentiles
        pct = latency_percentiles(all_samples_ms)
        qps = qps_best_of_n(run_mean_latencies_s)
        rows.append({
            "index": "scann", "params": f"leaves_to_search={leaves}",
            "recall_at_k": round(recall, 4), "qps": round(qps, 1),
            "p50": round(pct["p50"], 2), "p95": round(pct["p95"], 2), "p99": round(pct["p99"], 2),
            "mean_ms": round(pct["mean"], 2), "std_ms": round(pct["std"], 2),
        })
    return rows, round(build_ms, 0), round(peak_rss, 1), _scann_version()


def _scann_version() -> str:
    # scann does not expose a top-level __version__; read the installed dist metadata (honest repro record).
    try:
        from importlib.metadata import version
        return version("scann")
    except Exception:
        return "unknown"


def _load_m34_rows():
    """theodb_ivfflat + pgvector ivfflat frontier from the M34 SIFT1M artifact (ADR-2 — reuse, don't re-run)."""
    if not _M34_ARTIFACT.is_file():
        raise SystemExit(f"M34 artifact not found at {_M34_ARTIFACT} — needed for the theodb/pgvector columns")
    d = json.loads(_M34_ARTIFACT.read_text())
    rows = {"theodb_ivfflat": [], "pgvector_ivfflat": []}
    for r in d["results"]:
        key = "theodb_ivfflat" if r["index"] == "theodb_ivfflat" else "pgvector_ivfflat"
        rows[key].append(r)
    return rows, d


def _best_qps_at_recall(rows, min_recall):
    """Highest QPS among rows meeting the recall floor; None if none reach it."""
    ok = [r for r in rows if r["recall_at_k"] >= min_recall]
    return max(ok, key=lambda r: r["qps"]) if ok else None


def _verdict(theodb_qps, scann_qps, tol=0.10):
    """theodb's standing vs ScaNN on a higher-is-better metric (QPS). Within tol → PARITY."""
    if theodb_qps is None or scann_qps is None:
        return "INDETERMINATE"
    ratio = theodb_qps / scann_qps
    if ratio >= 1.0 + tol:
        return "SUPERIOR"
    if ratio >= 1.0 - tol:
        return "PARITY"
    return "GAP"


def consolidate(scann_rows, scann_build_ms, scann_rss_mb, scann_version, m34_rows, m34_meta, out_dir):
    recall_floor = 0.99
    scann_best = _best_qps_at_recall(scann_rows, recall_floor)
    theodb_best = _best_qps_at_recall(m34_rows["theodb_ivfflat"], recall_floor)
    pgv_best = _best_qps_at_recall(m34_rows["pgvector_ivfflat"], recall_floor)

    qps_verdict = _verdict(theodb_best["qps"] if theodb_best else None,
                           scann_best["qps"] if scann_best else None)
    # recall dimension: both reach the floor → PARITY on recall reachability
    recall_verdict = "PARITY" if (scann_best and theodb_best) else "INDETERMINATE"
    # latency dimension (p50, lower better) at the matched high-recall point
    lat_verdict = "INDETERMINATE"
    if scann_best and theodb_best:
        r = theodb_best["p50"] / scann_best["p50"]  # >1 means theodb slower
        lat_verdict = "SUPERIOR" if r <= 0.9 else "PARITY" if r <= 1.1 else "GAP"

    result = {
        "milestone": "M33",
        "dataset": "sift-128-euclidean (SIFT1M)",
        "n": m34_meta["n"], "dim": m34_meta["dim"], "k": m34_meta["k"],
        "n_queries": m34_meta["n_queries"], "runs": m34_meta["runs"], "seed": m34_meta.get("seed", 42),
        "host": m34_meta.get("host", "unknown"),
        "baseline_note": ("AlloyDB is GCP-managed (no local run); ScaNN OSS is the DoD-sanctioned proxy — "
                          "the algorithm behind AlloyDB's vector index (arXiv:1908.10396)."),
        "caveat_library_vs_database": (
            "ScaNN is a pure IN-MEMORY ANN library (no persistence/transactions/SQL); theodb_ivfflat is a "
            "persistent transactional PostgreSQL index. Raw-search numbers compare the ALGORITHM axis; they "
            "do NOT make ScaNN a database. ScaNN peak-RSS includes the full in-memory corpus; theodb index "
            "bytes are on-disk index structure — different measures."),
        "scann_version": scann_version,
        "scann_build_ms": scann_build_ms,
        "scann_peak_rss_mb": scann_rss_mb,
        "theodb_index_bytes": (theodb_best or {}).get("index_bytes"),
        "config": {"num_leaves": _NUM_LEAVES, "leaves_to_search_sweep": list(_LEAVES_TO_SEARCH_SWEEP),
                   "training_sample_size": _TRAINING_SAMPLE, "pre_reorder": _PRE_REORDER,
                   "note": "num_leaves matches theodb/pgvector lists=1000; leaves_to_search matches probes sweep"},
        "results": {
            "scann": scann_rows,
            "theodb_ivfflat": m34_rows["theodb_ivfflat"],
            "pgvector_ivfflat": m34_rows["pgvector_ivfflat"],
        },
        "matched_high_recall_point": {
            "recall_floor": recall_floor,
            "scann": scann_best, "theodb_ivfflat": theodb_best, "pgvector_ivfflat": pgv_best,
        },
        "verdict_per_dimension": {
            "recall_at_10": recall_verdict,
            "qps_at_recall_0.99": qps_verdict,
            "latency_p50_at_recall_0.99": lat_verdict,
            "memory": "INDETERMINATE (different measures — see caveat: ScaNN peak-RSS vs theodb index bytes)",
        },
        "overall_note": (
            "North Star (vector superiority vs AlloyDB): theodb_ivfflat vs the ScaNN algorithm on raw ANN "
            "search — see verdict_per_dimension. theodb's value is vector search INSIDE a transactional "
            "database; ScaNN's is a specialized in-memory library. A GAP on raw QPS is honest and expected "
            "(ScaNN uses learned anisotropic quantization + SIMD AH; theodb_ivfflat is full-precision IVFFlat)."),
    }
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "m33-scann-headtohead.json").write_text(json.dumps(result, indent=2) + "\n")
    (out_dir / "m33-scann-headtohead.md").write_text(_render_md(result))
    return result


def _render_md(r) -> str:
    L = []
    L.append("# M33 — head-to-head vs AlloyDB/ScaNN (North Star vector superiority)\n")
    L.append(f"**Dataset:** {r['dataset']} · n={r['n']:,} dim={r['dim']} k={r['k']} · "
             f"queries={r['n_queries']} (seed {r['seed']}) · runs={r['runs']} · host={r['host']}\n")
    L.append(f"**Baseline:** {r['baseline_note']}\n")
    L.append(f"> **CAVEAT (library vs database):** {r['caveat_library_vs_database']}\n")
    L.append(f"**ScaNN:** v{r['scann_version']} · build {r['scann_build_ms']:.0f} ms · "
             f"peak RSS {r['scann_peak_rss_mb']:.0f} MB (incl. in-memory corpus) · "
             f"config num_leaves={r['config']['num_leaves']} training_sample={r['config']['training_sample_size']} "
             f"pre_reorder={r['config']['pre_reorder']}\n")
    L.append("## Recall–QPS frontier (partition-fraction matched)\n")
    L.append("| system | params | recall@10 | QPS | p50 (ms) | p95 (ms) | p99 (ms) |")
    L.append("|---|---|---|---|---|---|---|")
    for sys_key in ("scann", "theodb_ivfflat", "pgvector_ivfflat"):
        for row in r["results"][sys_key]:
            L.append(f"| {sys_key} | {row['params']} | {row['recall_at_k']:.4f} | {row['qps']:.1f} | "
                     f"{row['p50']:.2f} | {row.get('p95','-')} | {row.get('p99','-')} |")
    mp = r["matched_high_recall_point"]
    L.append(f"\n## Verdict @ recall ≥ {mp['recall_floor']} (matched high-recall operating point)\n")
    L.append("| dimension | verdict | ScaNN | theodb_ivfflat | pgvector_ivfflat |")
    L.append("|---|---|---|---|---|")

    def cell(x, key):
        return f"{x[key]}" if x else "—"
    L.append(f"| best QPS | {r['verdict_per_dimension']['qps_at_recall_0.99']} | "
             f"{cell(mp['scann'],'qps')} | {cell(mp['theodb_ivfflat'],'qps')} | {cell(mp['pgvector_ivfflat'],'qps')} |")
    L.append(f"| p50 latency (ms) | {r['verdict_per_dimension']['latency_p50_at_recall_0.99']} | "
             f"{cell(mp['scann'],'p50')} | {cell(mp['theodb_ivfflat'],'p50')} | {cell(mp['pgvector_ivfflat'],'p50')} |")
    L.append(f"| recall@10 reachable | {r['verdict_per_dimension']['recall_at_10']} | "
             f"{cell(mp['scann'],'recall_at_k')} | {cell(mp['theodb_ivfflat'],'recall_at_k')} | {cell(mp['pgvector_ivfflat'],'recall_at_k')} |")
    L.append(f"| memory | {r['verdict_per_dimension']['memory']} | "
             f"{r['scann_peak_rss_mb']:.0f} MB RSS | {r.get('theodb_index_bytes','?')} B index | — |")
    L.append(f"\n**Overall:** {r['overall_note']}\n")
    L.append("## Reproduction\n")
    L.append("```\npip install scann   # dev-only, Apache-2.0, needs AVX2\n"
             "python3 benchmarks/run_m33_scann.py --n-queries 1000 --runs 3\n```\n")
    L.append("theodb/pgvector rows reused verbatim from `docs/benchmarks/m34-ivfflat-reloption.json` "
             "(same SIFT1M, hardware, neighbors-GT). ScaNN measured on the SAME 1000-query subsample (seed 42).\n")
    return "\n".join(L)


def main() -> int:
    ap = argparse.ArgumentParser(description="M33 ScaNN head-to-head (operator artifact)")
    ap.add_argument("--n-queries", type=int, default=1000)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--hdf5", default=_DEFAULT_HDF5)
    ap.add_argument("--out", default=str(_REPO / "docs" / "benchmarks"))
    args = ap.parse_args()

    if not Path(args.hdf5).is_file():
        raise SystemExit(f"SIFT1M not found at {args.hdf5} — see the repro header to download it")
    try:
        import scann  # noqa: F401
    except ImportError:
        raise SystemExit("scann not installed — `pip install scann` (dev-only, needs AVX2)")

    corpus, queries, gt_dist = load_hdf5_full(args.hdf5, args.n_queries, seed=42, k=10, metric="l2")
    scann_rows, build_ms, rss_mb, ver = measure_scann(corpus, queries, gt_dist, k=10, runs=args.runs)
    m34_rows, m34_meta = _load_m34_rows()
    result = consolidate(scann_rows, build_ms, rss_mb, ver, m34_rows, m34_meta, args.out)

    print(f"\n=== M33 ScaNN head-to-head (n={result['n']} k={result['k']} "
          f"queries={result['n_queries']} runs={result['runs']}) ===")
    for row in scann_rows:
        print(f"scann {row['params']:22} recall@10={row['recall_at_k']:.4f} qps={row['qps']:.1f} "
              f"p50={row['p50']:.2f} p95={row['p95']:.2f}")
    vp = result["verdict_per_dimension"]
    print(f"\nVerdict @ recall>=0.99:  QPS={vp['qps_at_recall_0.99']}  "
          f"p50={vp['latency_p50_at_recall_0.99']}  recall={vp['recall_at_10']}")
    print(f"artifact -> {args.out}/m33-scann-headtohead.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
