# Blueprint: Permissive BM25 lexical ranking for PostgreSQL (non-AGPL)

> **Version 1.0** — This blueprint resolves M7-S2's open question: is there a **permissive (non-AGPL)** way
> to give TheoDB a *true* Okapi BM25 lexical-ranking leg, given that the BM25 SOTA witness — ParadeDB
> `pg_search` — is **AGPL-3.0** and barred by PRD D1 / CLAUDE.md TheoDB rule 2? It investigates the ParadeDB
> BM25 surface (study-only), the PostgreSQL-native FTS baseline (`ts_rank_cd` + GIN/RUM, already shipped in
> M7-S1), the candidate permissive BM25 extensions, the Okapi BM25 algorithm itself, and the AlloyDB SOTA
> surface. The load-bearing result: a **verifiably-permissive AND mature** true-BM25 extension exists —
> `timescale/pg_textsearch` under the **PostgreSQL License** — alongside an Apache-2.0 fallback
> (`psql_bm25s`); the two AGPL/Elastic-licensed options (ParadeDB `pg_search`, VectorChord-bm25) are
> fail-closed excluded. It informs the adopt-vs-own-vs-keep-native decision for M7-S2.

**Slug:** `m7-bm25-permissive`
**Source plan:** `.claude/knowledge-base/discoveries/plans/m7-bm25-permissive-plan.md`
**Owner:** paulohenriquevn
**Generated:** 2026-06-28 via `/discover-execute`
**Confidence verdict:** SHIPPABLE_WITH_CAVEATS (placeholder — updated by `/discover-confidence`)

## Context

ROADMAP `### M7` top-risk #1 + DoD: "Sem peça permissiva madura para BM25 full-text (paradedb `pg_search` é
AGPL)" → "**alternativa permissiva** identificada para full-text BM25". M7-S1 confirmed
`.claude/knowledge-base/references/paradedb/LICENSE:1` is the "GNU AFFERO GENERAL PUBLIC LICENSE" (AGPL-3.0),
barred by D1, and shipped hybrid search with the lexical leg on PostgreSQL-native `ts_rank_cd` + GIN
(the pattern witnessed in `.claude/knowledge-base/references/pgvector/README.md:629`). The deferred question:
can a *true* BM25 leg (Okapi TF-IDF with document-length normalization — the field standard) be obtained on
permissive terms? BM25 generalizes better than cover-density ranking on heterogeneous corpora, so closing the
gap would strengthen the hybrid leg's recall — **but the magnitude of that gain on TheoDB's corpus is
`UNBENCHMARKED`** (R3). The SOTA anchor (ADR `0002-north-star-equal-or-superior-to-alloydb.md`) is
AlloyDB + ParadeDB `pg_search`. This discovery executes the license due-diligence (D1 is a release gate —
PRD §11) and identifies the permissive replacement.

## Objective

Decide the permissive BM25 path for M7-S2: which option (adopt a permissive extension vs own BM25-in-SQL vs
keep native `ts_rank_cd`), backed by verbatim license evidence + the Okapi BM25 algorithm + an integration shape.

---

## Coverage Corner 1 — Integration Tests

> How the OSS stack asserts lexical-ranking correctness (Q1). Three assertion styles emerge: **membership**
> (which rows match), **rank order** (relative ordering), and **exact score** (the numeric relevance value).

### supabase-postgres — PostgreSQL-native FTS (membership assertions)

The Supabase FTS regression test asserts **membership only** — it checks *which rows* a `@@` match returns,
never a numeric score or order:

- **Pattern**: `to_tsvector(...)` `@@` `to_tsquery(...)` predicates filter rows; no `ts_rank` appears in this file.
  ```sql
  -- .claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:42-48
  select * from books
  where to_tsvector(description) @@ to_tsquery('big');
  ```
- **Fixtures**: a 5-row `books` table seeded inline (`:9-28`); a `stored` generated `tsvector` column + GIN index
  for the indexed path (`:103-105`: `add column fts tsvector generated always as (...) stored; create index
  books_fts on books using gin (fts);`).
