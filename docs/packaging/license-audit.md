# License audit — TheoDB core package (M1 DoD-3)

**Result: ZERO AGPL in the distribution.** Reproducible via `bash packaging/license-sweep.sh` (exits non-zero
on any real AGPL/Affero finding). This is the committed evidence for the DoD-3 release gate (PRD §11 — D1).

## (a) System packages (apt) in the image

Scan of `/usr/share/doc/*/copyright` for `Affero|AGPL` → **only `ca-certificates`**, a **false positive**:
its copyright text is GPL-2+/MPL-2.0 and the match is in the MPL tri-license prose that merely *enumerates*
the AGPL by name. No AGPL-licensed apt package.

## (b) pgvectorscale Rust crate tree (statically linked into `vectorscale.so`)

`cargo metadata` over the pinned commit `57c88b7b4fe40a2afa20b195f60047a983279c19` (the same ref the image
builds): **293 crates, 0 AGPL/Affero.** Full distribution:

```
152 MIT OR Apache-2.0
 50 MIT
 19 Apache-2.0 OR MIT
 18 Unicode-3.0
 16 MIT/Apache-2.0
  6 Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT
  5 Apache-2.0
  4 Unlicense OR MIT
  3 Zlib OR Apache-2.0 OR MIT
  2 Apache-2.0/MIT
  2 ISC
  2 workspace-crate (PostgreSQL License)   # vectorscale, pgvectorscale_derive (the project itself)
  2 MIT OR Apache-2.0 OR LGPL-2.1-or-later
  2 Unlicense/MIT
  2 Apache-2.0 OR BSL-1.0 OR MIT
  2 BSD-2-Clause OR Apache-2.0 OR MIT
  1 BSD-3-Clause
  1 (Apache-2.0 OR MIT) AND BSD-3-Clause
  1 Apache-2.0 / MIT
  1 Zlib
  1 MIT OR Apache-2.0 OR Zlib
  1 (MIT OR Apache-2.0) AND Unicode-3.0
```

All permissive (MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode-3.0 / Unlicense / BSL-1.0 / LGPL-2.1-as-option).
The two "workspace-crate" entries are pgvectorscale's own crates under the project's PostgreSQL License.

## (c) Extensions / PL

- `pgvector` — pure C, PostgreSQL License (no Rust tree); C/system deps covered by (a).
- `pgvectorscale` — PostgreSQL License; Rust deps covered by (b).
- `plpython3u` — PostgreSQL License; embeds libpython (PSF, permissive); system deps covered by (a).

## Tool note (DoD-3 — `loop-check-licence`)

The ROADMAP DoD-3 names `loop-check-licence`. We implement the gate with a **deterministic, reproducible,
committed** sweep (`packaging/license-sweep.sh`: apt copyright scan + `cargo metadata` AGPL check) instead of
the `loop-check-licence` multi-agent plugin. Rationale: the gate's question is binary ("any AGPL in what we
ship?"), and a deterministic script over (a) the image's apt packages and (b) the exact pinned crate tree is
re-runnable in CI and produces a stable, auditable artifact — stronger as a release gate than a non-pinned
LLM audit. `loop-check-licence` remains available for deeper periodic provenance/similarity audits. (This is
the ADR-recorded deviation referenced in the M1 plan.)

## (d) BM25 candidates (M7-S2) — permissive-vs-AGPL identification

Reproducible via `bash packaging/license-sweep.sh` § (c) — each verdict is fetched from the candidate's
**canonical repo** (never assumed from memory; unfetchable → `UNVERIFIED`). These pieces are **not** in the
shipped distribution image (so they do not affect the distribution AGPL gate); this records the M7-S2
identification (ADR 0003) as re-runnable evidence.

| Candidate | License (verbatim source) | BM25 true? | Verdict |
|---|---|---|---|
| `timescale/pg_textsearch` v1.3.1 | **PostgreSQL License** (`raw.githubusercontent.com/timescale/pg_textsearch/v1.3.1/LICENSE`) | Yes (Okapi, k1=1.2/b=0.75, Block-Max WAND) | **PERMISSIVE — identified** |
| `tensorchord/VectorChord-bm25` | **AGPLv3 / Elastic License v2** (dual; `raw.githubusercontent.com/tensorchord/VectorChord-bm25/main/LICENSE`) | Yes | **BARRED (D1)** — neither license permissive |
| `paradedb/pg_search` | **AGPL-3.0** (`.claude/knowledge-base/references/paradedb/LICENSE`) | Yes | **BARRED (D1)** — study-only witness |
| PostgreSQL native `ts_rank_cd` | PostgreSQL License (built-in) | No (cover-density, not BM25) | permissive but not BM25 — interim default (M7-S1) |
| `Intelligent-Internet/psql_bm25s` | Apache-2.0 | Yes | permissive — fallback |

**Net:** `pg_textsearch` is the identified permissive BM25 alternative (ADR 0003); adoption into the shipped
image is gated on the recall benchmark `docs/benchmarks/m7-bm25-vs-tsrank.md` (measurement-first, ADR 0002).

## (e) Columnar candidates (M6) — permissive verification

Reproducible via `bash packaging/license-sweep.sh` § (e). These pieces are **not** in the shipped distribution
image (measurement-first gate — M6); this records the M6 columnar identification as re-runnable evidence.

| Candidate | License (verbatim source) | Verdict |
|---|---|---|
| `Mooncake-Labs/pg_mooncake` | **MIT** (`raw.githubusercontent.com/Mooncake-Labs/pg_mooncake/main/LICENSE`) | **PERMISSIVE** |
| `duckdb/pg_duckdb` | **MIT** (MIT body; `raw.githubusercontent.com/duckdb/pg_duckdb/main/LICENSE`) | **PERMISSIVE** |

Net: the columnar/HTAP stack (pg_mooncake + pg_duckdb + DuckDB) is permissive (MIT), D1-clean. Adoption into
the shipped image is gated on the PG17 build + a large-scale measurement (see `docs/benchmarks/m6-columnar-vs-row.md`).
