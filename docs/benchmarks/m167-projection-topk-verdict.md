# M167 — projection top-k (q23–q26): measured verdict

**Date:** 2026-07-28 (rev 3 — supersedes drafts 1 and 2; see § 7)
**Box:** `theo-e2e-runner` (DigitalOcean, 32 GB / 8 vCPU) — NOT the canonical ClickBench c6a.4xlarge.
**Build:** `cargo pgrx install --release`, PG 18.4 (pgrx), `datcollate = C`, `work_mem = 64MB`, `shared_buffers = 4GB`.
**Data:** ClickBench `hits` / `hits_heap`, 1,000,000 rows each — verified by `count(*)`.
**Raw artifacts:** all under `docs/benchmarks/m167-artifacts/`, plus `docs/benchmarks/m167-type-coverage.md`
(the `rules/testing.md` § 5.1 gate artifact required for a change to the columnar routing admit-paths).

| Artifact | What it backs | Postmaster |
|---|---|---|
| `paired-ab-ctas.log` | § 1 headline (paired CTAS) | wall-clock only — 23:11–23:15Z, i.e. **pre-`e2d0955`** (§ 3) |
| `suite-final-binary.json` + `.log` | § 2 routing + suite A/B | **01:27:21Z (final binary)** |
| `hits-topk-ab.log` | § 3 1M top-k oracle — H0 `9/9`, FINAL GATE ok, `rc=0` | **01:27:21Z (final binary)** |
| `ec-harness.log` | § 3 fixture oracle — EC FINAL GATE ok (14 assertions), `rc=0` | **01:27:21Z (final binary)** |
| `h0-gate-positive-control.log` | § 3 proof the **H0** gate can fail, `rc=3` | **01:27:21Z (final binary)** |
| `final-gate-positive-control.log` | § 3 proof the **FINAL** gate can fail, `rc=3` | **01:27:21Z (final binary)** |
| `guard-proofs.log` + `benchmarks/m167_guard_proofs.sh` | § 5 ICU-provider and `relpages=0` proofs, `rc=0` | **01:27:21Z (final binary)** |
| `b1-latemat-on.json` / `b1-latemat-off.json` / `b1-control.log` | § 6 noise floor + § 7.2 control (both arms) | 23:26:20Z run (**pre-`bf809e7`**, § 3) |
| `before-1m-SUPERSEDED.json` | § 7.2 first table (the withdrawn baseline) | 27 Jul run |
| `after-1m.json` | superseded by `suite-final-binary.json`; kept for the record | 23:17Z or earlier |

Every log **except two** opens with `postmaster=<pg_postmaster_start_time()>` and closes with `rc=<exit code>`, so
provenance is a property of the artifact rather than a claim in this document. The exceptions are
`paired-ab-ctas.log` and `b1-control.log`, which predate that convention and carry only a wall-clock — their binaries
are identified in § 3 instead.

## 1. Result — paired same-binary A/B

The headline measurement toggles `theodb.enable_columnar_late_mat` **inside one session on one binary**, 5 alternating
off/on pairs per query, each arm materialized with `CREATE TEMP TABLE … AS` so the k surviving rows are actually
formed. Raw log: `m167-artifacts/paired-ab-ctas.log`; harness: `benchmarks/m167_paired_ab.sql`.

| Query | shape | off (median) | on (median) | ratio | pairs on&lt;off |
|---|---|---|---|---|---|
| q23 | `SELECT *` + `LIKE` + `ORDER BY EventTime LIMIT 10` | 21.3814 s | **4.2897 s** | **4.98×** | 5/5 |
| q24 | narrow + `ORDER BY EventTime LIMIT 10` | 6.1929 s | **0.1383 s** | **44.78×** | 5/5 |
| q25 | narrow + `ORDER BY SearchPhrase LIMIT 10` (text key) | 5.8742 s | **0.1046 s** | **56.18×** | 5/5 |
| q26 | narrow + `ORDER BY EventTime, SearchPhrase LIMIT 10` (multi-key) | 5.8529 s | **0.1405 s** | **41.67×** | 5/5 |

