# Review — M143 (remoção total do pg_duckdb) — 2026-07-22

**Verdict:** READY_TO_MERGE

4 revisores adversariais em paralelo. Veredito consolidado: **0 BLOCKER; 2 HIGH + achados MEDIUM/LOW — TODOS
corrigidos e re-validados na imagem shipada.**

## Hard gates (cycle-review.md) — todos ✅

branch=develop · sem `Co-Authored-By` · sem secrets · CHANGELOG atualizado (Added + Changed/BREAKING + Removed + Security).

## Revisores e vereditos (inicial → pós-fix)

| Revisor | Lente | Veredito inicial | Findings |
|---|---|---|---|
| council-rust-pgrx | Rust/pgrx safety do `parquet.rs` | NEEDS_FIXES | 2 HIGH + 2 MEDIUM + 1 LOW |
| council-index-storage | extensão PG + upgrade M137 | PASS (chain) + 1 HIGH (segurança adjacente) | convergência fresh==in-place provada; HIGH ACL |
| council-benchmark | "mediu ou supôs?" | PASS | 2 MEDIUM (rastreabilidade/ablação) + 3 LOW |
| cross-validation | plano↔impl↔diff | READY_TO_MERGE | 8/8 coverage; 1 MEDIUM + 2 LOW |

## Achados e disposição (todos corrigidos + re-validados)

| Sev | Finding | Disposição |
|---|---|---|
| **HIGH** (rust-pgrx + index-storage) | `public.read_parquet`/`write_parquet`/`olap` EXECUTE p/ PUBLIC → escrita/leitura de arquivo server-side arbitrária por qualquer role (contorna o least-privilege do sql/85) | **CORRIGIDO** — `extension_sql!` `REVOKE ALL FROM PUBLIC` no `parquet.rs`. **Re-validado:** gate `REVOKE_OK` (lowpriv BLOQUEADO em read/write_parquet). |
| **HIGH** (rust-pgrx) | `block_on` sem `HeldInterrupts` → longjmp do PG salta o runtime tokio sem Drop | **CORRIGIDO** — `with_runtime` envolve o `block_on` com `HeldInterrupts` (mesma invariante do df_executor). |
| MEDIUM (rust-pgrx) | leitura materializa tudo sem `GreedyMemoryPool(work_mem)` → OOM | **CORRIGIDO** — `bounded_ctx()` limita por work_mem (parquet grande → erro tipado, não OOM). |
| MEDIUM (rust-pgrx) | `SELECT * FROM {rel}` interpola cru (seguro só por acidente) | **CORRIGIDO** — nome canônico via SPI parametrizado `$1::regclass::text` (injection-safe). |
| MEDIUM (cross-val) | `run_m62_htap.py`/`test_htap.py` chamam funções dropadas | **CORRIGIDO** — deletados (testavam a superfície pg_duckdb removida; não no CI). |
| MEDIUM×2 (benchmark) | 118 MB herdado do M142 (não re-medido); +9 MB do .so é cross-ambiente | **CORRIGIDO** — docs citam a fonte M142 + lideram com o delta de imagem +12 MB (same-env docker). |
| LOW (rust-pgrx) | temp de escrita fixo (corrida) + órfão em erro | **CORRIGIDO** — temp único por-backend (`{path}.{pid}.tmp`) + cleanup best-effort. |
| LOW (index-storage) | `v_n` atribuído nunca usado | **CORRIGIDO** — `PERFORM public.write_parquet(...)`. |
| LOW (cross-val) | README/ADR citam `theodb.read_parquet` (vivem em public) | **CORRIGIDO** — README/ADR referenciam `public.*` + nota least-privilege. |
| LOW (cross-val) | `spike-parquet-validate.sh` órfão | **CORRIGIDO** — deletado; citação removida do ADR-0057. |
| LOW (benchmark) | check de shared_preload mascara falha de psql | **CORRIGIDO** — captura-depois-assere. |

## Validação pós-fix (imagem shipada theodb:m143b, e2e-runner PG18.4)

`scripts/m143-removal-validate.sh` → **NO_PGDUCKDB + M62_OWNCODE + READ_MULTI + WRITE_FAILCLOSED + REVOKE_OK +
imagem 724 MB → M143_REMOVAL_OK**. O gate `REVOKE_OK` prova o fix do HIGH-1 (lowpriv bloqueado). Evidência:
`docs/benchmarks/m143-pgduckdb-removal.md`.

## DoD do milestone (ROADMAP M143) — verificação

Cross-validation confirmou 8/8 da Coverage Matrix com contrapartida real no diff; remoção COMPLETA
(Dockerfile.htap/CI job/scripts obsoletos deletados; 0 referência ATIVA a pg_duckdb; extensão idêntica nas
imagens; cadeia de upgrade 1.5→1.6 convergente).

## Conclusão

Merge-ready **após corrigir os 2 HIGH** (ACL de arquivo arbitrário + longjmp-safety) — os dois eram bugs reais
que os reviews adversariais pegaram antes do merge (o valor do council). Todos os achados corrigidos e
re-validados na imagem que ships. **Gate M143 → READY_TO_MERGE.**
