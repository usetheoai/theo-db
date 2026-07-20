#!/usr/bin/env python3
"""M127 — official-benchmark VECTOR pilot: drive TheoDB's ann-benchmarks BaseANN adapter to a MEASURED
recall@10 × QPS Pareto on real GloVe, and prove the retained adopt-and-wrap layer (byte-identical regression +
paired significance) that ann-benchmarks itself lacks (blueprint Q11).

Protocol (ann-benchmarks-faithful): build the index ONCE (adapter.fit), sweep `ef_search`, measure recall@10 vs
exact ground-truth × single-thread QPS per point (the Pareto frontier). Then the wrap layer: re-query the SAME
index and assert BYTE-IDENTICAL rankings (determinism/regression), and run paired significance on the two runs'
per-query recall (demonstrates the wired significance test — expected non-significant = deterministic).

Honesty rails (ADR M127-2, CLAUDE.md rule 5): GloVe is D1-safe (PDDL); this runs on a SELF-HOSTED box (labeled),
NOT the canonical AWS c6a.4xlarge — the public ann-benchmarks leaderboard PR is a tracked follow-up. No dataset →
status UNBENCHMARKED, clean exit (no fabricated numbers). Any ScaNN/AlloyDB magnitude must cite our own m73.
"""
import argparse
import json
import os
import sys
import time
import urllib.request

import numpy as np

from theodb_bench.ann_adapter import TheoDBAnn
from theodb_bench.dataset import load_hdf5_subsample
from theodb_bench.db import VectorDB, DBUnavailableError
from theodb_bench.recall import brute_force_ground_truth
from theodb_bench.regression import assert_byte_identical
from theodb_bench.significance import paired_significance

GLOVE_URL = "https://ann-benchmarks.com/glove-100-angular.hdf5"  # PDDL (D1-safe), real angular distribution
_UA = "Mozilla/5.0 (X11; Linux x86_64)"  # ann-benchmarks.com blocks the default urllib UA (403)


def _dsn() -> str:
    return (f"host={os.environ.get('PGHOST','localhost')} port={os.environ.get('PGPORT','28900')} "
            f"dbname={os.environ.get('PGDATABASE','postgres')} user={os.environ.get('PGUSER','postgres')} "
            f"password={os.environ.get('PGPASSWORD','postgres')}")


def _ensure_glove(path: str) -> bool:
    if os.path.isfile(path):
        return True
    try:
        print(f"  downloading GloVe → {path} (PDDL) …", flush=True)
        req = urllib.request.Request(GLOVE_URL, headers={"User-Agent": _UA})  # noqa: S310 — fixed host
        with urllib.request.urlopen(req, timeout=120) as r, open(path, "wb") as fh:  # noqa: S310
            while chunk := r.read(1 << 20):
                fh.write(chunk)
        return os.path.isfile(path)
    except Exception as e:  # network/5xx → UNBENCHMARKED, never fabricate
        print(f"  GloVe download failed: {e}")
        return False


def _query_pass(adapter, queries, k):
    """One single-thread query pass through the BaseANN adapter contract: returns (ids_by_q: dict, qps)."""
    ids_by_q = {}
    start = time.perf_counter()
    for qi, v in enumerate(queries):
        ids_by_q[qi] = adapter.query(v, k)  # the ann-benchmarks BaseANN contract (ids only)
    elapsed = time.perf_counter() - start
    qps = len(queries) / elapsed if elapsed > 0 else 0.0
    return ids_by_q, qps


def _recall_per_query(ids_by_q, gt_idx, k):
    """id-overlap recall@k per query vs exact brute-force top-k ids (load_vectors ids == corpus positions)."""
    out = []
    for qi in range(len(gt_idx)):
        truth = set(int(x) for x in gt_idx[qi][:k])
        got = set(int(x) for x in ids_by_q[qi][:k])
        out.append(len(truth & got) / k)
    return out


