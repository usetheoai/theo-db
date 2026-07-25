# Review — M150 chunk-group filtering (develop)

**Data:** 2026-07-25
**Verdict:** READY_TO_MERGE

## Método

Review de correção por 2 councils independentes (os que pegaram os bugs subtis do M148/M149), focados na
classe de bug que o A/B não detecta: o novo canal `SCAN_PREDICATES`, a limpeza compartilhada, o skip.

## Vereditos

| Council | Verdict | Achado |
|---|---|---|
| council-rust-pgrx | **LIMPO** (nenhum BLOCKER/HIGH) | Canal `SCAN_PREDICATES` ABA-proof por construção (insert incondicional, mais robusto que o remove-on-None do M149); EXPLAIN_ONLY não insere/lê; exec lê preds só após scandesc set; `predicates_needed` não dá panic; interação M149+M150 simétrica; isolamento por scandesc. |
| council-index-storage | **LIMPO** (nenhum BLOCKER/HIGH) | **Prova matemática** (6 invariantes) de que é impossível pular um chunk-group com match: min/max limita todos os valores presentes; `excluded()` é o contrapositivo correto; qual conjuntiva + ExecScan re-checa; domínio de bits casa; `minmax_kind_of` só admite tipos order-preserving; fail-safe triplo. Caso adversarial NaN/+inf construído e refutado. |

## Findings fechados (2 LOW)

- **LOW (ambos):** `enable_chunk_skip=on` é no-op quando `enable_projection=off` → nota de dependência adicionada ao doc do GUC (commit 32cd101) + CHANGELOG.
- **LOW/MEDIUM (rust-pgrx test-gap):** isolamento de predicados entre duas tabelas colunares → teste `test_two_table_predicate_isolation` (Rust) + assertion no harness SQL (commit 32cd101). **Provado no droplet:** `OK two-table predicate isolation (pair 25000,35000)`.

## Evidência empírica (prova RED→GREEN, commit 32cd101)

7 testes + two-table isolation provados no droplet (build release, commit exato do release):
T3.1 skip=4/5 A/B=1; never_loses_row (6 preds); T4.1 GUC gate off=0/on=4 (ablação same-binary); T2.2 best-effort
OR-noskip=0/AND-skip=4; T2.1 subxact-abort no stale predicate; two-table isolation. Benchmark 1M: skip 99%,
ganho ~52-90× (geomean 68×), A/B diverged=0 (`docs/benchmarks/m150-chunk-group-filtering.md`).

## Decisão

Nenhum BLOCKER, nenhum HIGH. 2 LOW fechados e provados. DoD do ROADMAP excedido (skip≥80%→99%, ganho≥5×→68×,
A/B diverged=0). **READY_TO_MERGE.**
