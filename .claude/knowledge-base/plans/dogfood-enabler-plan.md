---
slug: dogfood-enabler
milestone_id: M124
created_at: 2026-07-20
goal: Ship the theo-db-side dogfood enabler — self-host quickstart + the anchor path proven end-to-end on a self-hosted TheoDB with first recorded evidence, flipping the dogfood manifest to `wired`
---

# Plan — M124 Dogfood enabler (theo-data capability retrieval on self-hosted TheoDB)

## Goal

Deliver the theo-db-side enabler for the dogfood anchor `theo-data-capability-on-theodb` — a reproducible
self-host quickstart, the anchor retrieval path (`theodb.create_vectorizer` → vectorizer bgworker → embedding
column → `ai.hybrid_search_rrf`) demonstrated end-to-end on a self-hosted TheoDB, and the **first** recorded
dogfood evidence (incl. ≥1 failure story) — flipping `knowledge-base/dogfood/manifest.md` from `planned` to
`wired`.

**Single metric:** the manifest status is `wired` AND ≥1 evidence file under
`knowledge-base/dogfood/evidence/` (with the § 5 frontmatter) records the anchor path passing on a self-hosted
TheoDB, backed by a re-runnable smoke.

## Context

Consumes the `/analysis` verdict (`knowledge-base/audits/2026-07-20-analysis.md`, Recommendation 1 / Risk H9):
engineering trajectory is validated (6 core hypotheses), but production-maturity is blocked on **zero sustained
real use** — the one gap synthetic benchmarks cannot close. The dogfood-golden-rule anchor is
`theo-data-capability-on-theodb`; the manifest is `planned`. This milestone delivers the enabler + first `wired`
evidence. **The flip to `running` (sustained ≥30-day real traffic) is operational/cross-repo and is explicitly
NOT gated by this code milestone** (ROADMAP M124 DoD item 4 + risk 2).

## Baseline Context

Repo state: git sha `11c4efa`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `docs/ops/self-host-quickstart.md` | — | (NEW) | Reproducible self-host recipe (pgrx-install + `shared_preload_libraries` + vectorizer worker + embedding GUCs). |
| `benchmarks/dogfood_anchor_smoke.sh` | — | (NEW) | Re-runnable smoke exercising the anchor path end-to-end (create_vectorizer → bgworker → hybrid_search_rrf). |
| `knowledge-base/dogfood/evidence/2026-07-20-anchor-smoke.md` | — | (NEW) | First evidence (outcome pass, § 5 frontmatter). |
| `knowledge-base/dogfood/evidence/2026-07-20-anchor-failure-modes.md` | — | (NEW) | Failure story (≥1) — a dogfood without failures is theatre (golden-rule § 4). |
| `knowledge-base/dogfood/manifest.md` | 20 | status `planned` | Flip to `wired`; document `planned → wired → running`. |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/vectorizer.rs:105` — `theodb.create_vectorizer(source_table, source_pk_col, content_col, target_table, target_col, model, dims, chunk_strategy, chunk_size, chunk_overlap)` — the anchor's freshness path (bgworker enqueues on DML).
- `theodb_rs/src/api.rs:662` — `ai.hybrid_search_rrf(tbl, id_col, content_tsv_col, vector_col, query_text, query_vector, k, per_leg_limit, result_limit, language, filter_sql, lexical_engine, content_text_col)` — the anchor's query path.
- `theodb_rs/src/embed.rs:173` — `resolve_cfg` reads GUCs `theodb.embedding_endpoint` / `theodb.embedding_model` / `theodb.embedding_api_key` (SSRF-hardened: http(s) only) — the embedding provider config.
- `rules/dogfood-golden-rule.md § 1` — anchor slug + description; `knowledge-base/dogfood/manifest.md` — the status the smoke advances.

### Domain glossary

- **anchor path** — the end-to-end retrieval a real capability uses: content table → `create_vectorizer` keeps an embedding column fresh via the bgworker → queries fuse FTS + vector via `ai.hybrid_search_rrf`.
- **`wired`** (dogfood-golden-rule § 2) — the anchor is invoked at least once in CI or a manual smoke. Distinct from `running` (actively used on real infra, sustained).
- **self-host** — a TheoDB instance the team runs itself (here: pgrx-install PG17 + `theodb_rs`), not a managed service.

### Architecture boundaries affected

Per `rules/architecture.md`: this milestone adds NO production Rust code — it composes existing surfaces
(`create_vectorizer`, `hybrid_search_rrf`, embedding GUCs) into a documented recipe + a smoke. No layer changes.
Per the workspace CLAUDE.md, HA/control-plane/deploy are out of this repo — the enabler is theo-db-side only.

## Prior Art & Related Work

- Internal: the M122 async vectorizer (`theodb_rs/src/vectorizer.rs`, `embed.rs`) is the freshness engine this
  anchor depends on (hardened `backend_xmin` fix, v0.108.0). The M123/M125 BEIR harness already drives
  `ai.hybrid_search_rrf` + the embedding GUCs against a self-hosted TheoDB — the same wiring the smoke reuses.
- The dogfood-golden-rule (`rules/dogfood-golden-rule.md`) is the contract; the `wired`/`running` vocabulary is
  the pgvector/pgai-style "vectorizer keeps embeddings fresh" pattern applied to our own DB.

## ADRs

### ADR M124-1 — deliver the theo-db-side enabler + `wired` evidence; do NOT claim `running`

**Decision:** M124 ships the self-host quickstart + the anchor smoke + first evidence, and advances the manifest
to `wired`. It does NOT flip to `running` — that requires sustained ≥30-day real traffic on a capability, which is
operational and lives in the capability/workspace repos.

**Rationale (cites `rules/dogfood-golden-rule.md § 2` + ROADMAP M124 DoD item 4 + Honesty Rule 3):** `running` is
defined as "actively used by the team on real infrastructure" — a session smoke cannot manufacture 30 days of real
use. Claiming `running` would be exactly the "dogfood theatre" the golden rule § 7 guards against. `wired` is the
honest, verifiable state this milestone reaches.

**Alternatives rejected:**
- **Flip straight to `running`** — REJECTED: no sustained real-use evidence exists; dishonest (golden-rule § 7).
- **Migrate a capability's real retrieval from here** — REJECTED: the capabilities (theo-rag/theo-memory) live in
  other repos; theo-db cannot migrate another repo's store. The enabler + reference wiring is the theo-db-side
  half; the capability-side migration is cross-repo (ROADMAP risk 1).

### ADR M124-2 — smoke is a re-runnable script proving the path, not a new production surface

**Decision:** the anchor is exercised by a shell smoke (`benchmarks/dogfood_anchor_smoke.sh`) that drives existing
SQL surfaces; no new Rust/production code is added.

**Rationale (cites `rules/parsimony-ladder.md` rung 1):** the anchor path already exists (`create_vectorizer` +
`hybrid_search_rrf` ship). The gap is *proof it works self-hosted end-to-end + recorded evidence*, not new code.
The cheapest correct deliverable is a documented recipe + a re-runnable smoke — writing production code here would
be YAGNI.

**Alternatives rejected:** a new pgrx `#[pg_extern]` "dogfood" helper — REJECTED (YAGNI; the surfaces exist).

