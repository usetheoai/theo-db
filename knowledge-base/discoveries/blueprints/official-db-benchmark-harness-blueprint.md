# Blueprint: Official DB benchmark harness — canonical benchmark per pillar + the adopt-and-wrap verdict

> Discovery executed 2026-07-20 via 4 parallel web-evidenced research passes (REGRA MÁXIMA). Every number carries
> a source URL + date or is marked `UNBENCHMARKED`. Feeds the roadmap program M127 (vector) / M128 (columnar) /
> M129 (OLTP) / M130 (HTAP). Discovery plan: `knowledge-base/discoveries/plans/official-db-benchmark-harness-plan.md`.

## Context

TheoDB's ~40 bespoke benchmark scripts + `theodb_bench/` (190 artifacts) are self-authored — heavy on vector,
near-zero standard coverage on columnar/OLTP/HTAP. `rules/public-copy.md § 4` requires a **third-party-reproducible**
artifact for any comparative claim, which a self-authored harness cannot give. The owner decided (2026-07-20) to
adopt the benchmarks **officially used by DB-engineering teams**, across all four pillars, with real datasets. This
blueprint selects the canonical benchmark per pillar, the run protocol against a PG-wire-compatible engine, the
dataset licenses vs the D1 gate, and — decisively — answers whether "replacing" the bespoke harness drops
capabilities.

## Objective

Name the canonical official benchmark per pillar + its protocol + entry contract + license posture, and resolve the
critical replacement question with a RETAIN/DROP verdict. **Headline finding (unanimous across all four pillars):
NONE of the official tools provide paired statistical significance testing OR byte-identical result-regression
A/B — so a pure "replace" would drop capabilities TheoDB shipped in 2026 (M123/M125 significance; M114/M126
byte-identical A/B). The evidence-backed architecture is ADOPT-AND-WRAP: official driver + datasets + leaderboard
entry per pillar, with a thin TheoDB-owned analysis layer (significance + regression + correctness) retained on top.**

## Coverage Corner 1 — Integration Tests

The load-bearing corner: what does each official benchmark VERIFY, and does it give the two guarantees TheoDB's
bespoke harness has (paired significance; byte-identical regression)?

| Pillar / tool | Reproducibility protocol | Result-correctness oracle? | Paired significance? | Byte-identical regression? |
|---|---|---|---|---|
| **ann-benchmarks** | recall vs exact top-100 ground-truth (HDF5); single-thread enforced; standardized AWS box | **Yes** — recall verified against ground truth | **No** | **No** (stores per-query lists but ships no diff) |
| **VectorDBBench** | recall + max-QPS + latency; explicit concurrency; streaming case | Yes — recall vs ground truth | **No** | **No** |
| **ClickBench** | 3 runs/query (cold=1st, hot=min-of-2 hot), geomean combine, `c6a.4xlarge` | **No** — `check` is a `SELECT 1` liveness probe; timing-only ⇒ a wrong-but-fast engine could top the board | **No** | **No** (results never stored) |
| **TPC-H/TPC-DS (official)** | spec-mandated validation + independent auditor | **Yes** (auditor-enforced) — but audit is paid/heavy | No | No |
| **pgbench / HammerDB TPROC-C** | rampup + steady-state (HammerDB); duration-bounded (pgbench) | **No** — pure load drivers; will post a big TPS/NOPM with `fsync=off`. ACID gate lives ONLY in audited TPC-C (Clause 3: 4 consistency conditions, 7 isolation tests, durability pull-the-plug) | **No** | **No** |
| **CH-benCHmark / BenchBase** | mixed OLTP+OLAP one work-phase; per-txn throughput+latency `summary.json` | **No** — validates timing + completion, not OLAP result values under contention; TPC-C consistency-check execution UNCONFIRMED in BenchBase (present in TiDB `tiup bench ch`) | **No** | **No** |

