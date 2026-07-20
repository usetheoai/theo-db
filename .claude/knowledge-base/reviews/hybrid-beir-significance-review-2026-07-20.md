# Review — M123 paired significance of hybrid vs vector (BEIR/SciFact)

**Date:** 2026-07-20 · **Slug:** hybrid-beir-significance · **Milestone:** M123
**Branch:** develop (commits `45d6294`, `6fa9d8a` + LOW-fix follow-up) · **Verdict:** READY_TO_MERGE

## Scope

A paired significance test (permutation p + bootstrap CI + t-test cross-check) over the hybrid vs vector per-query
nDCG@10 arrays, wired into the BEIR harness, with a MEASURED verdict on SciFact. `theodb_bench/significance.py`,
`run_m53_hybrid_beir.py` (`_paired_sig`), 8 unit tests, `docs/benchmarks/m123-hybrid-significance.{md,json}`, plus
two harness fixes needed for the run (OpenAI 429 backoff; `theodb_rs` own-vector extension).

## Reviewer — council-benchmark (benchmark rigour + honesty) → **SOUND (no BLOCKER/HIGH/MEDIUM)**

The reviewer independently RE-MEASURED every claim ("você mediu ou está supondo?"):

- **Permutation test CORRECT** — textbook two-sided paired randomization (Smucker CIKM 2007); the `(count+1)/(B+1)`
  Monte-Carlo correction is right; the `>= obs − 1e-12` tolerance biases only conservatively (cannot manufacture
  significance); null (a==b) reproduces p=1.0. The reviewer reconstructed the exact combinatorial p over the 2⁴=16
  informative sign patterns = **4/16 = 0.25**, which the MC estimate (0.253) converges to — the "not significant"
  is a correct small-support result, not a bug.
- **Bootstrap CI valid** (percentile, straddles 0). LOW: coarse with 4 informative pairs — noted in the report.
- **PARITY verdict HONEST, no spin** — the harness verdict rule is conjunctive+conservative (`SIGNIFICANT` only if
  `p<0.05 AND ci_low>0`); the report explicitly calls the +0.004 "within noise" and does not claim hybrid wins;
  the pre-declared-endpoint / no-dataset-shopping contract (ADR M123-2) is honored (it explicitly declines to run
  NFCorpus to fish for significance).
- **Candidate-set parity independently corroborated** — the reviewer found vector Recall@100 == hybrid Recall@100
  == 0.9733 exactly (the FTS leg added zero relevant docs to the top-100) — harder evidence than the prose; now
  cited in the report.
- **Harness fixes correct** — 429 backoff affects completion not values (embeddings cached/deterministic); the
  `theodb_rs` switch binds BOTH legs to theodb's own vector, so the hybrid-vs-vector DIFFERENCE is internally
  consistent + the engine is disclosed twice.
- **numpy-vs-scipy fallback necessary + correct** — scipy is un-importable in the env (numpy binary skew, exactly
  the fragility the docstring cites); the normal-approx t-test p (0.19259) matches the exact t(df=299) p (0.19359)
  to 0.001 — a fine cross-check at n=300.

## Findings (all LOW/praise — none block merge)

| Sev | Finding | Disposition |
|---|---|---|
| LOW | Bootstrap CI is coarse (4 informative pairs) + percentile-not-BCa — worth a one-line note; verdict-invariant | **FIXED** — added a CI-coarseness note to the report |
| LOW | "296 ties (hybrid == vector)" means nDCG@10-equal, not ranking-identical | **FIXED** — clarified the wording |
| LOW (praise) | The equal Recall@100=0.9733 is stronger candidate-set-parity evidence than the prose | **FIXED** — cited it in the candidate-set caveat |

## Gate checks

- 8/8 unit tests pass (null→p≈1, clear shift→significant, ties counted, bad input→typed error, qid-alignment).
- No new hard dependency (numpy already present; scipy optional). No secrets committed; OpenAI key file removed
  from the droplet post-run. No `Co-Authored-By`; no direct commit to `main`. CHANGELOG updated.
- Measured evidence: SciFact 300 queries, Δ̄=+0.0041, 95%CI=[−0.0010,+0.0108], p_perm=0.253, 296/300 ties →
  PARITY (`docs/benchmarks/m123-hybrid-significance.md`).

## Verdict

**READY_TO_MERGE** — a single domain review SOUND with no BLOCKER/HIGH/MEDIUM; the reviewer independently
re-derived the p-value and confirmed the test is statistically correct and the parity verdict is honest (no
p-hacking, full disclosure). A clean honest-negative: measurement-first, pre-declared endpoint, conservative
significance rule. The three LOW notes are fixed. Proceed to `/release`.