## Dependencies

No new dependency (composes existing surfaces + a shell smoke). `## Dependencies`: **none** — no crate added.
External service: OpenAI embeddings endpoint (already used by M122/M123/M125 via the `theodb.embedding_*` GUCs;
key from `.env`, never committed).

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Reproducible self-host quickstart | T1 (quickstart doc) |
| Anchor path proven end-to-end on self-hosted TheoDB | T2 (anchor smoke run on the droplet) |
| First evidence incl. ≥1 failure story | T3 (evidence files) |
| Manifest advances `planned → wired`, documents path to `running` | T4 (manifest flip) |

## Phase 1 — enabler

### T1.1 — self-host quickstart doc

#### Why this step
DoD item 1: a team member must be able to stand up a self-hosted TheoDB from zero. Reasoning: document the exact
recipe proven on the droplet (pgrx-install PG17 + `theodb_rs` install + the vectorizer worker + the three
embedding GUCs), so the recipe is reproducible, not tribal knowledge.

#### Files to edit
- `docs/ops/self-host-quickstart.md` (NEW).

#### TDD
- RED: the doc must contain the exact, runnable commands (extension create, GUC set, `shared_preload_libraries` note, vectorizer worker check) — verified by the smoke (T2) re-running the doc's commands successfully.
- GREEN: the doc's commands are the ones the T2 smoke executes and passes.

#### Failure scenarios
- OpenAI endpoint unreachable / 5xx → the vectorizer job stays queued and the doc's troubleshooting section says how to observe it (job table); the smoke records this as a failure mode, not a hang.

#### Acceptance criteria
- The doc's commands match exactly what the T2 smoke runs (no divergence); a reader can copy-paste to stand up TheoDB + a vectorizer.

#### DoD
- `docs/ops/self-host-quickstart.md` exists with the full recipe + a troubleshooting section.

### T2.1 — anchor smoke end-to-end on self-hosted TheoDB

#### Why this step
DoD item 2: prove the anchor retrieval path works self-hosted. Reasoning: drive the exact path a capability uses —
create a content table, `theodb.create_vectorizer` to keep an embedding column fresh, insert rows, wait for the
bgworker to embed, then `ai.hybrid_search_rrf` a real query and assert non-empty fused results.

#### Files to edit
- `benchmarks/dogfood_anchor_smoke.sh` (NEW).

#### TDD
- RED: on a fresh self-hosted TheoDB with no vectorizer, `hybrid_search_rrf` over an unembedded column returns degenerate/empty vector-leg results — the smoke asserts the FRESH embedding column (post-vectorizer) yields non-empty fused top-k.
- GREEN: after `create_vectorizer` + bgworker run, the embedding column is populated and `hybrid_search_rrf` returns a ranked fused result set (FTS + vector legs both alive).