**Verdict (Q10 + Q11):** the official tools are **timing/throughput leaderboard runners**. Correctness gating
exists only in audited TPC (paid) — ClickBench, pgbench, HammerDB TPROC-C, and BenchBase all measure speed, not
validity. **None** provides paired significance or byte-identical regression. → **RETAIN** a TheoDB-owned analysis
layer: (a) result-correctness / crash-safety checks (our existing `#46/#47` crash harnesses; byte-identical A/B),
and (b) paired significance (`benchmarks/theodb_bench/significance.py`). Sources: ann-benchmarks README + arXiv
1807.05614; VectorDBBench README; ClickBench `lib/benchmark-common.sh` + `postgresql/check`; TPC-C FDRs (tpc.org);
HammerDB ch03s06; BenchBase README (all fetched 2026-07-20).

## Coverage Corner 2 — Dependencies

D1 gate = only Apache-2.0/MIT/BSD/PostgreSQL in DISTRIBUTED artifacts; data merely downloaded for CI eval is the
laxer case; GPL/AGPL/NC barred from the shipped tree.

| Asset | License (source, 2026-07-20) | D1 posture |
|---|---|---|
| **pgbench** | PostgreSQL License (ships in PG core) | ✅ D1-clean — may reference/vendor |
| **GloVe vectors** | PDDL 1.0 (public-domain); code Apache-2.0 (stanfordnlp/GloVe) | ✅ D1-safe to bundle |
| **SIFT1M / GIST1M (TEXMEX)** | **UNCONFIRMED** — `corpus-texmex.irisa.fr` TLS cert-altname mismatch; no explicit OSS license located | ⚠️ MUST-VERIFY before any bundling; treat as download-for-eval only |
| **ClickBench `hits`** | **CC BY-NC-SA 4.0** (repo LICENSE) — NonCommercial + ShareAlike | ⚠️ Non-permissive → **CI-download only** from `datasets.clickhouse.com`, never vendor into the shipped artifact |
| **TPC-H/DS data** | spec PDFs free; `dbgen`/`dsdgen` free-to-use under TPC EULA (redistribution restricted → community kits `tpch-kit`/`pg_tpch`/DBT3 exist); "official TPC-H" branding needs a **paid audit** | Use community kits; label results **"TPC-H-derived,"** never "official TPC-H" |
| **HammerDB** | **GPLv3** (github.com/TPC-Council/HammerDB) | ⚠️ GPL barred from the tree, but OK as an **external out-of-tree driver** (talks over the wire, like GPL `psql`/`gdb`); **never vendor/fork/link** |
| **VectorDBBench Cohere/OpenAI/LAION sets** | Wikipedia/C4/CC lineage, not OSI code licenses | Eval-only; verify per-file; do not bundle |
| **BenchBase toolchain** | Apache-2.0 code, but **Java 23** (non-LTS) + no release tags (`2023-SNAPSHOT`) | Pin a git SHA for reproducibility; Java 23 is a build liability |

## Coverage Corner 3 — Tools

**Vector — ann-benchmarks entry (also reused by big-ann):** a `Dockerfile` + a `module.py` subclassing `BaseANN`
(`fit(X)` build, `query(v,n)` search, optional `batch_query`/`set_query_arguments`), a `config.yml` param grid, CI
wiring, and a PR to `erikbern/ann-benchmarks`; the runner writes per-(algo,params) HDF5 → recall×QPS. VectorDBBench
already ships `pgvector`/`pgvectorscale`/`pgdiskann`/`alloydb` drivers to copy. A `psycopg` wrapper around TheoDB's
SQL surface is sufficient.

**Columnar — ClickBench entry:** copy the `postgresql/` directory (`create.sql`, `queries.sql`, glue hooks
`install`/`start`/`stop`/`check`/`load`/`query`/`data-size`, `template.json`), run on the canonical `c6a.4xlarge`
(writes `results/*.json` of raw `[t1,t2,t3]` triples), open a PR; CI regenerates the leaderboard. No result-verify
gate at submission.