- **Coverage**: it asserts the *recall set* (boolean match), leaving *ranking* to other tests — the honest gap
  this blueprint targets, since membership ≠ relevance ordering.

### supabase-postgres — RUM index (rank-order assertions)

The RUM test asserts **rank order** via a distance operator, the permissive index-served ranking path:

- **Pattern**: `a <=> to_tsquery(...)` produces a rank; `ORDER BY` that distance + a `round()` snapshot
  asserts the ordering.
  ```sql
  -- .claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql:31-37
  select t, round(a <=> to_tsquery('english', 'beautiful | place')) as rank
  from v.test_rum
  where a @@ to_tsquery('english', 'beautiful | place')
  order by a <=> to_tsquery('english', 'beautiful | place');
  ```
- **Fixtures**: `create index rumidx ... using rum (a rum_tsvector_ops)` (`:27`) with a `tsvector_update_trigger`
  (`:11-19`).
- **Coverage**: asserts relevance *order*, not an exact BM25 score — RUM ranks by tsvector distance, not Okapi BM25.

### paradedb pg_search — BM25 score function (exact-score assertions) — STUDY ONLY (AGPL/D3)

The ParadeDB regression test asserts the **exact numeric BM25 score** via `pdb.score(id)`:

- **Pattern**: `content @@@ 'technology'` matches; `pdb.score(id)` returns the BM25 relevance used in `ORDER BY`
  and even threshold predicates (`WHERE sp.relevance > 0.5`).
  ```sql
  -- .claude/knowledge-base/references/paradedb/pg_search/tests/pg_regress/sql/columnar_advanced_06_score_function.sql:64-68
  SELECT title, pdb.score(id), rating
  FROM score_test
  WHERE content @@@ 'technology'
  ORDER BY title, pdb.score(id), rating DESC ...
  ```
- **Fixtures**: a `bm25` index over mixed field types with per-field tokenizers
  (`:47-54`: `CREATE INDEX score_test_idx ON score_test USING bm25 (id, title, content, ...) WITH (key_field='id',
  text_fields='{"title":{"tokenizer":{"type":"default"}...}}')`).
