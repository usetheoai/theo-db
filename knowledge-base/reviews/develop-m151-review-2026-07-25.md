# Review — M151 DataFusion coverage / `<>` + cross-type (develop)

**Data:** 2026-07-25
**Verdict:** READY_TO_MERGE

## Método

2 passes do council-rust-pgrx (correção FFI/pgrx + semântica de operador), gated por A/B empírico (`run_m128
--agg`, 43 queries reais do ClickBench, diverged=0) + harness focado de regressão.

## Vereditos e findings

| Pass | Achado | Resolução |
|---|---|---|
| council-rust-pgrx (detecção `<>` por negador) | **LIMPO** (nenhum BLOCKER/HIGH). Provou via `pg_operator.c` que a detecção por negador é sound (PG bloqueia duplo-negador → `get_negator(op)==` implica `op` é o `<>` canônico). 2 findings de robustez. | `#[repr(i32)]` + discriminantes explícitos em `ZoneOp` (round-trip encode/decode estável) + doc corrigido. |
| council-rust-pgrx (coerção cross-type) | **1 HIGH + 1 MEDIUM + 1 LOW.** HIGH: o relaxamento `var_side==vartype` admitia temporal cross-type (`date`=dias vs `timestamp`=μs; timezone) e float cross-type (`f4=x::float8`) → coerção por bits crus dá resultado errado (poda + `build_filter_expr` comparam bits crus). O A/B diverged=0 NÃO pegou (ClickBench-agg não usa esses shapes). | **Fix:** cross-type restrito à classe inteira `{int2,int4,int8}` (widening isomórfico de ordem); temporal/float declina → nativo. **Provado** por harness de regressão: `ts<DATE` e `f4=1.1::float8` → `Seq Scan` (nativo) + A/B byte-idêntico (incl. sob `TimeZone` não-UTC). LOW doc corrigido. |

## Evidência

- **Cobertura medida:** `run_m128 --agg --n 100000` → `columnar_customscan_count = 14` (era 6 pré-M149), `result_ab.diverged = 0`, 43/43 pass. Atribuição honesta: M149 (projeção) 6→11; **M151 (int cross-type) 11→14** (q1/q7/q41). `docs/benchmarks/m151-datafusion-coverage.md`.
- **Fix HIGH provado:** harness `m151_validate.sql` — int cross-type (`s<>0`) roteia ao `theodb_columnar_agg`; temporal/float cross-type (`ts<DATE`, `f4=float8`) declina (`Seq Scan`); A/B byte-idêntico em tudo (incl. timezone). M151_VALIDATE_OK.
- **Same-type intocado:** `encode_const_coerced` é byte-idêntico a `encode_const_bits` para `consttype==vartype` (provado item-a-item pelo council).

## Limitação honesta (documentada, não bloqueia)

`<>` em texto (`SearchPhrase <> ''`) é honest-negative (ADR-4: const-texto não cabe na serialização `custom_private`
`lappend_int`) — follow-up bem-especificado. Temporal/float cross-type declina ao nativo (correto).

## Decisão

Nenhum finding aberto após os fixes. HIGH temporal/float corrigido e provado; robustez fechada. Cobertura 6→14
medida, diverged=0. DoD do ROADMAP atendido (cobertura sobe de 6, A/B byte-idêntico, número real). **READY_TO_MERGE.**