**OLTP — pgbench + HammerDB:**
```bash
pgbench -i -s 50 theodb                       # build
pgbench -c 32 -j 8 -T 600 -r -P 10 theodb     # TPC-B-like, TPS
```
```tcl
# HammerDB TPROC-C (docker: tpcorg/hammerdb): dbset db pg; dbset bm TPC-C
diset tpcc pg_count_ware 20; diset tpcc pg_num_vu 4; buildschema
diset tpcc pg_driver timed; diset tpcc pg_rampup 2; diset tpcc pg_duration 5
loadscript; vuset vu 4; vucreate; vurun   # → NOPM (claim-grade) + TPM
```

**HTAP — BenchBase CH-benCHmark (Java 23 + Maven):**
```bash
./mvnw clean package -P postgres
java -jar benchbase.jar -b tpcc,chbenchmark -c config/postgres/sample_chbenchmark_config.xml \
  --create=true --load=true --execute=true   # TPC-C mix 45/43/4/4/4 + Q1..Q22 in one phase
```
BenchBase emits per-txn throughput + latency percentiles in `summary.json`; the canonical tpmC/QphH dual metric
must be **derived** by post-processing (TPC-C txns vs Q1–Q22). Seed-level deterministic replay is unconfirmed.

## Coverage Corner 4 — Techniques

