# ADR 0042 — M99: build an OWN-CODE columnar Table Access Method (supersedes 0041's DEFER for the own-code path)

**Status:** Proposed (owner sign-off pending) · **Date:** 2026-07-14 · **Milestone:** M99 · **Decision:** GO (own-code only)

## Context

ADR 0041 (M97) **DEFERRED** a new columnar/HTAP pillar. Its rationale is correct and stands: every *adoptable*
columnar differentiator is license-barred — Hydra columnar and Citus columnar are **AGPLv3**, pg_mooncake's sync
engine (moonlink) is **BSL 1.1**, ParadeDB/pg_analytics is **AGPLv3** — all barred by the D1 license gate
(Apache-2.0 / MIT / BSD / PostgreSQL only). 0041 concluded: keep the shipped pg_duckdb (MIT) route.

**What 0041 did NOT evaluate:** building the in-Postgres columnar Table Access Method **from scratch as own code**,
studying the AGPL designs as *literature only* (algorithms/layouts are not copyrightable) and reimplementing on
permissive pieces (pgrx + pg_sys + arrow-rs codecs). This is the exact posture already ratified for the vector
pillar against AGPL VectorChord (see the `vectorchord-agpl-study-only` precedent + M60/M69 own-code `public.vector`).

M99 discovery (2026-07-14, council-index-storage + council-research-adr, reading the real Hydra/Citus/cstore_fdw
references) confirmed: the design is settled (heap-catalog metadata delegating MVCC + synthetic TID + stripe/chunk/
zone-map layout, append-only) and is **D1-legal only as own code** — no AGPL code copied or linked.

**Honesty correction (Rule 3):** the M98 roadmap amendment mislabeled M99 as "Hydra-model, Apache-2.0". Hydra's
`columnar/` subtree is **AGPLv3** (`hydra/README.md:83`). This ADR + the roadmap edit + a CHANGELOG `[Unreleased]`
correction fix that factual error. The only Apache-2.0 native-columnar reference is `cstore_fdw` — an FDW (not a
TableAM), deprecated in favor of Citus columnar.

## Decision

**GO on M99 as OWN CODE**, superseding 0041's DEFER *for the own-code path only*. 0041's bar on *adopting* AGPL/BSL
columnar code remains fully in force. Concretely:

- Build a `theodb_columnar` `TableAmRoutine` in Rust/pgrx from scratch: study Hydra/Citus (AGPL) as design
  literature; **copy no AGPL source; link no AGPL library.** cstore_fdw (Apache-2.0) and arrow-rs (Apache-2.0) are
  the permissive reuse surfaces.
- Scope: **append-only analytical** (INSERT/COPY/seq-scan/aggregate + index-fetch); UPDATE/DELETE/tuple-lock/
  serializable/parallel/bitmap/sample callbacks are typed-`ERROR` stubs (the Citus base surface). "Updatable
  columnar HTAP" is explicitly out of scope — claiming it would be over-claiming (M73/M97 discipline).
- Reuse TheoDB's own storage surface: `am/page.rs` (GenericXLog/WAL), `am/tid.rs` (TID codec), `am/mod.rs` (AM
  registration idiom) — this is NOT greenfield.

## Rationale

1. **Own-code was never on 0041's ballot.** 0041 weighed *adopt AGPL/BSL* vs *keep pg_duckdb*. It did not weigh
   *build permissively from scratch* — the same option that produced TheoDB's own `public.vector` type and IVF/HNSW
   AMs. This ADR adds that third option and selects it for the columnar TableAM.
2. **D1-legal.** Algorithms and on-disk layouts are not copyrightable; a clean-room Rust reimplementation on pgrx +
   arrow-rs violates no license. Precedent: the vector pillar vs AGPL VectorChord.
3. **The MVCC-correctness trick is reusable for free.** Delegating stripe visibility to a heap catalog row's MVCC
   (Citus/Hydra design) means we do NOT re-implement MVCC — the single highest-risk thing a TAM could do. This keeps
   essential complexity essential (parsimony ladder) and correctness Postgres-native.
4. **Differentiator vs the shipped pg_duckdb route.** pg_duckdb is a *separate* engine/planner (ADR 0023: `ERROR:
   DuckDB execution is not supported inside functions`). A native TableAM is *in-core PG storage* — the substrate the
   single-planner pillar (M100 DataFusion CustomScan) needs to push scans into. M99 is the storage half of that seam.

## Alternatives considered (and rejected)

- **A — Keep DEFER (0041 unchanged).** Rejected: leaves the single-planner pillar (M100-M103) with no native
  columnar substrate; the pg_duckdb route is paradigm-blocked from a single plan (ADR 0023). 0041's DEFER was for
  *adoption*, not for the own-code path this ADR opens.
- **B — Adopt Hydra/Citus columnar directly.** Rejected: **AGPLv3, barred by D1** (this is exactly 0041's finding).
- **C — Embed Parquet-file-per-stripe (arrow-rs) instead of a bespoke PG-fork stripe layout.** Deferred, not
  rejected: trades PG-native WAL/crash integration for format reuse (the pg_duckdb/pg_mooncake choice). Recorded as
  the considered alternative for Decision (c) in the plan; M99 chooses bespoke PG-fork framing + arrow-rs *codecs*
  (zstd/lz4) so crash-safety stays Postgres-native while compression is reused (Rule 9, parsimony rung 4).
- **D — Custom metadata pages with hand-rolled visibility (instead of heap catalog tables).** Rejected: re-implements
  MVCC — forbidden by the parsimony ladder + Rule 9; discards the entire correctness trick.

## Consequences

- M99 ships an append-only columnar TableAM; UPDATE/DELETE is a later bet (its only reference impl, Hydra `row_mask`,
  is AGPL — must be clean-room designed when/if pursued).
- Aborted stripes leak disk space until a rewrite (accepted, documented — same as Citus).
- No serializable isolation on columnar (RC/RR only) — recorded as an explicit non-goal.
- The correctness proof is **not** de-riskable without concurrency permutation tests (`pg_isolation_regress`) — those
  are a non-optional DoD item, not a nice-to-have.

## Verification

Governance-only ADR (no code). The M99 plan (`knowledge-base/plans/m99-*-plan.md`) cites this ADR as the
license/scope contract; `/plan-confidence` reads it. Owner sign-off flips Status → Accepted.
