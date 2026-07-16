# ADR-0046: Disposition of the inert vendored `rabitq/` tree (delete-or-cfg-gate)

- **Status:** proposed
- **Date:** 2026-07-16
- **Deciders:** vector-pillar owner
- **Relates:** ADR-0032 (vendor rabitq-rs core), ADR-0036 (M74 RaBitQ lever verdict — memory not QPS)
- **Tags:** deletion-safety, yagni, zombie-dependency

> Drafted by the system-design audit (Phase 6). Convergent finding — flagged independently as
> a HIGH deletion zombie, a boundary leak, and a YAGNI risk.

## Context and problem statement

`src/rabitq/vendor/` is a **5651-LoC** Apache-2.0 vendored tree (7 `.rs` files, incl. a 119 KB
`simd.rs`) that is:

- **Not compiled** — there is no `mod rabitq;` in `src/lib.rs` (verified: lib.rs declares
  `am, ai_op, ann, … vindex, api` — no `rabitq`), and **zero** `rabitq::` references exist
  anywhere in the compiled crate. It is not in `Cargo.toml` or `build.rs`.
- **Mis-documented** — `VENDORED.md` claims integration edits rewrote imports to
  `crate::rabitq::vendor::…`, but the files still use the original standalone-crate root paths
  (`crate::simd`, `crate::math`, `crate::Metric`, …), so the tree would **not even compile**
  against the theodb crate root if declared. The stated anti-corruption boundary is fictional.

The vendoring itself was a legitimate, license-clean, spike-proven decision (ADR-0032; the M74
measurements in ADR-0036 used it). The problem is the **frozen-inert state**: 5651 LoC of
uncompiled code at commit `10b9a4e` that no CI touches → guaranteed bit-rot, and a
`VENDORED.md` that overclaims a wiring that does not exist.

## Decision drivers

- Anti-sunk-cost (CLAUDE.md): effort spent vendoring never justifies keeping dead weight.
- Uncompiled vendored code drifts silently — no compiler, no test, no CI guards it.
- Honesty (Rule 3): docs must match reality; VENDORED.md currently does not.
- ADR-0036 already decided **not** to build the full IVF-RaBitQ AM speculatively (D3 gate).

## Considered options

1. **Delete the tree** (`git rm -r src/rabitq/`) — history preserves it; re-vendor from
   upstream when/if billion-scale memory demand (D3) actually materializes.
2. **cfg-gate it** behind `#[cfg(feature = "rabitq_wip")]` with a real `mod rabitq;`
   declaration, perform the documented path rewrite so it **compiles** under the feature, and
   rewrite `VENDORED.md` to say "WORK IN PROGRESS — NOT WIRED" + a tracking issue.
3. **Leave as-is.** (Rejected — bit-rot + doc-overclaim is the worst of both.)

## Decision outcome

**Chosen: Option 1 (delete) unless a named, dated billion-scale feature is on the roadmap
within one milestone**, in which case Option 2 (compile-gate + honest docs + tracking issue).
Either way, **the current inert-and-overclaimed state is not acceptable.**

### Consequences

- **Good (Option 1):** removes 5651 LoC of guaranteed-to-rot code; git history + ADR-0032
  preserve provenance for a clean re-vendor later; VENDORED.md overclaim disappears.
- **Good (Option 2):** the code compiles and CI guards it against rot; the boundary becomes
  real and testable; intent ("future memory feature, not wired") is explicit.
- **Bad (Option 1):** a future re-vendor re-does the (small) vendoring effort — acceptable per
  anti-sunk-cost.
- **Bad (Option 2):** carries a compile-time feature and its maintenance for a not-yet-demanded
  feature — only justified if the demand is genuinely near.

## Validation

- `grep -r "mod rabitq" src/lib.rs` matches (Option 2) **or** `src/rabitq/` no longer exists
  (Option 1).
- `VENDORED.md` no longer claims a wiring that the crate does not implement.
