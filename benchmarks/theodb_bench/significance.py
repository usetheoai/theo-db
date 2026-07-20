"""M123 — paired significance for two IR systems on per-query metric arrays (e.g. nDCG@10).

Headline p = paired permutation / randomization test (Smucker, Allan & Carterette, CIKM 2007 — the IR-recommended
test; Wilcoxon/sign rejected because they discard per-query magnitude and drop ties). 95% CI on the mean
difference = paired bootstrap (percentile). Paired t-test reported as an agreeing cross-check (Urbano SIGIR 2013:
bootstrap/t also perform well → report all three).

The permutation + bootstrap are done in numpy (they are trivial, well-understood resampling — the blueprint's
sanctioned KISS path; a hard scipy pin is avoided because scipy's binary compat vs numpy 2.x is fragile). The
t-test p-value uses `scipy.stats.t` when scipy imports cleanly, else a normal approximation (valid for large n),
recorded in `p_ttest_method`. Pure computation over arrays — no DB, no network — so it is unit-testable offline.
Deterministic via a fixed seed so p and CI reproduce exactly.
"""

from __future__ import annotations

import math

import numpy as np


def paired_significance(a, b, *, seed: int = 20260720, n_resamples: int = 100_000) -> dict:
    """Paired significance of system `a` vs system `b` over per-query scores (same queries, same order).

    Returns the effect size (`mean_diff` = mean per-query a−b), a paired-bootstrap 95% CI on it, the paired
    permutation p-value (two-sided), a paired t-test p-value (cross-check), the win/loss/tie counts, and Cohen's
    dz. `a`/`b` MUST be equal-length arrays with n >= 2 (typed ValueError otherwise).
    """
    a = np.asarray(a, dtype=float)
    b = np.asarray(b, dtype=float)
    if a.shape != b.shape:
        raise ValueError(f"paired_significance: arrays must be equal length, got {a.shape} vs {b.shape}")
    n = int(a.shape[0])
    if n < 2:
        raise ValueError(f"paired_significance: need >= 2 paired observations, got {n}")

    d = a - b  # per-query difference (a − b); a = hybrid, b = vector for M123
    mean_diff = float(d.mean())
    wins = int((d > 0).sum())
    losses = int((d < 0).sum())
    ties = int((d == 0).sum())

    rng = np.random.default_rng(seed)

    # ── Paired permutation (randomization) test ──
    # Under H0 (label exchangeability) each pair's difference is equally likely +d_i or −d_i. Draw B random
    # sign vectors, recompute the mean, two-sided p = fraction with |perm mean| >= |observed mean|. The observed
    # assignment is one of the permutations, so p is (count+1)/(B+1) (never 0 — standard Monte-Carlo correction).
    obs = abs(mean_diff)
    signs = rng.choice(np.array([1.0, -1.0]), size=(n_resamples, n))
    perm_means = np.abs((signs * d).mean(axis=1))
    p_permutation = float((np.count_nonzero(perm_means >= obs - 1e-12) + 1) / (n_resamples + 1))

    # ── Paired bootstrap 95% percentile CI on mean(d) ──
    idx = rng.integers(0, n, size=(n_resamples, n))
    boot_means = d[idx].mean(axis=1)
    ci95_low = float(np.percentile(boot_means, 2.5))
    ci95_high = float(np.percentile(boot_means, 97.5))

    # ── Paired t-test cross-check ──
    sd = float(d.std(ddof=1))
    cohens_dz = float(mean_diff / sd) if sd > 0 else 0.0
    if sd > 0:
        t_stat = mean_diff / (sd / math.sqrt(n))
        try:
            from scipy import stats as _sp  # optional; only for the exact t-distribution p-value
            p_ttest = float(2.0 * _sp.t.sf(abs(t_stat), df=n - 1))
            p_ttest_method = "scipy.stats.t"
        except Exception:
            # normal approximation (t → N(0,1) as df grows; fine for the n≈300 BEIR test sets)
            p_ttest = float(math.erfc(abs(t_stat) / math.sqrt(2.0)))
            p_ttest_method = "normal-approx"
    else:
        t_stat = 0.0
        p_ttest = 1.0
        p_ttest_method = "degenerate-zero-variance"

    return {
        "n": n,
        "mean_diff": mean_diff,
        "ci95_low": ci95_low,
        "ci95_high": ci95_high,
        "p_permutation": p_permutation,
        "p_ttest": p_ttest,
        "p_ttest_method": p_ttest_method,
        "t_stat": float(t_stat),
        "wins": wins,
        "losses": losses,
        "ties": ties,
        "cohens_dz": cohens_dz,
        "seed": seed,
        "n_resamples": n_resamples,
    }