**20 of 20 pairs favour the new path.** Per-arm spread is tight (q23 off 21.33–22.31, on 4.15–4.79; the narrow three
have `on` between 0.094 and 0.168 s against `off` between 5.60 and 6.71 s).

Why paired-in-one-binary and not before-run vs after-run: **this box drifts up to ~2× between runs on sub-200 ms
queries**, measured on three queries the GUC cannot affect (§ 6, last bullet). A cross-run comparison therefore cannot
license a ratio on its own, whatever it reports. A GUC toggle inside one session removes build, cluster, session and
thermal drift by construction; the toggle is the only asymmetry left. A same-binary control corroborates these
numbers independently (§ 7.2).

## 2. Routing — the metric that actually discriminates

From `m167-artifacts/suite-final-binary.json` — the 43-query suite re-run against **the binary being merged**
(`postmaster=01:27:21Z`, `rc=0`, stamped in `suite-final-binary.log`):

| Query | `columnar_agg_routed` | `result_ab_identical` |
|---|---|---|
| q23 | **true** | true |
| q24 | **true** | true |
| q25 | **true** | true |
| q26 | **true** | true |

Suite totals: `columnar_agg_routed` **36/43**; `result_ab` **`diverged = 0`** over 42 passing queries
(1 ERRORED — q28, see § 6); `routed_identical` 36, `declined_trivial` 6, `no_pushdown_exercised: false`.

An earlier revision sourced this table from `after-1m.json`, which `git log` places **before** two Rust commits — so
the document asserted a provenance the repository contradicted. Re-run rather than re-worded.

**`columnar_customscan` is deliberately NOT cited here.** That field is `"theodb_columnar_agg" in plan or "Custom
Scan" in plan` (`run_m128_clickbench.py:271`) and its own docstring calls it "broad and ~always True … CANNOT tell an
agg pushdown from a declined agg over a projection scan".

The § 7.2 control (`b1-latemat-on.json` vs `b1-latemat-off.json`) proves it vacuous under the tightest possible conditions — same binary, same data, same session
parameters, **only the GUC differing**:

| | q23 | q24 | q25 | q26 |
|---|---|---|---|---|
| `columnar_customscan`, late-mat **on** / **off** | true / true | true / true | true / true | true / true |
| `columnar_agg_routed`, late-mat **on** / **off** | true / **false** | true / **false** | true / **false** | true / **false** |

The broad field does not move when the routing does. The first draft of this verdict cited it; that was a false-green.

## 3. Correctness — which oracle proves what

The two gates are not interchangeable, and the distinction is the reason M167 has both:

| Claim | Proven by | Why the other cannot prove it |
|---|---|---|
| No storage/aggregate regression across the suite | the 43-query A/B — 42/43 `result_ab_identical`, `diverged = 0` | it **strips the trailing LIMIT** (`run_m128_clickbench.py:283`) and order-normalizes both sides (`_canonical`, `:243`); with no `Limit` node the top-k swap declines (`columnar_agg.rs` parent check), so it never exercises the path |
| The top-k returns the right k rows **in the right order** | `benchmarks/m158_ec_harness.sql` — LIMIT-preserving symmetric-`EXCEPT` over full rows on a tie-free key, plus an emission-order oracle via `row_number()` | the suite A/B is blind to ordering by construction |

**Harness result:** every `*_ab_mism` / `*_order_mism` block **0**; the seeded negative control reports **2**, so the
zeros carry information. Blocks and their measured verdicts:

