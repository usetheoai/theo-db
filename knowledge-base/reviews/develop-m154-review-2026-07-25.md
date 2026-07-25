# Review — M154 (COUNT DISTINCT → CustomScan colunar)

**Data:** 2026-07-25 · **Branch:** develop · **Commits:** b839876 (feat) + fbfcebe (review remediation)
**Councils:** council-rust-pgrx, council-benchmark (paralelos, leram o código/artefatos reais).

## Verdict: READY_TO_MERGE

Sem BLOCKER. 1 HIGH — **corrigido e provado** (não apenas mitigado). Todas as ressalvas de benchmark fechadas.

## Achados e resolução

### HIGH (rust-pgrx) — `count(DISTINCT float4/float8)` divergia do PG — CORRIGIDO
- **Defeito:** o `FloatDistinctCountAccumulator` do DataFusion dedup por total-order IEEE (`-0.0 ≠ +0.0`; NaN
  bit-patterns distintos); o `float8eq` do PG trata `0.0 = -0.0` e todo NaN igual → `count(DISTINCT {0.0,-0.0})`
  daria 2 (roteado) vs 1 (PG). Classe de shape que o A/B do ClickBench não exercita (lição M151).
- **Fix (fbfcebe):** declinar `FLOAT4OID/FLOAT8OID` no branch kind-8 (ADR-M154-4) — restringir à classe
  provadamente-segura (int/text-determinístico). **Provado:** EC-5 A/B `COUNT(DISTINCT float8)` = 3 = 3 (declina;
  sem o guard seria 5). Bônus: adotado `inputcollid` (a collation que o PG usa para a igualdade do DISTINCT) no
  guard de determinismo — defesa em profundidade que o council recomendou.
- Demais perguntas rust-pgrx: **LIMPO com prova** — FFI `get_collation_isdeterministic` é leitura de syscache
  segura (não é panic novo atravessando C); guard de collation sem escape (CollateExpr/cast/domain/citext todos
  declinam); serialização kind-8 round-trip correto e inalcançável pelo `unreachable!()` do fast-path min/max;
  multi-arg/expr/não-distinct/sum-distinct todos declinam; sem UB/dangling; accumulator EXATO (não HLL).

### Ressalvas (benchmark) — HONESTO COM RESSALVAS → todas fechadas
- **MÉDIO-1 (sample head p/ milestone de correção + gap de alta-cardinalidade):** fechado — rodado systematic 300k
  com `UserID`≈290k distintos + `work_mem=256MB` → **18/43, diverged=0, byte-idêntico** (`m154_agg_systematic_wm256.json`);
  q4 direto 290.874 = 290.874, 49,6ms < 147,8ms nativo. O "erro" a work_mem default (4MB) é o contrato
  bounded-work_mem D3 do M100 (governa TODO agg colunar), não regressão do M154 — documentado no doc.
- **MÉDIO-2 (ec_harness não commitado):** fechado — `benchmarks/m154_ec_harness.sql` commitado.
- **BAIXO-MÉDIO-3 (EC-1 "4" raciocinado, não medido):** fechado — contrafactual guard-off medido:
  `COUNT(DISTINCT s COLLATE "C")` = 4 ≠ `COUNT(DISTINCT s)` = 2 (`m154_ec_guards.txt`).
- **INFO-4 (18 mistura projeção+agg):** fechado — doc divulga 18 = 13 agg + 5 projeção.
- **Manchetes auditadas HONESTAS:** +4 todas-COUNT(DISTINCT) verificável no JSON; diverged=0 oráculo legítimo
  (count_distinct exato, columnar vs heap, canonicalização order-insensitive); +4 vs previsão ~2 não é cherry-pick
  (q10/q11/q13 reportados honestamente como não-roteados).

## Hard gates (cycle-review)
- Sem testes falhando (A/B diverged=0 nos dois regimes). Sem secrets. Sem commit em main. Sem trailer Co-Authored-By.
  CHANGELOG `[Unreleased]` atualizado. ✓

## Evidência
- `docs/benchmarks/m154-count-distinct.md` + `docs/benchmarks/m154-artifacts/{m154_agg.json, m154_agg_systematic_wm256.json, m154_ec_guards.txt}`
- `benchmarks/m154_ec_harness.sql` (reprodutível)