- **Vector:** no single canonical — **VectorDBBench** is field-canonical for a *PG-compatible engine* (the only tool
  co-locating pgvector/pgvectorscale/**AlloyDB** on one axis; recall + max-QPS + explicit concurrency + a streaming
  case); **ann-benchmarks** is canonical for the *algorithm* recall×QPS Pareto (single-thread, SIFT/GIST/GloVe/
  deep-image, recall@10) and is where a new engine earns public credibility; **big-ann-benchmarks** is canonical at
  billion-scale (NeurIPS, recall@10-vs-throughput at a fixed target QPS).
- **Columnar/OLAP:** **ClickBench** (43 queries over one 100 GB `hits` table; 3 runs cold=1st/hot=min-of-hot;
  geomean; single-node) as the fast scan/filter/aggregate standard + **TPC-H-derived** (tpch-kit/DBT3/HammerDB
  TPROC-H) for join-heavy generality.
- **OLTP:** **HammerDB TPROC-C** as the claim-grade tool (real TPC-C mix 45/43/4/4/4, reported as **NOPM**;
  cannot be called "tpmC"/"TPC-C" — audit-gated); **pgbench** as the ubiquitous TPC-B-like smoke/tuning tool (TPS).
- **HTAP:** **CH-benCHmark** (TPC-C + 22 TPC-H queries on one schema; DBTest 2011) via **BenchBase** (cmu-db, the
  OLTP-Bench successor; supports PostgreSQL).

**Honest positioning (Q5) — the ScaNN/AlloyDB gap:** a standard vector benchmark REPRODUCES the *direction* of the
gap — Google's blog (2025-03-11) shows ScaNN-for-AlloyDB at **431 ms vs pgvector HNSW > 4 s** on BigANN-1B
(out-of-RAM) + 4× less memory + up to 60× lower build cost — but publishes **latency, not recall-matched QPS**, and
**no neutral source publishes the ~25–44×@0.99 QPS magnitude** TheoDB measured (M33/M73). Any claim of that
magnitude MUST cite TheoDB's own `docs/benchmarks/m73-headtohead-verdict.md`, not a public leaderboard. pgvectorscale
vendor numbers (471 QPS @99% recall @50M vs Qdrant 41) are first-party, not third-party-reproduced (public-copy §4).
→ adopting a standard benchmark strengthens the honest "recall-parity + billion-scale + open" story precisely by
making our numbers third-party-reproducible; it will not manufacture a QPS win we do not have.

## Cross-cutting Comparison

| Pillar | Canonical tool | Metric | Entry / leaderboard | Dataset license (D1) | Correctness oracle | Sig. + regression |
|---|---|---|---|---|---|---|
| Vector | VectorDBBench (+ ann-benchmarks for public Pareto) | recall × max-QPS | copy driver / BaseANN + PR | GloVe ✅; SIFT/GIST ⚠️verify; Cohere/OpenAI eval-only | recall vs ground-truth ✅ | **neither** → retain |
| Columnar | ClickBench (+ TPC-H-derived) | cold/hot geomean | copy `postgresql/` + PR | hits CC-BY-NC-SA → CI-only ⚠️ | **none** (SELECT 1) | **neither** → retain |
| OLTP | HammerDB TPROC-C (+ pgbench) | NOPM (+ TPS) | out-of-tree driver | pgbench ✅; HammerDB GPLv3 external-only ⚠️ | **none** (audited TPC-C only) | **neither** → retain |
| HTAP | CH-benCHmark / BenchBase | tpmC/QphH (derived) | pin SHA; Java 23 | Apache code; data self-gen | timing+completion only | **neither** → retain |

The single most important cross-cutting fact: **the "significance + byte-identical regression" column is
"neither → retain" in all four rows.** That is the evidence that reopens the replace-vs-augment decision.

## ADRs

### D1 — ADOPT-AND-WRAP, not a pure replace (the load-bearing decision)

**Decision:** adopt the official benchmark **driver + datasets + leaderboard entry** per pillar for external
comparability, but **retain a thin TheoDB-owned analysis layer**: (a) paired significance
(`benchmarks/theodb_bench/significance.py`), (b) byte-identical / cross-version regression A/B, (c) result-
correctness + crash-safety gating. **Rationale:** all four pillars' research independently found the official tools
provide NONE of (a)/(b)/(c) — they are timing leaderboard runners. A pure replace would DROP capabilities shipped in
M123/M125 (significance) and M114/M126 (byte-identical A/B) and the `#46/#47` crash gates. **Alternatives rejected:**
(i) **pure replace** (the owner's initial choice) — REJECTED by unanimous evidence: drops significance + regression +
correctness, none of which the official tools provide; (ii) **keep bespoke only** — REJECTED: no third-party
reproducibility, the exact gap `public-copy.md § 4` flags. **This ADR contradicts the owner's "substituir" decision
and MUST be surfaced for re-decision (Rule 3).**

### D2 — Per-pillar canonical selection

**Decision:** Vector = VectorDBBench (PG-compat comparability) + ann-benchmarks (public algorithm Pareto);
Columnar = ClickBench (+ TPC-H-derived later); OLTP = HammerDB TPROC-C (+ pgbench smoke); HTAP = CH-benCHmark via
BenchBase. **Rationale:** these are the tools the field actually reports on for a PG-compatible engine (Corner 4).
**Alternative rejected:** big-ann-benchmarks as the primary vector tool — deferred (billion-scale only; not the
credibility surface for a first entry).

### D3 — License handling under D1

**Decision:** ClickBench `hits` (CC-BY-NC-SA) and TEXMEX SIFT/GIST (unconfirmed) are **CI-download-only, never
vendored**; HammerDB (GPLv3) runs as an **external out-of-tree driver, never forked/linked**; TPC results are labeled
**"TPC-H-derived,"** never "official TPC-H"; only GloVe (PDDL) + pgbench (PG License) may be bundled. **Rationale:**
the D1 gate bars NC/GPL from the distributed tree but the laxer eval-download case + external-driver pattern keep
usage clean. **Alternative rejected:** vendoring datasets for offline CI — REJECTED (D1 violation for hits/HammerDB).

### D4 — Honest positioning of the vector gap

**Decision:** any statement of the ScaNN/AlloyDB QPS-gap magnitude cites TheoDB's own
`docs/benchmarks/m73-headtohead-verdict.md`; public sources are cited only for the gap's *direction*. **Rationale:**
no neutral source publishes the 25–44×@0.99 QPS figure (Corner 4). **Alternative rejected:** citing the Google blog
for a QPS magnitude — REJECTED (it publishes latency, not recall-matched QPS).

## Recommendations

1. **Re-decide replace-vs-augment WITH this evidence (owner call).** The unanimous finding is that a pure replace
   drops significance + regression + correctness. Recommended: **adopt-and-wrap** (ADR-D1).
2. **Roadmap program (one milestone per pillar), each = adopt the official driver + retain the wrap layer:**
   - **M127 Vector** — a VectorDBBench/ann-benchmarks `BaseANN` `psycopg` entry for TheoDB; target a public
     ann-benchmarks Pareto submission; retain significance on the per-query HDF5.
   - **M128 Columnar** — a ClickBench `postgresql/`-style entry for TheoDB (create/queries/glue + results JSON);
     add TPC-H-derived (tpch-kit/DBT3) later; retain byte-identical result A/B (ClickBench has no result oracle).
   - **M129 OLTP** — HammerDB TPROC-C (NOPM) + pgbench (TPS) as external drivers; retain the ACID/crash-safety gate
     (`#46/#47`) alongside every throughput number.
   - **M130 HTAP** — BenchBase CH-benCHmark against PG17 (pin a SHA; Java 23); derive tpmC/QphH; retain OLAP result
     validation + significance.
3. **Retire the ~40 bespoke comparative `run_m*.py`** once each pillar's official entry lands — but keep
   `theodb_bench/significance.py` + the byte-identical A/B harness as the retained wrap layer (they are the
   capabilities the official tools lack).
4. **Open MUST-VERIFY:** the TEXMEX SIFT/GIST license (page down) before any dataset bundling.

## Sources (primary, fetched 2026-07-20)

Vector: [ann-benchmarks](https://github.com/erikbern/ann-benchmarks) · [ANN-Benchmarks paper arXiv:1807.05614](https://arxiv.org/pdf/1807.05614) · [VectorDBBench](https://github.com/zilliztech/VectorDBBench) · [big-ann-benchmarks](https://github.com/harsha-simhadri/big-ann-benchmarks) · [ScaNN-for-AlloyDB vs pgvector (Google, 2025-03-11)](https://cloud.google.com/blog/products/databases/how-scann-for-alloydb-vector-search-compares-to-pgvector-hnsw) · [ScaNN for AlloyDB whitepaper](https://services.google.com/fh/files/misc/scann_for_alloydb_whitepaper.pdf) · [pgvectorscale vs Qdrant/Pinecone (TigerData)](https://www.tigerdata.com/blog/pgvector-is-now-as-fast-as-pinecone-at-75-less-cost) · [GloVe (PDDL)](https://github.com/stanfordnlp/GloVe).
Columnar: [ClickBench](https://github.com/ClickHouse/ClickBench) · [ClickBench README](https://raw.githubusercontent.com/ClickHouse/ClickBench/main/README.md) · [ClickBench LICENSE (CC-BY-NC-SA)](https://raw.githubusercontent.com/ClickHouse/ClickBench/main/LICENSE) · [leaderboard](https://benchmark.clickhouse.com/) · [TPC-H spec](https://www.tpc.org/tpc_documents_current_versions/pdf/tpc-h_v3.0.1.pdf) · [tpch-kit](https://github.com/gregrahn/tpch-kit) · [DBT3 on PG](https://www.postgresql.fastware.com/pzone/2025-01-running-the-tpc-h-benchmark-using-dbt3).
OLTP: [PG17 pgbench](https://www.postgresql.org/docs/17/pgbench.html) · [HammerDB TPROC-C mix](https://www.hammerdb.com/docs/ch03s05.html) · [tpmC vs NOPM](https://www.hammerdb.com/blog/uncategorized/how-to-understand-tpc-c-tpmc-and-tproc-c-nopm-and-what-is-good-performance/) · [HammerDB GPLv3](https://github.com/TPC-Council/HammerDB/blob/master/LICENSE) · [TPC-C FDRs](https://www.tpc.org/results/fdr/tpcc/).
HTAP: [CH-benCHmark (DBTest 2011)](https://dl.acm.org/doi/10.1145/1988842.1988850) · [TUM project](https://db.in.tum.de/research/projects/CHbenCHmark/?lang=en) · [BenchBase](https://github.com/cmu-db/benchbase) · [BenchBase chbenchmark config](https://raw.githubusercontent.com/cmu-db/benchbase/main/config/postgres/sample_chbenchmark_config.xml) · [CedarDB CH-benCHmark doc](https://cedardb.com/docs/example_datasets/chbenchmark/).