def run(args) -> dict:
    base = {"dataset": "glove-100-angular", "n_corpus": args.n, "n_queries": args.n_queries,
            "k": args.k, "ef_sweep": args.ef, "metric": "angular/cosine", "box": "self-hosted (NOT canonical c6a.4xlarge)",
            "seed": args.seed}
    if not _ensure_glove(args.hdf5):
        return {**base, "status": "UNBENCHMARKED", "reason": "GloVe dataset absent/undownloadable",
                "pareto": None, "wrap": None}

    # 1. Real GloVe subsample + exact brute-force GT ids (subsample invalidates the file's precomputed neighbors).
    corpus, queries = load_hdf5_subsample(args.hdf5, args.n, args.n_queries, args.seed)
    gt_idx, _gt_dist = brute_force_ground_truth(corpus, queries, args.k, metric="cosine")

    # 2. Build the index ONCE via the BaseANN adapter.
    db = VectorDB(_dsn()).connect()
    db.ensure_extension()
    adapter = TheoDBAnn(db, metric="angular", table="m127_glove")
    t0 = time.perf_counter()
    adapter.fit(corpus)
    build_s = round(time.perf_counter() - t0, 2)

    # 3. Sweep ef_search → recall@10 × QPS Pareto (single-thread, via the adapter contract).
    pareto = []
    for ef in args.ef:
        adapter.set_query_arguments(ef)
        ids_by_q, qps = _query_pass(adapter, queries, args.k)
        recall = float(np.mean(_recall_per_query(ids_by_q, gt_idx, args.k)))
        pareto.append({"ef_search": ef, "recall@10": round(recall, 4), "qps": round(qps, 1)})
        print(f"  ef={ef:>4}  recall@10={recall:.4f}  qps={qps:.1f}")

    # Sanity gate (drawback #2): recall must rise toward 1.0 at the top ef.
    recalls = [p["recall@10"] for p in pareto]
    monotone_ok = all(recalls[i] <= recalls[i + 1] + 0.02 for i in range(len(recalls) - 1))
    top_recall_ok = recalls[-1] >= 0.90

    # 4. Wrap layer at the top ef: byte-identical A/B (re-query SAME index) + significance (2 runs' per-query recall).
    adapter.set_query_arguments(args.ef[-1])
    ids_a, _ = _query_pass(adapter, queries, args.k)
    ids_b, _ = _query_pass(adapter, queries, args.k)
    ab = assert_byte_identical(ids_a, ids_b)  # SAME index re-query MUST be byte-identical
    per_q_recall_a = _recall_per_query(ids_a, gt_idx, args.k)
    per_q_recall_b = _recall_per_query(ids_b, gt_idx, args.k)
    sig = paired_significance(per_q_recall_a, per_q_recall_b)  # expected: not significant (deterministic)
    db.close()

    return {
        **base, "status": "OK", "build_seconds": build_s, "pareto": pareto,
        "sanity": {"recall_monotone_in_ef": monotone_ok, "top_recall_ge_0.90": top_recall_ok,
                   "top_recall": recalls[-1]},
        "wrap": {
            "byte_identical_ab": ab,  # {identical, n, first_divergence, diverged}
            "significance_run1_vs_run2": {"mean_diff": sig["mean_diff"], "p_permutation": sig["p_permutation"],
                                          "verdict": "deterministic (not significant)" if sig["p_permutation"] > 0.05 else "UNEXPECTED difference"},
        },
        "caveats": [
            "self-hosted box, NOT the canonical AWS c6a.4xlarge — public ann-benchmarks leaderboard PR is a follow-up (ADR M127-2)",
            "GloVe subsampled to n_corpus with exact brute-force GT; the full-corpus canonical run is the operational follow-up",
            "any ScaNN/AlloyDB QPS-gap magnitude cites docs/benchmarks/m73-headtohead-verdict.md, not this run",
        ],
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--hdf5", default="benchmarks/.cache/glove-100-angular.hdf5")
    ap.add_argument("--n", type=int, default=200_000, help="corpus subsample size")
    ap.add_argument("--n-queries", type=int, default=1_000)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--ef", type=int, nargs="+", default=[10, 40, 100, 200, 400])
    ap.add_argument("--seed", type=int, default=20260720)
    ap.add_argument("--out", default="docs/benchmarks/m127-ann-benchmarks-vector.json")
    args = ap.parse_args()

    try:
        data = run(args)
    except DBUnavailableError as e:
        print(f"DB unavailable: {e}", file=sys.stderr)
        sys.exit(2)

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as fh:
        json.dump(data, fh, indent=2)
    print(f"wrote {args.out}  status={data['status']}")
    if data["status"] == "OK":
        w = data["wrap"]
        print(f"  byte-identical A/B: {w['byte_identical_ab']['identical']} "
              f"(n={w['byte_identical_ab']['n']}, diverged={w['byte_identical_ab']['diverged']})")
        print(f"  significance run1-vs-run2: {w['significance_run1_vs_run2']['verdict']}")
        print(f"  sanity: top recall={data['sanity']['top_recall']} (>=0.90: {data['sanity']['top_recall_ge_0.90']})")


if __name__ == "__main__":
    sys.exit(main())
