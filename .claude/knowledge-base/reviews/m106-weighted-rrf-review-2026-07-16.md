---
slug: m106-weighted-rrf
milestone_id: M106
date: 2026-07-16
cycle: review
---

# /review — M106 weighted RRF

**Verdict:** READY_TO_MERGE (LOW fixed)

Independent adversarial review (council-security) focused on injection safety of the weight literals, validation completeness, backward-compat, and benchmark honesty.

## Verified
- **Injection safe:** weights validated `is_finite() && >= 0.0` BEFORE `format!("{:.6}")`; fixed-precision never uses exponent/scientific notation (tested f64::MAX, subnormals); NaN/inf rejected. The literal is a numeric `%6$s`/`%7$s` format arg in an arithmetic slot — no quote/paren/semicolon/comment reachable. No injection vector.
- **Validation complete:** negative/NaN/±inf → typed 22023 (err_input), no bypass; JSON path defaults 1.0, coerces int-or-float, delegates to the same validation.
- **Backward-compat holds:** positional `_hybrid_search_rrf` passes 1.0/1.0; default `1.0 *` is a score no-op → 324 pg_tests GREEN, 0 regression. The "byte-identical" claim is about score/result (honest; SQL text gains `1.000000 *` prefixes but the math is exact).
- **Benchmark honest:** math verified (1/61, 3/61); fixture logic sound (dv vector-only, df FTS-only via NULL embedding); all three ranking-flip claims follow deterministically; named tests exist. No spin.

## Finding — FIXED
- **[LOW]** IEEE-754 `-0.0` passed the `>= 0.0` guard and formatted to `"-0.000000"` (benign unary-minus, semantically 0.0 — NOT an injection). Fixed: normalize `-0.0 → +0.0` via `+ 0.0` before formatting (keeps the unsigned-decimal-literal invariant). Added `m106_negative_zero_weight_behaves_as_zero` (vector_weight=-0.0 disables the vector leg → df wins). 4/4 m106 pg_tests GREEN.

## Hard gates
✅ no BLOCKER · ✅ no HIGH · ✅ no secrets · ✅ no main commit · ✅ commit-trailer policy honored · ✅ CHANGELOG updated · 324 pg_tests GREEN + 1 (`-0.0` test).

**READY_TO_MERGE.**