#### Failure scenarios
- Embedding endpoint 429/timeout → the vectorizer job remains pending (does not corrupt); the smoke surfaces the pending-job count as a diagnostic (not a silent pass).
- `theodb.embedding_api_key` unset → `create_vectorizer` succeeds but embedding jobs fail with a typed error; the smoke asserts the typed error, not a hang.
- SSRF guard: a non-http(s) endpoint is rejected with a typed input error (fail-closed) — asserted.

#### Acceptance criteria
- The smoke exits 0 with a populated embedding column + non-empty `hybrid_search_rrf` result on the self-hosted droplet; it is re-runnable (idempotent setup).

#### DoD
- `benchmarks/dogfood_anchor_smoke.sh` runs green on the self-hosted TheoDB; its output is captured in the T3 evidence.

### T3.1 — first evidence (pass + failure story)

#### Why this step
DoD item 3 + golden-rule § 4: record the first dogfood evidence with the § 5 frontmatter, INCLUDING ≥1 failure
story (a dogfood without failures is theatre). Reasoning: the evidence files are what the `/dogfood` gate reads to
attribute the `wired` status; the failure story is mandatory honesty.

#### Files to edit
- `knowledge-base/dogfood/evidence/2026-07-20-anchor-smoke.md` (NEW — outcome pass).
- `knowledge-base/dogfood/evidence/2026-07-20-anchor-failure-modes.md` (NEW — outcome partial/fail, the real failure modes observed running the smoke).

#### TDD
- RED: `/dogfood` hard cap #3 (`no_anchor_evidence`) fires when no evidence file has `scenario:` matching the anchor slug.
- GREEN: both evidence files carry `scenario: theo-data-capability-on-theodb` + full § 5 frontmatter; the failure file's `outcome:` is `partial` or `fail` with a real observed failure mode.

#### Failure scenarios
- (none — no external I/O in writing the evidence file; the failures it *documents* are from T2.)

#### Acceptance criteria
- ≥2 evidence files with valid § 5 frontmatter; ≥1 documents a real failure mode from the smoke run.

#### DoD
- Evidence files exist and validate against the golden-rule § 5 frontmatter shape.

### T4.1 — flip manifest to `wired`

#### Why this step
DoD item 4: advance the manifest honestly and document the `planned → wired → running` path. Reasoning: the smoke
proves the anchor is invoked (the `wired` bar); the flip records that, while explicitly stating `running` remains
operational/cross-repo.

#### Files to edit
- `knowledge-base/dogfood/manifest.md` (status `planned` → `wired`).

#### TDD
- RED: with status `planned`, `/dogfood` hard cap #2 (`anchor_not_running`) blocks a production-ready claim — correct (we do NOT claim it).
- GREEN: status is `wired`; the manifest documents what `wired` means here + the exact remaining step to `running` (sustained ≥30-day capability traffic).

#### Failure scenarios
- (none — a doc edit.)

#### Acceptance criteria
- Manifest status is `wired`; the `planned → wired → running` path + the operational gate to `running` are documented; NO `running`/`production-ready` claim is made.

#### DoD
- `knowledge-base/dogfood/manifest.md` status `wired`; honest path documented.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Cross-repo scope — the capability's real retrieval migration lives in theo-rag/theo-memory, not theo-db | MEDIUM | This milestone delivers the theo-db-side enabler (quickstart + wiring + first evidence) only; `running` is explicitly out of scope (ADR M124-1) | implementer |
| No sustained real traffic in-session → status stops at `wired`, not `running` | MEDIUM | Honest by design (ROADMAP risk 2): `wired` is the truthful state; the production-ready claim stays unmade until real use accrues | implementer |
| OpenAI dependency for the smoke (external I/O) | LOW | Key from `.env` (never committed); the smoke records provider failures as failure-story evidence rather than hiding them | implementer |

## Unresolved Questions

- Which theo-data capability will actually migrate its retrieval to self-hosted TheoDB to accrue `running`
  evidence? Resolved at plan time as **out of scope for this code milestone** (cross-repo, operational — ADR
  M124-1); the enabler is a precondition, not the migration itself.
- (none other — every in-scope decision is resolved at plan time.)

## Failure scenarios

- **Embedding endpoint 429/timeout during the smoke** — the vectorizer job stays pending (no corruption); the smoke reports the pending-job count as a diagnostic and it becomes a failure-story evidence entry, never a silent pass.
- **`theodb.embedding_api_key` unset** — embedding jobs fail with a typed error; the smoke asserts the typed error rather than hanging.
- **Non-http(s) embedding endpoint (SSRF attempt)** — rejected fail-closed with a typed input error (existing `embed.rs` guard); the smoke asserts the rejection.

## Global DoD

- `docs/ops/self-host-quickstart.md` (reproducible recipe) + `benchmarks/dogfood_anchor_smoke.sh` (green on the
  self-hosted droplet) + ≥2 evidence files (§ 5 frontmatter, ≥1 failure story) + manifest `wired`.
- No new production Rust code; no new dependency. CHANGELOG `[Unreleased]` updated. No secrets committed.
- NO `running` / `production-ready` claim (honesty — ADR M124-1). `/dogfood` at `wired` is the truthful state.
