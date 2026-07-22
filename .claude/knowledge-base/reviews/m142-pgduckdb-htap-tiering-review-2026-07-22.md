# Review — M142 (tier-out do pg_duckdb) — 2026-07-22

**Verdict:** READY_TO_MERGE

4 revisores adversariais em paralelo (index-storage/upgrade-chain, security, benchmark, cross-validation).
Veredito consolidado: **0 BLOCKER, 0 HIGH, 2 MEDIUM + LOWs — TODOS corrigidos e re-validados no binário/imagem
shipada.**

## Hard gates (cycle-review.md) — todos ✅

branch=develop · sem `Co-Authored-By` · sem secrets · CHANGELOG atualizado (Added + Changed/BREAKING + Fixed).

## Revisores e vereditos

| Revisor | Lente | Veredito | Achados |
|---|---|---|---|
| council-index-storage | extensão PG + cadeia de upgrade M137 | PASS | convergência fresh/upgrade provada, sem version skew; 1 LOW (to_regproc overload) + 1 INFO (COMMENT no delta) |
| council-security | superfície de ataque + fail-closed | PASS | tier-out genuíno, ca-certificates preservado, guard injection-safe; 1 **MEDIUM** (allow_community gate órfão) + 1 LOW |
| council-benchmark | "mediu ou supôs?" | PASS | 175MB medido, smokes reais, gates fail-closed; 1 **MEDIUM** M1 (proveniência do .so) + 2 LOW |
| cross-validation | plano↔impl↔diff | READY_TO_MERGE | 7/7 coverage, tier-out completo, fixes justificados; 3 LOW |

## Achados e disposição (todos corrigidos)

| Sev | Finding | Disposição |
|---|---|---|
| **MEDIUM** (security) | `duckdb.allow_community_extensions=off` perdeu a verificação automatizada no tier-out (o m61-smoke que a checava ficou órfão) | **CORRIGIDO** — `Dockerfile.htap` seta `off` explícito; smoke `m142-tiering-validate.sh` + job CI `htap-image` asserem `off`. Re-validado. |
| **MEDIUM** (benchmark M1) | tamanho do `pg_duckdb.so` no doc constava como "medido" mas não saía do harness | **CORRIGIDO** — o harness mede via `stat -c%s` (118 MB / 124.213.040 bytes) e emite no bloco medido; doc atualizado. |
| LOW (security+index) | guard dependia de `to_regproc('duckdb.query')` (falso-positivo se overload futuro) | **CORRIGIDO** — guard keyed em `pg_extension WHERE extname='pg_duckdb'` (imune a overload; o check correto). sql/85 + delta 1.4→1.5 + test. Re-validado. |
| LOW (benchmark L1) | smoke só asseria `a\|2\|15`, não a `b\|1\|5` citada | **CORRIGIDO** — asserção `^b\|1\|5` adicionada. |
| LOW (benchmark L2) | grep do shared_preload mascarava falha do psql (`&& fail \|\| true`) | **CORRIGIDO** — captura-depois-assere. |
| LOW (cross-val) | upgrade script + control não estavam na tabela baseline do plano | **CORRIGIDO** — linhas adicionadas ao plano. |
| INFO (index) | delta 1.4→1.5 não re-aplica os `COMMENT ON FUNCTION` (cosmético, mesmo padrão do 1.3→1.4) | Aceito (consistente com a disciplina existente; não-comportamental). |

## Validação pós-fix (imagens shipadas, e2e-runner PG18.4)

`scripts/m142-tiering-validate.sh` (rebuild cacheado) → **DEFAULT_OK** (guard via pg_extension, pg_duckdb ausente,
theodb_rs/`vector`/theodb_columnar verdes) + **HTAP_OK** (pg_duckdb presente, `allow_community_extensions=off`,
M62 e2e `a\|2\|15`+`b\|1\|5`) + **delta 175 MB** + **M142_TIERING_OK**. Evidência: `docs/benchmarks/m142-pgduckdb-tiering.md`.

## DoD do milestone (ROADMAP M142) — verificação

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | Default builda sem pg_duckdb, delta ≥150MB | ✅ | 712 MB, delta 175 MB medido |
| 2 | Smoke default: pg_duckdb ausente + core intacto | ✅ | DEFAULT_OK |
| 3 | Dockerfile.htap + M62 e2e | ✅ | HTAP_OK |
| 4 | guard condicional (sql/85 + CREATE EXTENSION só htap) | ✅ | guard pg_extension (0A000) + initdb 01 só htap |
| 5 | ADR-0056 + README + CHANGELOG | ✅ | commits 4f78224, f3cbee2 |
| 6 | CI builda as 2 imagens | ✅ | job htap-image + asserção default |

## Conclusão

Merge-ready. Os 2 MEDIUM (trava de segurança órfã + proveniência do .so) e os LOWs foram corrigidos e
re-validados na imagem shipada — não deferidos. O tier-out é completo e medido: default 175 MB menor, sem o único
componente C++/httpfs; lakehouse opt-in via `theodb-htap`; guard fail-closed robusto; cadeia de upgrade M137
intacta. **Gate M142 → READY_TO_MERGE.**
