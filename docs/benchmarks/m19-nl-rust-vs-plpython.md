# M19 benchmark — `ai.nl_to_sql` Rust vs plpython3u

**Verdict: NO-REGRESSION** (rust/plpy = 0.922, bar 1.20).

## What this measures

`ai.nl_to_sql` was ported from `plpython3u` to Rust (theodb_rs, M19). This is a head-to-head of the Rust
implementation against the **exact retired plpython3u body** (git `6c1dddb:sql/60-theodb-nl.sql`, renamed
`ai.nl_to_sql_plpy`). Both call the **same** Rust `ai._chat` against the **same** deterministic chat stub
(`tools/chat_server.py`), so the LLM round-trip is held constant and the measured delta isolates the
**validation glue**: Rust stdlib token-scan + `Spi` EXPLAIN vs plpython3u `re` + `plpy` EXPLAIN.

`ai.nl_to_sql` end-to-end latency is dominated by the model call (I/O-bound) — like per-row `embed`, the
honest expectation is parity, and the gate is **no-regression**, not a speedup claim.

## Results

| Implementation | mean (ms) | ± std (run means) | p95 (ms) |
|---|---|---|---|
| Rust `ai.nl_to_sql` | 0.993 | 0.125 | 1.278 |
| plpython3u `ai.nl_to_sql_plpy` | 1.078 | 0.171 | 1.509 |

- Samples: 5 runs × 20 calls = 100 per implementation (warmup excluded).
- Ratio Rust/plpython3u (mean of run means): **0.922**.
- Host: Linux-6.8.0-124-generic-x86_64-with-glibc2.35; Python 3.10.12.

## Methodology

1. Build the M19 image (`docker build -t theo-db:m19 .`) and start it with `--add-host=host.docker.internal:host-gateway`.
2. A throwaway DB installs `theodb_rs` (Rust `ai.nl_to_sql` + `ai._chat`) + `plpython3u` + the baseline `ai.nl_to_sql_plpy`.
3. `theodb.llm_endpoint` points at the local `tools/chat_server.py` stub (deterministic benign SELECT).
4. Warmup 5 calls/impl (excluded); then 5 runs × 20 sequential calls/impl, timed client-side
   (`time.perf_counter` around `SELECT ai.<fn>(question, allowed)`), same question + allowlist for both.
5. Report mean ± population-std of per-run means + p95 over all samples.

Reproduce: `PGHOST=localhost PGPORT=55432 PGUSER=postgres PGPASSWORD=postgres python3 benchmarks/bench_nl.py --write-doc`.

## Honesty notes

- The stub removes real-network variance so the comparison is repeatable; against a real model both impls pay
  the same (much larger) RTT, so the validation-glue delta shrinks to noise in production.
- The plpython3u baseline is reconstructed from git for measurement only — it is **not** shipped (the extension
  is 100% plpython3u-free since M19).