| Block | Shape | Expected | Measured |
|---|---|---|---|
| M167-A | `SELECT *` + `LIKE` + non-text key | route | route, `ab_mism = 0` |
| M167-B | narrow projection, emission-order oracle | route | route, `order_mism = 0` |
| M167-C | text key, DATABASE DEFAULT collation | route (cluster is `datcollate = C`) | route, `ab_mism = 0` |
| M167-C2 | text key, named `en_US.utf8` | decline | decline |
| M167-D | multi-key, first key ties | route | route, `order_mism = 0` |
| **M167-D2** | **multi-key with a TEXT second key** (q26's real shape) | route | route, `order_mism = 0` |
| **M167-D3** | second key `COLLATE "en_US.utf8"` | decline | decline |
| **M167-D3b** | `bpchar` as a non-first sort key | decline | decline |
| **M167-D4a** | **exactly 8 distinct keys** (== `TOPK_MAX_SORT_KEYS`) | route | route |
| **M167-D4** | **9 distinct keys** (over the ceiling) | decline | decline, `Sort Key: k1…k9` |
| M167-E | seeded divergence (negative control) | `> 0` | **2** |

D2/D3 matter most: they are the only place the two new mechanisms intersect. A per-key loop that validated key 0's
collation and forgot key 1's would pass every other test and fail exactly there.

**D4 was vacuous until this revision, and the committed log proved it.** It read
`ORDER BY v, wid, sd, bc, v, wid, sd, bc, v` over a **four-column** fixture: nine items, but PostgreSQL deduplicates
redundant pathkeys, so the plan showed `Sort Key: v, wid, sd, bc` — **four** keys. The ceiling was never approached
and the decline came from `bc` (bpchar), i.e. D4 was re-proving D3b under another name. A 4-column fixture cannot
express 9 distinct keys, so the fixture was the defect: `t_dc9` now carries 9 plain `int` columns. **D4a is the
boundary control** — at exactly 8 keys it must *route*, otherwise a decline at 9 would prove only that something
about the fixture failed, not that the ceiling fired. Measured: 8 keys → `Custom Scan (theodb_columnar_agg)`;
9 keys → `Sort Key: k1, k2, k3, k4, k5, k6, k7, k8, k9` over a declined scan, so the planner really did produce
nine pathkeys.

`benchmarks/columnar_type_ab.py` (the M163/M164 type-coverage gate, required by the ROADMAP DoD) carries four
projection-top-k routing cases: **35/35 as-expected, positive control `diverged = 2`**.

### At the measured scale — `benchmarks/m167_hits_topk_ab.sql`

The two oracles above run on fixtures (20k / 2k rows). This one runs on the **relation the numbers came from** —
1M rows, 105 columns — comparing columnar `hits` (late-mat ON) against the heap twin with the LIMIT preserved:

| Assertion | Measured |
|---|---|
| **H0 — routing precondition** (every shape the file runs reaches `theodb_columnar_agg`, no surviving `Sort`) | **ok, 9/9** |
| sort-key multiset, q23 / q24 / q25 / q26 | **0 / 0 / 0 / 0** |
| full rows under a total order (key↔payload alignment) | **0** |
| wide variant — full rows over all **105** columns | **0** |
| distinct values of the first sort key in those 20 rows | **1** — a total tie, so the tie-break decided every row |
| negative control (seeded divergence) | **40** |
| script exit code | **0** (`rc=0` line in `hits-topk-ab.log`, alongside the `postmaster=` provenance stamp) |

Two of those rows exist because a draft of this block failed them:

- **H0 is machine-checked, not printed.** It was first written as four bare `EXPLAIN`s for a human to eyeball. That
  is not a gate: if the swap declines — which it does at stock `work_mem`, by design (§ 6) — both arms run the same
  native plan and every block below reports 0 differences while proving nothing. H0 now `RAISE`s and, under
  `ON_ERROR_STOP`, aborts the whole oracle. **Positive control for the gate itself:** re-run at `work_mem = 64kB` and
  it stops at shape 1 with `M167-H0 FAILED … would pass vacuously`, executing no comparison block.
- **H0 must cover every shape it claims to guard, and at first it did not.** Its four shapes sort by
  `EventTime` / `SearchPhrase`, but the two full-row blocks (H5, H5b) sort by `CounterID, WatchID, UserID` — a 3-key
  shape H0 never checked. Those are precisely the blocks that catch key↔payload misalignment and the only comparison
  over all 105 columns, so they were the two left free to pass vacuously. A second pass found three more: H1, H2 and
  H4 project *different columns* than the four suite shapes (`EventTime` vs `SearchPhrase` vs both), and while their
  routing is implied — the decode guard bills the whole relation regardless of projection — "implied" is precisely
  what this gate exists to replace. The array now carries **all nine** SQL statements the file actually executes, and
  the gate reports **9/9** — the file itself executes **six** distinct shapes (H1, H2, H3, H4, H5, H5b) and the array
  additionally carries the four *suite* shapes q23–q26 (one of which H3 shares), because those are the shapes whose
  numbers this document publishes. Nine statements, six of them run below.
- **The tie-break must actually tie.** The first version tie-broke on `EventTime`, which turned out *unique* in its
  top-20 — the block passed while exercising no tie-break at all. `CounterID` ties completely (1 distinct value in
  20 rows), so the second and third keys decide every row.

**Binary provenance — including the headline's, which an earlier revision left unnamed.** Three binaries appear in
this document, and § 1 ran on the oldest of them:

| Section | Ran at | Binary |
|---|---|---|
| § 1 headline (`paired-ab-ctas.log`) | 23:11–23:15Z | **pre-`e2d0955`** — older than both binaries discussed below |
| § 7.2 control (both arms) | 23:37–23:53Z | pre-`bf809e7` (postmaster 23:26:20Z) |
| § 2 suite, § 3 oracles + both gate controls, § 5 guard proofs | 01:27–01:45Z | **final (`ad132ab`)**, postmaster 01:27:21Z |

§ 1's internal validity is untouched — both of its arms shared one binary, which is the only property a paired
toggle needs — but the document previously pinned § 2/§ 3 to the final binary while saying nothing about the number
that goes into the CHANGELOG. That is the § 2 defect displaced onto the headline, and it is disclosed rather than
re-measured because neither intervening commit can move a paired wall-clock: `e2d0955` caches a trace-env lookup in
a `OnceLock` — planning-time only, and **whatever its state, it was the same in both arms of the same session**,
which is the property that matters — and `bf809e7` is the fail-closed `INFINITY` flip on an unreachable
null-syscache path (§ below). Neither touches the executor, so neither can change how long a scan takes.

Both oracles, both gate self-tests, the § 5 guard proofs and the § 2 routing table were re-run **after** the final
commit, against a postmaster restarted at `01:27:21Z` so the shipped `.so` was the one loaded. This matters: the § 7.2 control ran on
the immediately-preceding binary (postmaster up since `23:26:20Z`, `.so` rewritten at `23:44:58Z` — PostgreSQL loads
`shared_preload_libraries` at startup, so the rebuild did not reach those two suites). That leaves the control
internally valid — **both** of its arms used one identical binary, which is the only property it needed — while the
correctness evidence is pinned to the code being merged.

The delta between those two binaries is `bf809e7`, and it is one behavioural line: `relation_physical_bytes` used to
return `0.0` when the `pg_class` syscache tuple was null (an unknown size read as "small" — **admit**) and now returns
`f64::INFINITY` (**decline**). It is a fail-closed flip on a path that is in practice unreachable, and it can only
make routing *stricter*, never looser — so it cannot have manufactured a route that the final binary would refuse.
The other four changes in that commit are a comment, a `pg_sys::COLLPROVIDER_LIBC` swapped in for a local `const` of
the same value, a `format!` moved behind its trace check, and a redundant cast. (An earlier revision of this
paragraph named the `datlocprovider` requirement as the delta — wrong: `git log -S datlocprovider` places it in
`6d6f78c`, which predates *both* binaries.)

**Every artifact now states its own provenance** rather than relying on this paragraph: each oracle log opens with
`postmaster=<pg_postmaster_start_time()>` and closes with `rc=<exit code>`, so a reader can pin a result to a
postmaster image without trusting prose. The two oracle logs, both gate self-tests, the § 5 guard proofs and the
§ 2 suite all report the `01:27:21Z` postmaster; `paired-ab-ctas.log` (§ 1, 23:11–23:15Z) and the § 7.2 control
(23:37–23:53Z) predate it, and the table above says so per artifact.

## 4. What changed

**q23/q24 — the boot default.** Both were already routable; `enable_columnar_late_mat` booted `off` and the harness
never set it. Flipping the default is the whole change for these two.

**q25 — byte-order proven, not allowlisted.** M158 admitted a text sort key only under collation OID 950/951. On a
`datcollate = C` cluster the column carries OID **100** (`default`), so a provably safe case was declined. The
predicate now resolves `default` against `pg_database.datcollate` **and requires `datlocprovider = 'c'`** — see § 5.

**q26 — multi-key.** `numCols != 1` was a scope limit, not a safety property. The wire format carries N keys, every
key is checked independently, one failing key declines the whole swap, ceiling `TOPK_MAX_SORT_KEYS = 8`.

**The decode bound (ADR-4).** With the default ON, an unfiltered wide top-k would decode the whole relation before the
bounded-heap TopK. The guard declines when the relation's size exceeds `work_mem × 8`.

## 5. Two correctness holes found in review and closed

Both were found by independent reviewers, re-verified here, and fixed before release. **Both demonstrations are
committed** as `m167-artifacts/guard-proofs.log` (`postmaster=01:27:21Z`, `rc=0`) and re-runnable via
`benchmarks/m167_guard_proofs.sh` — an earlier revision asserted
these two as measured while shipping no artifact for either, which is the same defect the reviewers had just made
me fix elsewhere.

**ICU provider (`datlocprovider`).** `CREATE DATABASE d LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LOCALE 'C'` stores
`datcollate = 'C'` while the DEFAULT collation orders by ICU (`pg_locale.c` dispatches on `datlocprovider`;
`dbcommands.c` writes the two fields independently). Reading `datcollate` alone admitted a text sort key whose
DataFusion byte order disagrees with PG — **the exact wrong-rows class the M158 guard existed to prevent**, made
reachable without a session `SET` by the default flip.

Proof A in `guard-proofs.log`: a database created exactly that way reports `provider=i datcollate=C` — it *says* C
— and the text sort key declines (`Sort Key: s` survives over `Custom Scan (theodb_columnar_project)`). Without the
`datlocprovider` clause the predicate would read `C` and admit.

**The guard was inert without ANALYZE.** `pg_class.relpages` is written only by ANALYZE/VACUUM, and a columnar table
never triggers either on its own — there is no `pgstat` counting anywhere in `theodb_rs/src/`, so autoanalyze never
fires, and `relation_vacuum` (`columnar.rs:1851`) is an error stub. Measured on a fresh 200k-row columnar table:
`relpages = 0` and **the guard did not fire even at `work_mem = 64kB`**. The first draft's guard demonstration only
worked because ANALYZE had been run by hand during investigation, unrecorded. Fixed by falling back to the relation's
true current size.

Proof B in `guard-proofs.log`: a fresh 200k-row columnar table reports `relpages = 0`, and on that same table the
guard declines at `work_mem = 64kB` (`Sort Key: v` survives) and routes at `1GB` (`Custom Scan
(theodb_columnar_agg)`). The fallback is what makes the guard bite at all on a table PostgreSQL never analyses.

## 5.5. ROADMAP M167 Definition of done — item by item

Each DoD bullet, the artifact that settles it, and an honest status. One is **partial**, and it is marked as such
rather than ticked.

| # | DoD bullet (ROADMAP.md:3054-3056) | Evidence | Status |
|---|---|---|---|
| 1 | q23–q26 show the projection/late-mat Custom Scan in `EXPLAIN` **and** byte-identical A/B vs heap (`diverged=0`), **with the `WHERE URL LIKE …` filter also routed** (composed with M156) | H0 gate `9/9` (`hits-topk-ab.log`); suite `diverged = 0`, `columnar_agg_routed` true for all four; the `LIKE` is proven inside the scan **from committed evidence**: H0 shape 1 is q23 verbatim (`m167_hits_topk_ab.sql:31`) and `try_swap_topk` returns `None` on any qual it cannot push down (`columnar_agg.rs`, un-pushable branch), so *routing entails the filter was pushed*. (An earlier revision cited an `EXPLAIN (VERBOSE)` run that was never committed — the same unartifacted-claim defect § 5 records two sections earlier, reappearing in the same commit set.) | **met** |
| 2a | `ORDER BY` on text routes **only** under a deterministic collation | M167-C (routes, `datcollate = C`), C2 (`en_US.utf8` → declines), D3 (non-first key `en_US.utf8` → declines), D3b (`bpchar` → declines); predicate reads `datcollate` **and** requires `datlocprovider = 'c'` (§ 5) | **met** |
| 2b | the LIMIT-k heapsort is **O(k)**, not an O(N) batch | The heap itself *is* bounded — DataFusion's `TopK` keeps k rows. **But the decode that feeds it is O(N)**: the whole relation is materialized into Arrow before the heap sees it. That is mitigated by the ADR-4 size guard, not eliminated. Peak RSS was **not** measured (§ 6). | **PARTIAL — see below** |
| 3 | late-mat GUC honoured; the M163/M164 type-coverage A/B exercises the projection-top-k case; `CHANGELOG [Unreleased]` | GUC is the sole asymmetry of the § 1 and § 7.2 measurements; `columnar_type_ab.py` carries 4 projection-top-k routing cases, **35/35** with positive control `diverged = 2` (`m167-type-coverage.md`); CHANGELOG entry present | **met** |

**On 2b, plainly:** this milestone does not make the path O(k) end-to-end and does not claim to. The plan said so
before any code was written (ADR-4: a streaming O(k) top-k is "real new executor mechanism, out of scope for this
milestone; recorded as the honest gap"). What shipped is a bounded heap fed by an O(N) decode, with a plan-time
guard that declines when the relation exceeds `work_mem × 8` — a ceiling on catastrophe, not the O(k) property the
bullet asks for. Ticking bullet 2b would be a false green.

**It is tracked, not merely noted:** [issue #215](https://github.com/usetheodev/theo-db/issues/215) carries the
instrumentation of the real decoded-batch size, the streaming-decode evaluation, and the question of whether the
default can hold at PostgreSQL's stock `work_mem`. When the M167 ROADMAP checkbox flips at release it records that
the milestone shipped — **not** that bullet 2b's O(k) clause was met; that clause lives in #215 until it is.

## 6. What is NOT proven

- **PostgreSQL's stock `work_mem` is 4 MB**, giving a budget of 32 MB — below the 1M-row `hits`. On a stock cluster
  these queries **decline**. The measurements here were taken at `work_mem = 64MB`. The headline "routes by default"
  is true of the default *GUC*, not of a default *cluster*.
- **Peak memory of the top-k path is UNMEASURED.** `VmRSS` is dominated by the 4 GB of `shared_buffers` mapped into
  every backend (both arms returned an identical 4,705,368 kB); per-PID sampling did not capture the transient
  backend. What is proven is the *bound* (`est_bytes` vs `budget`), not the actual peak. A reviewer noted the batch
  size is already computed in-process (`df_executor.rs:585` `batch.get_array_memory_size()`) and one trace line would
  expose it — not done here.
- **Beyond 1M rows.** M162 measured the columnar *scan* at 100M and hit `byte array offset overflow` (Arrow varlena
  i32 offsets > 2 GB); a wide top-k decode can reach the same class.
- **Parallel plans.** The harness sets `max_parallel_workers_per_gather = 0`.
- **The guard bills the whole relation**, ignoring projection width and filter selectivity, so a narrow projection
  over a large relation can be declined even though its decode would be small. And because on-disk bytes are
  compressed, the decoded Arrow batch is *larger* than the estimate — for an OOM bound that under-estimation is the
  **dangerous** direction (false admits). It is a ceiling on catastrophe, not a tight bound.
- **q28 exceeds the 60 s query ceiling on this cluster** and is ERRORED in both arms, so it is the one query with no
  A/B verification. It completed in 33.64 s in `m166-clickbench-agg.json`, so calling it "pre-existing" (as the first
  draft did) is not supported; the cause on this cluster is unexplained.
- **This box has between-run drift up to ~2× on sub-200 ms queries, measured.** The § 7.2 control exposed it (both arms committed; the
  numbers below are `b1-latemat-on.json` vs `b1-latemat-off.json`, **not** `after-1m.json`, which is a different
  run of the same arm): three queries moved between the two arms whose plans the GUC provably did **not** change.

  That is measured, not argued. Comparing `columnar_agg_routed` element-wise across the two suites, the flag differs
  on **exactly four queries — q23, q24, q25, q26 — and on no others**. The three below kept an identical routing
  decision in both arms, so whatever moved their wall-clock, it was not this change. (An earlier revision justified
  their immunity with "none has a `Sort` node". That was wrong — q8 is `… ORDER BY u DESC LIMIT 10` and does have
  one; what makes it immune is that its `Sort` sits over an `Agg` rather than a `theodb_columnar_project`, so
  `try_swap_topk`'s parent check never matches. The element-wise routing comparison is both stronger and simpler,
  because it does not depend on my reading of any plan.)

  | Query | shape | late-mat on | late-mat off | delta |
  |---|---|---|---|---|
  | q29 | `SUM(ResolutionWidth), SUM(+1), …` | 0.0661 s | 0.1243 s | −46.8% |
  | q8 | `RegionID, COUNT(DISTINCT UserID) GROUP BY` | 0.0953 s | 0.1593 s | −40.2% |
  | q5 | `COUNT(DISTINCT SearchPhrase)` | 0.1452 s | 0.1229 s | +18.1% |

  Within each run the three repetitions are tight and non-overlapping (q29: 0.066/0.069/0.075 vs 0.124/0.126/0.131),
  so intra-run CV reports high confidence in a number that does not reproduce across runs — the classic
  underestimation Georges et al. (OOPSLA'07, `papers/rigorous-perf-eval-georges-2007.pdf`) describe.

  **Consequence: no claim below ~1.9× is supportable on this box from a cross-run comparison.** That is why § 7.1's
  withdrawal stands and why § 7.2 is corroboration rather than evidence. It is **not** the reason the § 1 ratios are
  trusted — a floor measured *between* runs does not govern a measurement taken *within* one session, where the
  toggle is the only asymmetry and the arms interleave. Citing it as their justification (as an earlier revision of
  this bullet did) is a category error, and it does not survive its own arithmetic: against a 1.88× floor
  (0.1243/0.0661, the largest GUC-immune move), q24/q25/q26 clear it by 23.8×/29.9×/22.2× — but **q23 clears it by
  2.65×, not by an order of magnitude**. What licenses q23 is its own paired arms, which do not overlap
  (off 21.33–22.31 s, on 4.15–4.79 s, 5/5 pairs), not its distance from this floor.

  Nothing here explains *why* the box drifts; it is characterized, not diagnosed.

## 7. What the first two drafts of this verdict got wrong

Recorded because the errors are more instructive than the result.

### 7.1 — draft 1: a cross-run comparison, withdrawn for the right reason

The first draft compared a "before" suite run against an "after" suite run and published **6.51× / 42.51× / 62.07× /
41.88×**. It was withdrawn because a cross-run comparison cannot, on its own, exclude build/cluster/thermal drift —
the instrument does not license the claim. That reason still holds and the withdrawal was correct.

### 7.2 — draft 2: the right withdrawal, the wrong diagnosis

Draft 2 went further and asserted *why*: that the `before` baseline was inflated, citing
`docs/benchmarks/m166-clickbench-agg.json` (same box, same parameters, one day earlier, late-mat off) which records
q24 **3.0751 s** / q25 **2.7126 s** / q26 **2.9036 s** against the baseline's 5.9088 / 6.0528 / 5.9517. **That
diagnosis is falsified.**

The control that settles it — **both arms committed**, `m167-artifacts/b1-latemat-on.json` and
`b1-latemat-off.json`, driver `b1-control.log`: the **same binary**, the **same suite**, run
twice back-to-back, changing only `enable_columnar_late_mat`.

| | q23 | q24 | q25 | q26 | geomean (43q) |
|---|---|---|---|---|---|
| baseline `before-1m` (27 Jul, late-mat off) | 21.5151 | 5.9088 | 6.0528 | 5.9517 | 0.36917 |
| control arm (28 Jul, **same binary**, late-mat **off**) | 24.0039 | 6.7435 | 6.0978 | 6.8404 | 0.36313 |
| delta | +11.6% | +14.1% | +0.7% | +14.9% | **−1.6%** |

The withdrawn baseline **reproduces**. It was never inflated. `m166` is the outlier, and this verdict does not have
an explanation for it — stating one would repeat the same mistake at one more level of confidence.

With the drift hypothesis dead, three independent instruments now agree on the effect:

| Instrument | q23 | q24 | q25 | q26 |
|---|---|---|---|---|
| cross-run (draft 1, withdrawn as method — see note) | 6.51× | 42.51× | 62.07× | 41.88× |
| paired CTAS in one session (§ 1 — headline) | 4.98× | 44.78× | 56.18× | 41.67× |
| same-binary GUC control (§ 7.2) | 7.10× | 48.51× | 63.19× | 40.81× |

The headline stays the paired CTAS row: it is the only one where the toggle is the sole asymmetry *and* the k rows
are actually materialized. The other two are corroboration, not evidence to average.

The draft-1 row is quoted from that draft, **not reproducible from this repository**: its "after" suite had
`hot_geomean 0.25108` and that JSON was overwritten before it was ever committed (no artifact here carries that
geomean). It is listed for the record of what was claimed, and it is the one row a reader cannot check.

**And this control is itself a cross-run comparison** — two suite invocations 16 minutes apart — so § 1's own
objection applies to it. It is cited anyway, for a stated reason: its ratios (7.10×–63.19×) clear the measured
between-run floor of 1.88× by 3.8× to 33.6×, so drift cannot manufacture them. It could not license a 1.5× claim,
and this verdict does not make one.

### 7.3 — two method errors, both self-caught

1. **A `count(*)` wrapper erased the effect.** The first paired attempt wrapped each query in
   `SELECT count(*) FROM (…)`, which lets PostgreSQL skip materializing the projected columns — exactly the cost late
   materialization saves. Measured under that wrapper: q23 **1.03×**, q24 **0.99×**. The mechanism was being optimized
   away by the instrument. CTAS fixed it.
2. **Running `columnar_type_ab.py` against the same database DROPped and recreated `hits`** with its 2000-row
   synthetic schema, destroying the ClickBench data. Two measurements taken afterwards were reading the type fixture
   and were void. Data reloaded and everything re-measured.

The direction was never in doubt; the magnitude was, and the instrument is what decided it.

## 8. Reproduction

```bash
ssh root@165.227.121.20            # confirm the host with `doctl compute droplet list`, never from memory
P=/root/.pgrx/18.4/pgrx-install/bin

# headline: paired same-binary A/B
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres -q -f /root/theo-db/benchmarks/m167_paired_ab.sql"

# correctness oracle (must be all 0, control > 0)
su - pgtest -c "$P/psql -h localhost -p 28900 -d postgres -U postgres -f /root/theo-db/benchmarks/m158_ec_harness.sql"

# type-coverage routing gate (35/35)
cd /root/theo-db && PGHOST=localhost PGPORT=28900 PGDATABASE=postgres PGUSER=postgres \
  python3 benchmarks/columnar_type_ab.py     # NOTE: recreates `hits` — never against a ClickBench database

# suite (storage oracle + columnar_agg_routed)
cd /root/theo-db && PGHOST=localhost PGPORT=28900 PGDATABASE=postgres PGUSER=postgres \
  python3 benchmarks/run_m128_clickbench.py --agg --n 1000000 --sample systematic --out after.json
```

Companion artifacts: `m167-baseline-and-routing-facts-2026-07-28.md` (pre-code measurement; note its `lc_collate` GUC
recommendation was falsified during implementation — PG 18 has no such GUC), and the review report at
`.claude/knowledge-base/reviews/m167-projection-topk-review-2026-07-28.md`.
