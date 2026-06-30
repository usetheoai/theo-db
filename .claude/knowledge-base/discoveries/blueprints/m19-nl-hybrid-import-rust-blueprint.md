# Blueprint — M19: NL→SQL + Hybrid + Import → Rust (end of plpython3u)

**Slug:** m19-nl-hybrid-import-rust · **Date:** 2026-06-30 · **Milestone:** M19 (ROADMAP-v2)
**Scope (user decision):** DoD-2 literal — port `ai.nl_to_sql`, `ai.nl_query`, `ai.hybrid_search(_rrf)`,
`theodb.import_pinecone(_chunked)` to Rust; remove `plpython3u` from `requires`; the `theodb` surface becomes
100% Rust (orchestration) over SQL/plpgsql primitives executed via SPI.

> **Nature:** internal port (Rule 9). Prior art = M17/M18 (`theodb_rs/src/{embed,chat,http,pg,lib}.rs`) + our
> own plpython3u/plpgsql source (the behavioral spec). Parity gates: `test_nl_sql.py` (35), `test_hybrid.py`
> (14, offline twin), `test_unified.py` (11). No external references.

## Critical finding (shapes the HOW, not the WHETHER)

**Only `ai.nl_to_sql` is `LANGUAGE plpython3u`** (`sql/60:26`). `ai.nl_query` (sql/60, plpgsql),
`ai.hybrid_search(_rrf)` (sql/40, plpgsql), `theodb.import_pinecone(_chunked)` (sql/80, plpgsql) are already
own-code. Removing plpython3u from `requires` hinges ONLY on porting `nl_to_sql`.

## ADR seeds

### A — `nl_to_sql` L4 stays `EXPLAIN (FORMAT JSON)` via SPI; NO Rust SQL parser
The relation-allowlist (L4) MUST enumerate relations by asking Postgres's planner (`EXPLAIN (FORMAT JSON,
VERBOSE false) <sql>` → walk the JSON for `"Relation Name"`/`"Schema"`). The plpython3u version does this
(`sql/60:89-115`) precisely because regex-lexing missed comma-joins + quoted identifiers (a documented review
BLOCKER, `sql/60:4-8`). A `sqlparser` crate in Rust would REOPEN that vulnerability class (parser-vs-planner
divergence: search_path, view→base-rel expansion) and risks tests 14/15/17/26. **Decision:** port the
orchestration to Rust; keep EXPLAIN-via-`Spi` + `serde_json` tree-walk for L4. Rejected: `sqlparser` crate
(security regression + new dep + parsimony).

### B — L2 static validation ports to Rust stdlib (no regex crate)
L2 = comment-strip + single-statement + `^(select|with)` + a 29-keyword `\b…\b` denylist + `do $$|call` guard
(`sql/60:60-83`) — all regex/string heuristics on a lowercased, comment-stripped copy. Port with stdlib
char-scanning (the house style: `chat.rs::first_token/first_number/strip_fence` hand-roll `re.*` to avoid a
regex crate). **Byte-faithful caveats:** (a) `\b` treats `_` as a word char (so the `pg_ls_` alternative
catches `pg_ls_anything(`); (b) block-comment strip is DOTALL (`/\*.*?\*/` over newlines); (c) the fence
strip here is `^```[a-zA-Z]*\n?` / `\n?```$` — DISTINCT from `chat.rs::strip_fence` (`[A-Za-z0-9_-]*`), do not
reuse it. Every L2 rejection → `pg::err_input` (22023) with the verbatim message (port map § 1 table).

### C — `hybrid_search(_rrf)` + `import_pinecone(_chunked)`: Rust #[pg_extern] orchestrating the SQL via SPI
The RRF fusion (RANK per leg → FULL OUTER JOIN → `1/(k+rank)` sum, `sql/40:76-107`) and the jsonb→INSERT loop
(`sql/80:32-46`) are inherently relational. Port = Rust functions that build the SAME parameterized,
`%I`-quoted dynamic SQL (preserving the embed-seam guard, the `'english'` FTS pin, the `score DESC, id ASC`
tie-break, the regclass/`%I` injection-safety) and run it via `Spi`; for import, loop the jsonb (`serde_json`
or `jsonb_array_elements` via SPI) + INSERT via `Spi`. Reimplementing RANK/JOIN/planner in Rust is rejected
(reinventing the engine — anti-objetivo). The chunked PROCEDURE's per-batch COMMIT stays a plpgsql PROCEDURE
OR a pgrx-equivalent — TBD in PLAN (COMMIT-in-pgrx via `Spi::run("COMMIT")` is fragile; keep plpgsql if so).

### D — Retirement migration `theodb--1.2--1.3.sql` + remove plpython3u from requires + README
Mirror the M17/M18 guarded retirement: conditional DROP of the legacy plpython3u `ai.nl_to_sql` (only when
plpython3u AND not a theodb_rs member); `theodb.control` `requires = 'vector, vectorscale'` (drop plpython3u);
`default_version → 1.3`; the keepers (`ai.nl_query`, hybrid, import if kept plpgsql) late-bound; README §
"Limitação honesta (plpython3u)" removed + the `CREATE EXTENSION` comment updated. Verify `sql/70` (model
registry) is plpython3u-free first (grep shows it is). Re-evaluate `superuser = true` (vectorscale/DiskANN may
still need it — keep unless proven removable).

## Security invariants (must not regress)

L1 prompt byte-identical (stub routes on `"read-only postgresql select"`); L2 denylist + single-statement +
SELECT/WITH-only verbatim; L4 EXPLAIN-based allowlist (schema-qualified OR bare-public match), fail-closed on
plan error; L3 (`ai.nl_query`, plpgsql) read-only sandbox (`set_config('transaction_read_only','on',true)` +
`statement_timeout 5000`, write → 25006) UNCHANGED; `%I`/regclass injection-safety in hybrid/import; every
function `REVOKE ALL FROM PUBLIC`.

## Benchmark (CTO requirement, measurement-first)

`benchmarks/bench_nl.py` (mirror bench_chat/bench_embed): nl_to_sql Rust vs a plpython3u `ai.nl_to_sql_py`
baseline against the same chat stub — honest I/O-bound no-regression framing (the L1 LLM call dominates;
the L2/L4 validation is the only added local work). Report `docs/benchmarks/m19-nl-rust-vs-plpython.md`.

## Coverage corners (internal port)

- **Integration tests:** `test_nl_sql.py` (35 — the parity gate, esp. the 12 injection cases), `test_unified.py`
  (11, import), `test_hybrid.py` (14, offline RRF twin). New `test_nl_edge.py` for any L2/L4 edge the stub
  cannot drive.
- **Dependencies:** none new — reuse `pgrx`/`serde_json`/`minreq`; NO `sqlparser` crate (ADR-A/B).
- **Tools:** `tools/chat_server.py` stub (`__nlinject_*` seams exist); `cargo pgrx`, clippy, ruff.
- **Techniques:** M17/M18 pgrx pattern + EXPLAIN-via-SPI relation enumeration + stdlib regex-equivalent scanning.

## References (internal)

- Behavioral spec: `sql/60-theodb-nl.sql` (nl_to_sql/nl_query), `sql/40` (hybrid), `sql/80` (import).
- Reuse: `theodb_rs/src/{lib,http,chat,embed,pg}.rs`. Parity oracle: `test_nl_sql.py`/`test_unified.py`/`test_hybrid.py` + `tools/chat_server.py`.
