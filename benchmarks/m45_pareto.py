"""M45 — pure post-processing for the rigorous mean±std Pareto claim (theodb_hnsw vs pgvector hnsw).

No I/O, no DB, no numpy — stdlib only, so the claim-bearing arithmetic (matched-recall interpolation +
effect>variance verdict) is unit-testable without a container. Consumed by run_m45_pareto.py.

A "frontier" is a list of operating points, each a dict with at least: `recall` (0..1), `qps_mean`,
`qps_std`. The comparison is the ANN-Benchmarks recall×QPS curve, not a single point.
"""
from __future__ import annotations

# A superiority/inferiority claim needs the margin to clear this relative band AROUND parity (1.0) — a
# 5% ratio guard so tiny ratios never read as a "win" (companion to the effect>variance std gate below).
_MARGIN_TOL = 0.05


def interpolate_qps_at_recall(points, target_recall):
    """Linear-interpolate QPS at ``target_recall`` on a recall×QPS frontier.

    Returns the interpolated QPS, or ``None`` when ``target_recall`` is outside the frontier's measured
    recall range (NO extrapolation — an out-of-range target is a handled negative case, never a
    fabricated claim). A single-point frontier covers only its exact recall.
    """
    pts = sorted(points, key=lambda p: p["recall"])
    if not pts:
        return None
    if target_recall < pts[0]["recall"] or target_recall > pts[-1]["recall"]:
        return None
    for a, b in zip(pts, pts[1:]):
        if a["recall"] <= target_recall <= b["recall"]:
            span = b["recall"] - a["recall"]
            if span == 0:  # flat segment (two points same recall) → no div-by-zero
                return a["qps_mean"]
            frac = (target_recall - a["recall"]) / span
            return a["qps_mean"] + (b["qps_mean"] - a["qps_mean"]) * frac
    # single-point frontier where target == that point's recall (range check already passed)
    return pts[0]["qps_mean"]


def _std_at(points, recall):
    """Variance band proxy at an interpolated recall: the qps_std of the nearest measured point."""
    nearest = min(points, key=lambda p: abs(p["recall"] - recall))
    return float(nearest.get("qps_std", 0.0))


def pareto_margin_verdict(theodb_pts, pgvector_pts, margin_tol=_MARGIN_TOL):
    """Compare two frontiers at shared recall levels; return matched-recall margins + an honest verdict.

    Verdict is gated on effect > variance (PRD D3, anti-sunk-cost): ``SUPERIOR`` only when theodb QPS
    exceeds pgvector's at EVERY shared recall level by more than ``margin_tol`` AND the absolute QPS gap
    exceeds the combined std at that level; ``INFERIOR`` is the symmetric case; otherwise ``PARITY``.
    No recall overlap → ``PARITY`` (reason). Empty input → ``PARITY``. Never raises, never fabricates.
    """
    if not theodb_pts or not pgvector_pts:
        return {"shared_levels": [], "margins": [], "verdict": "PARITY", "reason": "empty frontier"}

    t_lo, t_hi = min(p["recall"] for p in theodb_pts), max(p["recall"] for p in theodb_pts)
    p_lo, p_hi = min(p["recall"] for p in pgvector_pts), max(p["recall"] for p in pgvector_pts)
    lo, hi = max(t_lo, p_lo), min(t_hi, p_hi)
    if lo > hi:
        return {"shared_levels": [], "margins": [], "verdict": "PARITY", "reason": "no recall overlap"}

    # Data-driven shared grid: every measured recall (from either frontier) inside the overlap band.
    levels = sorted(
        {p["recall"] for p in theodb_pts if lo <= p["recall"] <= hi}
        | {p["recall"] for p in pgvector_pts if lo <= p["recall"] <= hi}
    )

    margins, n_superior, n_inferior = [], 0, 0
    for r in levels:
        qt = interpolate_qps_at_recall(theodb_pts, r)
        qp = interpolate_qps_at_recall(pgvector_pts, r)
        if qt is None or qp is None or qp == 0:
            continue
        margin = qt / qp
        gap = abs(qt - qp)
        effect = gap > (_std_at(theodb_pts, r) + _std_at(pgvector_pts, r))
        margins.append({
            "recall": round(r, 4),
            "margin": round(margin, 3),
            "qps_theodb": round(qt, 1),
            "qps_pgvector": round(qp, 1),
            "effect_gt_variance": effect,
        })
        if margin > 1 + margin_tol and effect:
            n_superior += 1
        elif margin < 1 - margin_tol and effect:
            n_inferior += 1

    if not margins:
        return {"shared_levels": levels, "margins": [], "verdict": "PARITY",
                "reason": "no interpolable shared level"}

    total = len(margins)
    if n_superior == total and n_inferior == 0:
        verdict = "SUPERIOR"
    elif n_inferior == total and n_superior == 0:
        verdict = "INFERIOR"
    else:
        verdict = "PARITY"
    return {"shared_levels": levels, "margins": margins, "verdict": verdict,
            "reason": "effect>variance gate over shared recall levels"}