- **Coverage**: asserts both membership *and* an exact float score + relevance buckets
  (`:200-204` `CASE WHEN pdb.score(id) > 0.8 THEN 'High Relevance' ...`). **Read for technique only — AGPL,
  never copied** (D3); the file header `bm25_search.rs:5-8` is the AGPL notice ("GNU Affero General Public License
  ... version 3").

**Test-shape takeaway for M7-S2:** TheoDB's permissive BM25 test should assert **rank order on a fixed corpus**
(the RUM-style `ORDER BY ... LIMIT k` membership-of-top-k assertion is deterministic and license-clean), and —
if a true-BM25 extension is adopted — an **exact-score snapshot** like ParadeDB's, but written from scratch
against the adopted extension's own operator.

---

## Coverage Corner 2 — Dependencies

> License + maturity of every candidate BM25 piece (Q2). **License verdicts are sourced verbatim from each
> candidate's canonical repo, never memory (D2).** CLAUDE.md TheoDB rule 2 admits only Apache-2.0 / MIT / BSD /
> PostgreSQL License into the distribution; AGPL and Elastic License v2 are barred.

| Piece | License (verbatim source) | Mature? | In TheoDB distribution? |
|---|---|---|---|
| **ParadeDB `pg_search`** | **AGPL-3.0** — "GNU AFFERO GENERAL PUBLIC LICENSE / Version 3" (`.claude/knowledge-base/references/paradedb/LICENSE:1-2`; code header `paradedb/tests/tests/bm25_search.rs:5-8`) | Yes (PG 15+, `pg_search/README.md:8`) | **NO** — AGPL barred by D1 (study-only witness) |
| **VectorChord-bm25** (`tensorchord/VectorChord-bm25`) | **Dual AGPLv3 / Elastic License v2** — "This software is licensed under a dual license model: GNU Affero General Public License v3 (AGPLv3)" + ELv2 (`raw.githubusercontent.com/tensorchord/VectorChord-bm25/main/LICENSE`) | Yes — v0.3.0 (2025-12-15), active, 6 releases (`github.com/tensorchord/VectorChord-bm25`) | **NO** — neither AGPL nor Elastic-v2 is permissive (E1 dual-license trap) |
| **`timescale/pg_textsearch`** (Tiger Data) | **PostgreSQL License** — "Permission to use, copy, modify, and distribute this software and its documentation for any purpose, without fee, and without a written agreement is hereby granted..." (`raw.githubusercontent.com/timescale/pg_textsearch/main/LICENSE`) | **Yes — GA** v1.3.1 (2026-06-23), "v1.4.0-dev — Production ready", 3.8k stars, 278 commits, 20 releases, not archived (`github.com/timescale/pg_textsearch`) | **YES** — PostgreSQL License is permissive (CLAUDE.md rule 2) ✅ |
| **`Intelligent-Internet/psql_bm25s`** | **Apache-2.0** — "Apache License / Version 2.0, January 2004" (`raw.githubusercontent.com/Intelligent-Internet/psql_bm25s/main/LICENSE`) | Partial — independent, self-reported benchmarks only (`UNBENCHMARKED` independently) | **YES (fallback)** — Apache-2.0 is permissive ✅, but less proven than pg_textsearch |
| **`tensorchord/pg_bestmatch.rs`** | **UNVERIFIED** — LICENSE not fetched; same maintainer as the dual-licensed VectorChord-bm25 → treat as suspect until confirmed | Generates BM25 *sparse vectors* only — not an index access method (needs pgvector to search) | **NO (until verified)** — fail-closed (D1) |
| **PostgreSQL-native FTS** (`ts_rank`/`ts_rank_cd` + GIN/RUM) | **PostgreSQL License** (built into the engine; no added dependency) | Yes — shipped in M7-S1 | **YES** — but **not BM25** (see Corner 4 / Q5) |

**Fail-closed outcome (E1):** the two AGPL/Elastic options are excluded by license, not by preference. The
recommendation draws **only** from the verbatim-permissive set: `pg_textsearch` (PostgreSQL License),
`psql_bm25s` (Apache-2.0), or native FTS.

---

## Coverage Corner 3 — Tools

> Install/build cost: native FTS (zero-install) vs a compiled BM25 extension vs SQL-owned BM25 (Q3).

| Option | Install / build cost | Ships in TheoDB image? |
|---|---|---|
| **Native `ts_rank_cd` + GIN/RUM** | **Zero** — built into PostgreSQL; RUM is a small contrib-style extension already used by the SOTA. Witness: GIN index `using gin (fts)` (`supabase-postgres/.../docs-full-text-search.sql:105`), RUM `using rum (...)` (`.../z_15_rum.sql:27`) | Already shipped (M7-S1) |
| **`pg_textsearch`** (PostgreSQL License) | Compiled PostgreSQL extension (PGXS-style build into the image, comparable to building `pgvector`: `make && make install`, cf. `pgvector/README.md:24-30`). Adds Block-Max WAND + `CREATE INDEX CONCURRENTLY` (`github.com/timescale/pg_textsearch`) | **Yes** — permissive, so it can be bundled |
| **`psql_bm25s`** (Apache-2.0) | Compiled PostgreSQL access-method extension; build into the image | Yes — permissive |
| **`pg_search` (ParadeDB)** | Heavy — Rust + `cargo-pgrx 0.18.1` + `libclang`, builds tantivy (`paradedb/pg_search/README.md:14-43`, `:68`) | **No** — AGPL barred regardless of build cost |
| **SQL-owned BM25** (plpgsql over `ts_stat`) | **Zero install** — pure SQL/plpgsql functions; no compiled artifact | Yes — but reinvents `pg_textsearch` (Rule 9; see D1) |

**Takeaway:** the cheapest *clean* path that yields *true BM25* is adopting `pg_textsearch` (build cost ≈ pgvector,
already in the image's build pipeline). SQL-owned BM25 has zero install but trades it for maintenance + a
Rule-9 reinvention. Native `ts_rank_cd` has zero cost but is not BM25.

---

## Coverage Corner 4 — Techniques

### Technique 1 — The Okapi BM25 formula (Q4) — SOTA anchor: ParadeDB `pg_search` / Lucene

**SOTA anchoring (R1):** ParadeDB `pg_search` (via tantivy) and AlloyDB-class lexical engines score with Okapi
BM25; TheoDB's permissive equivalent must compute the same function. The algorithm is the public
Robertson/Spärck-Jones technique — citable from the primary source, never from AGPL code (D3).

The standard Okapi BM25 score of document *D* for query *Q* (the form reproduced across the IR literature):

```
score(D, Q) = Σ_{qi ∈ Q}  IDF(qi) · ( f(qi, D) · (k1 + 1) )
                          ─────────────────────────────────────────────
                           f(qi, D) + k1 · ( 1 − b + b · |D| / avgdl )
```

where `f(qi,D)` = term frequency of `qi` in `D`; `|D|` = document length (in terms); `avgdl` = average document
length over the corpus; `IDF(qi)` = inverse document frequency of `qi`; `k1` controls term-frequency saturation
and `b` controls document-length normalization.

- **Citation (primary source):** Robertson, S. & Zaragoza, H. (2009), *The Probabilistic Relevance Framework:
  BM25 and Beyond*, Foundations and Trends in Information Retrieval 3(4):333–389, **DOI 10.1561/1500000019**
  (resolves via `doi.org/10.1561/1500000019` → the article "The Probabilistic Relevance Framework: BM25 and
  Beyond" — identity confirmed by the DOI redirect).
- **Default parameters `k1 ≈ 1.2`, `b = 0.75`** — the TREC/Okapi defaults (corroborated by WebSearch on the
  Robertson & Zaragoza framework; `k1→0` approaches a binary model, `b=1` fully length-normalizes, `b=0`
  disables length normalization). **Second independent witness (R2):** the permissive `pg_textsearch` repo
  exposes exactly these as its tunables — `k1` (default **1.2**, range 0.1–10.0) and `b` (default **0.75**,
  range 0.0–1.0) (`github.com/timescale/pg_textsearch`). The two sources agree on the canonical defaults.

### Technique 2 — BM25 over PostgreSQL native FTS statistics, and the gap vs `ts_rank_cd` (Q5)

**Feasibility verdict: BM25-in-SQL is computable from native statistics** — every BM25 input is exposed by
PostgreSQL with no added dependency:

| BM25 input | Native PostgreSQL source |
|---|---|
| `f(qi, D)` (term frequency) | positions/lexemes in the row's `tsvector` (`to_tsvector(...)`, `docs-full-text-search.sql:30`) |
| document frequency / `IDF` | `ts_stat(...).ndoc` = "number of documents (tsvectors) the word occurred in" (`www.postgresql.org/docs/current/textsearch-features.html`) |
| `N` (corpus size) | `count(*)` over the documents |
| `|D|` (document length) | `length(vector tsvector) returns integer` — "number of lexemes stored in the vector" (`www.postgresql.org/docs/current/textsearch-features.html`) |
| `avgdl` | `avg(length(tsvector))` across the corpus |

**Gap vs `ts_rank_cd` (the M7-S1 default):** PostgreSQL's native ranking is **not BM25**. The docs state
`ts_rank` "ranks vectors based on the frequency of their matching lexemes" and `ts_rank_cd` "computes the
*cover density* ranking … as described in Clarke, Cormack, and Tudhope … 1999" — proximity-based, **not Okapi
TF-IDF** (`www.postgresql.org/docs/current/textsearch-controls.html`; the docs do **not** mention BM25 at all).
Its length-normalization is a coarse integer bitmask (`1` = divide by `1 + log(doclen)`, `2` = divide by
`doclen`, `4` = mean-harmonic-distance, etc.) — none of which reproduce BM25's *saturating* normalization
`(k1+1)·f / (f + k1·(1 − b + b·|D|/avgdl))`. So `ts_rank_cd` and BM25 are different relevance models.

**Performance posture (E2 — honest):** a SQL/plpgsql BM25 over `ts_stat` would be a **post-filter compute**, not
an index-served scan — `ts_stat` aggregates the whole corpus per call. Its latency, and the recall@k *gain* of
true BM25 over the shipped `ts_rank_cd` on TheoDB's corpus, are **`UNBENCHMARKED`** (seeds for M2's recall
harness). No "faster/better" claim is made here (R3 / `public-copy.md`).

### Technique 3 — SOTA surface and the permissive equivalent (Q6)

| Layer | SOTA surface | Permissive equivalent TheoDB can own | Gap |
|---|---|---|---|
| BM25 index DDL | ParadeDB: `CREATE INDEX ... USING bm25 (...) WITH (key_field=..., text_fields='{tokenizer...}')` (`paradedb/.../columnar_advanced_06_score_function.sql:47-54`) | `pg_textsearch`: `CREATE INDEX ON documents USING bm25(content) WITH (text_config='english', k1=1.5, b=0.8)` (`github.com/timescale/pg_textsearch`) | Both `USING bm25`; pg_textsearch is single-column-per-index, PostgreSQL-License |
| Query / score op | ParadeDB: `content @@@ 'technology'` + `pdb.score(id)` (`columnar_advanced_06...sql:64-68`); VectorChord: `<&>` negative-BM25 | `pg_textsearch`: `ORDER BY content <@> 'search terms'` | Different operator spelling; same true-BM25 semantics |
| Hybrid fusion | AlloyDB: `ai.hybrid_search()` fuses vector + text with **Reciprocal Rank Fusion (RRF)**; lexical leg uses **PostgreSQL `ts_rank`-style scoring over RUM/GIN**, vector via **ScaNN/HNSW** — **not native BM25** (AlloyDB docs via `cloud.google.com` search: "Full-text search overview", "Run a hybrid vector similarity search"; see UNVERIFIED note) | TheoDB M7-S1: `ts_rank_cd` + RRF (`pgvector/README.md:629-632`) | **TheoDB already matches the AlloyDB lexical approach** (ts_rank-style + RRF); BM25 would *exceed* the SOTA's lexical leg, not merely match it |

**R1 anchor + honest gap:** The SOTA's *exposed BM25* lives in ParadeDB (AGPL); **AlloyDB itself does not ship a
native BM25** — its hybrid uses ts_rank-style lexical scoring fused by RRF over RUM/GIN + ScaNN. So TheoDB's
shipped `ts_rank_cd` + RRF is already at AlloyDB-parity for the lexical leg; adopting `pg_textsearch` would
*surpass* it. Whether that surplus is worth the dependency is a **measurement-first (D3)** question, not a
foregone conclusion.

---

## Cross-cutting Comparison

| Dimension | ParadeDB `pg_search` | VectorChord-bm25 | `pg_textsearch` (Tiger) | `psql_bm25s` | Native `ts_rank_cd` | SQL-owned BM25 |
|---|---|---|---|---|---|---|
| **License (verbatim-sourced)** | AGPL-3.0 (`paradedb/LICENSE:1`) | Dual AGPLv3/ELv2 (repo LICENSE) | **PostgreSQL License** (repo LICENSE) | **Apache-2.0** (repo LICENSE) | PostgreSQL License (built-in) | PostgreSQL License (own code) |
| **Ships permissively?** | ❌ No | ❌ No | ✅ **Yes** | ✅ Yes | ✅ Yes | ✅ Yes |
| **True Okapi BM25?** | ✅ Yes | ✅ Yes | ✅ **Yes (k1/b)** | ✅ Yes | ❌ No (cover-density) | ✅ Yes (if implemented right) |
| **Maturity** | GA, PG15+ | v0.3.0, active | **GA v1.3.1, 3.8k★, 20 rel.** | Independent, less proven | Shipped (M7-S1) | None (would be new code) |
| **Install cost** | Heavy (Rust/pgrx/tantivy) | Compiled ext | Compiled ext (≈pgvector) | Compiled ext | Zero | Zero (plpgsql) |
| **Index-served ranking?** | ✅ | ✅ | ✅ (Block-Max WAND) | ✅ | ✅ (GIN/RUM) | ❌ (post-filter compute) |

---

## ADRs

### D1 — M7-S2 deliverable: identify `pg_textsearch` (PostgreSQL License) as the permissive BM25 path; gate integration on a recall benchmark; reject own-in-SQL

**Decision:** For M7-S2 ("permissive BM25 alternative identified"), the identified path is **adopt-candidate
`timescale/pg_textsearch`** — a verifiably-permissive (PostgreSQL License), GA-mature, *true* Okapi-BM25
extension (k1=1.2/b=0.75 defaults). **Integration is gated** on a reproducible recall@k benchmark proving BM25
beats the already-shipped `ts_rank_cd` on TheoDB's corpus (PRD D3 / measurement-first). Until that benchmark
exists, **keep `ts_rank_cd` + RRF as the shipped default** (it is already at AlloyDB lexical-parity).
**`psql_bm25s` (Apache-2.0) is the documented fallback** if `pg_textsearch` proves unsuitable. **Own-BM25-in-SQL
is rejected.**

**Rationale:** (a) **License is settled by evidence, not preference** — `pg_textsearch`'s LICENSE reads verbatim
"Permission to use, copy, modify, and distribute this software … without fee" = the PostgreSQL License, which
CLAUDE.md rule 2 explicitly admits. (b) **Don't reinvent (Rule 9 / parsimony ladder rung 4):** a mature,
permissive, index-served true-BM25 extension already exists — hand-rolling BM25-in-SQL would be *accidental*
complexity (`CLAUDE.md` "Esforço ≠ Complexidade"), reproducing `pg_textsearch` worse and as a non-index-served
post-filter (Q5/E2). (c) **Measurement-first (D3, anti-sunk-cost):** the recall gain of BM25 over `ts_rank_cd`
is `UNBENCHMARKED`, and AlloyDB itself ships ts_rank-style + RRF (Q6) — so adopting a new dependency before
measuring would be effort without justified complexity.

**Alternatives considered:**
- **(B) Own BM25-in-SQL over `ts_stat`** — feasible (all inputs native: `ts_stat.ndoc`, `length(tsvector)`,
  `avg(...)`), and maximally D1-clean, but **rejected**: Rule-9 reinvention of `pg_textsearch`, post-filter
  perf (`UNBENCHMARKED`), ongoing maintenance. Kept only as a last resort if *no* permissive extension survives
  due-diligence (it did survive — so B is unnecessary).
- **(C) Keep native `ts_rank_cd` only** — zero-cost, already AlloyDB-parity, but **not true BM25**; weaker on
  heterogeneous corpora (gain `UNBENCHMARKED`). Retained as the *interim shipped default*, not the M7-S2
  identification (the DoD asks for the BM25 *alternative*, which C is not).
- **VectorChord-bm25 / ParadeDB `pg_search`** — rejected fail-closed: dual-AGPL/ELv2 and AGPL respectively (D2).

**Consequences:** M7-S2 ships an *identification* (this blueprint + a pinned `pg_textsearch` evaluation note),
not a forced integration. The recall@k harness (M2's first item) becomes the gate that converts the
identification into an integration. TheoDB stays D1-clean throughout. If `pg_textsearch`'s benchmark does not
beat `ts_rank_cd`, C remains and the dependency is never added (YAGNI honored).

### D2 — License fail-closed: verbatim-from-canonical-repo, exclude dual/Elastic licenses

**Decision:** No candidate enters the recommendation without a license quoted verbatim from its canonical repo;
**dual-license (AGPL-or-Elastic) and Elastic License v2 are treated as non-permissive and excluded**, exactly
like pure AGPL.

**Rationale:** D1 is a release gate (PRD §11) and a license verdict from memory is the Rule-3 violation this
discovery must avoid (E1). VectorChord-bm25's LICENSE reads "dual license model: GNU Affero General Public
License v3 (AGPLv3)" + Elastic License v2 — **neither** branch is in CLAUDE.md rule 2's permissive set
(Apache-2.0 / MIT / BSD / PostgreSQL). A dual license does not become permissive because one *can* pick the
Elastic branch — ELv2 is a source-available, non-OSI license.

**Alternatives considered:** "ELv2 is fine for self-hosted" — rejected; TheoDB is a *distributed* download
(PRD), and ELv2's usage restrictions are incompatible with an Apache-2.0 distribution. Assume-permissive on a
recognizable maintainer — rejected (Rule 3); `pg_bestmatch.rs` (same maintainer as the dual-licensed
VectorChord) is therefore marked **UNVERIFIED**, not assumed.

**Consequences:** Only `pg_textsearch` (PostgreSQL License) and `psql_bm25s` (Apache-2.0) survive into the
recommendation. The barred set is documented as study-only witnesses.

### D3 — Borrow the BM25 technique (public algorithm), never AGPL code

**Decision:** The Okapi BM25 formula is taken from the **primary source** (Robertson & Zaragoza 2009,
DOI 10.1561/1500000019), with the permissive `pg_textsearch` repo as a second witness for the `k1=1.2, b=0.75`
defaults. ParadeDB `pg_search` is read **only** to understand the surface (index DDL, `@@@`, `pdb.score`);
**no AGPL code, schema, or test is copied.**

**Rationale:** D1 bars AGPL *artifacts* from the distribution; algorithms are public and not the licensed
artifact. The formula must be cited line-exact to be implementable/auditable.

**Alternatives considered:** Cite ParadeDB's implementation as the formula authority — rejected (D3/E3: risks
treating AGPL code as the source). Cite only one source for the defaults — rejected (frontier R2 requires ≥2;
satisfied by the paper + the pg_textsearch repo).

**Consequences:** Any TheoDB test/score assertion is written from scratch against the *adopted* extension's
operator, citing the paper for the model — never derived from ParadeDB's AGPL test suite.

## Recommendations for the project

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | **Record `timescale/pg_textsearch` (PostgreSQL License, GA v1.3.1, true BM25 k1=1.2/b=0.75) as the identified permissive BM25 alternative for M7-S2** — this satisfies the ROADMAP DoD "alternativa permissiva identificada". | Q2, Q4, Q6, **D1**, CLAUDE.md rule 2 | **HIGH** |
| 2 | **Gate the actual integration on a reproducible recall@k benchmark** (`pg_textsearch` BM25 vs shipped `ts_rank_cd`) before adding the dependency — do not integrate on faith. | Q5, Q6, **D1/D3**, PRD D3, `public-copy.md` | **HIGH** |
| 3 | **Keep `ts_rank_cd` + RRF as the shipped lexical default** until rec #2's benchmark justifies the switch (it is already at AlloyDB lexical-parity). | Q6, **D1 alt-C**, parsimony-ladder | **HIGH** |
| 4 | **Exclude ParadeDB `pg_search` and VectorChord-bm25 from the distribution** (AGPL-3.0 / dual-AGPL+ELv2); keep them as study-only witnesses. Mark `pg_bestmatch.rs` UNVERIFIED until its LICENSE is fetched. | Q2, **D2**, E1/E4 | **HIGH** |
| 5 | **Record `psql_bm25s` (Apache-2.0) as the documented fallback** if pg_textsearch is unsuitable; verify its independent maturity before relying on it. | Q2, **D1/D2**, E4 | **MEDIUM** |
| 6 | **Do NOT own BM25-in-SQL** unless every permissive extension fails due-diligence — it reinvents pg_textsearch (Rule 9) and is post-filter, not index-served. Keep the `ts_stat`/`length(tsvector)` feasibility note as the last-resort design. | Q5, **D1 alt-B**, Rule 9, E2 | **LOW** |

## Blocked questions (if any)

| Question | Reason | Suggested human follow-up |
|---|---|---|
| (none) | All 6 questions answered. | — |

> Non-blocking caveats: (a) **AlloyDB exact-quote UNVERIFIED-host** — the canonical `cloud.google.com/alloydb/docs/ai/hybrid-search` and `.../work-with-embeddings` pages **301-redirect to `docs.cloud.google.com`** (a host *not* in `rules/discover-web-allowlist.txt`); the AlloyDB surface (RRF via `ai.hybrid_search()`, ts_rank-style lexical over RUM/GIN, ScaNN/HNSW, **no native BM25**) is sourced from `cloud.google.com`-domain search snippets and corroborated by local witnesses (`pgvector/README.md:629`, `supabase-postgres/.../z_15_rum.sql`), not a verbatim page quote. (b) **`pg_bestmatch.rs` license UNVERIFIED** (LICENSE not fetched). (c) **BM25-vs-`ts_rank_cd` recall gain + post-filter perf UNBENCHMARKED**; **`psql_bm25s` self-reported QPS** is the authors' claim, not independently reproduced.

## Halt-loop progress (audit trail)

- Iterations used: 1 (single-pass execution; all sources reachable on first attempt)
- Questions answered: 6 / 6
- Questions blocked: 0
- Local citations verified (by reading the file + line): 8 distinct files — `paradedb/LICENSE`,
  `paradedb/tests/tests/bm25_search.rs`, `paradedb/pg_search/README.md`,
  `paradedb/pg_search/tests/pg_regress/sql/columnar_advanced_06_score_function.sql`,
  `supabase-postgres/nix/tests/sql/docs-full-text-search.sql`, `supabase-postgres/nix/tests/sql/z_15_rum.sql`,
  `pgvector/README.md` (hybrid + distance ops)
- Web sources fetched (allowlisted), with license verdicts where applicable:
  1. `raw.githubusercontent.com/tensorchord/VectorChord-bm25/main/LICENSE` → **Dual AGPLv3 / Elastic v2 (barred)**
  2. `github.com/tensorchord/VectorChord-bm25` → maturity (v0.3.0, active)
  3. `raw.githubusercontent.com/timescale/pg_textsearch/main/LICENSE` → **PostgreSQL License (permissive ✅)**
  4. `github.com/timescale/pg_textsearch` → maturity (GA v1.3.1, 3.8k★) + BM25 surface + k1/b defaults
  5. `raw.githubusercontent.com/Intelligent-Internet/psql_bm25s/main/LICENSE` → **Apache-2.0 (permissive ✅)**
  6. `www.postgresql.org/docs/current/textsearch-controls.html` → ts_rank/ts_rank_cd are NOT BM25 (cover density)
  7. `www.postgresql.org/docs/current/textsearch-features.html` → `ts_stat`, `length(tsvector)` native stats
  8. `doi.org/10.1561/1500000019` → BM25 paper identity confirmed (Robertson & Zaragoza)
  9. WebSearch `cloud.google.com` (AlloyDB hybrid) + WebSearch `github.com` (permissive candidates)
- UNVERIFIED markers: AlloyDB exact-page quote (host redirect), `pg_bestmatch.rs` license
- UNBENCHMARKED markers: BM25-vs-`ts_rank_cd` recall gain; SQL-owned/post-filter BM25 perf; `psql_bm25s` QPS
- Promise: this single-pass execution answered all 6 questions with all 4 corners populated and no fabricated
  citation; ready for `/discover-confidence`.

## Related

- Discovery plan: `.claude/knowledge-base/discoveries/plans/m7-bm25-permissive-plan.md`
- Project rules: `.claude/rules/discover-phd-rigor.md` (R1/R2/R3 frontier rigor), `.claude/rules/public-copy.md`
  (no unbenchmarked perf claims), `.claude/rules/parsimony-ladder.md` (adopt-vs-own rung 4 / Rule 9)
- Project decisions: `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (SOTA anchor), PRD D1 (no AGPL),
  PRD D3 (fork/adopt only on reproducible benchmark — measurement-first), CLAUDE.md TheoDB rule 2 (license set)
- Primary source: Robertson & Zaragoza (2009), *The Probabilistic Relevance Framework: BM25 and Beyond*,
  DOI 10.1561/1500000019
