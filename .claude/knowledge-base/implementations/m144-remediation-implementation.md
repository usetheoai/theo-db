---
slug: m144-remediation
milestone_id: M144
date: 2026-07-23
plan: .claude/knowledge-base/plans/m144-remediation-plan.md
status: IMPLEMENTATION_COMPLETE
---

# Implementation — M144 Remediação P0+P1 do code-review

Fecha os 3 HIGH + 4 MEDIUM P1 do audit `theodb-rs-code-review-2026-07-23.md` sob TDD.
**Substrato de validação:** `cargo pgrx test` é estruturalmente inexecutável neste ambiente
(a) o bench `scan_hot_path` sob `pg_test` não linka contra símbolos PG; (b) o test-binary do
crate não linka PG standalone — limitação inerente do pgrx. Portanto a validação é feita no
**droplet DO (theo-e2e-runner, PG18.4 pgrx-install, cargo-pgrx 0.19)**: instalar a extensão +
smokes SQL comportamentais + o harness de upgrade M137. Funções puras são provadas standalone
com `rustc --test`.

## Tarefas + evidência medida (todas provadas no droplet 2026-07-23)

| Task | Fix | Evidência (droplet PG18.4) |
|---|---|---|
| **T1.1** upgrade chain | `theodb_rs--1.1.0--1.2.0.sql` full-schema self-healing + `default_version=1.2.0` | Harness `scripts/test-upgrade.sh` FROM=1.1.0 TO=1.2.0: **SCENARIO_A_OK** (pós-upgrade==clean, schema+ACL) + **CONVERGENCIA_OK** (280→290) + **IDEMPOTENTE_OK** (2×, snapshot inalterado) + **SCENARIO_B1_DONE** (.so novo sobre catálogo 1.1.0, 0 crashes). **TODOS PASSARAM, exit 0** |
| **T1.2** REVOKE `symqg_spike_bench` | `REVOKE ALL … FROM PUBLIC` via `extension_sql!` + no upgrade script | `has_function_privilege('public', 'symqg_spike_bench(text,bigint,bigint,int)','EXECUTE')` = **false**; role comum = **false**; owner (postgres) = **true** (função permanece no `.so`, upgrade-safe). read_parquet/write_parquet/olap idem = false |
| **T1.3** delete propaga | 2 armas de `_vectorizer_process_delete` trocam `let _=` por `.unwrap_or_else(err_input)` | Smoke: doc ausente → **retorna limpo** (marca done, 0 rows); target quebrada → **diverge** (`column "emb" does not exist`), nunca `Ok`. **Nota honesta:** no pgrx 0.19 o erro de DML faz longjmp (o `let _=` antigo já propagava) — fix é defense-in-depth + fecha o caminho `SpiError`-code; a propriedade de segurança (delete falho ≠ done) já valia via `in_subtxn_msg`/M132. ADR-3 |
| **T2.1** columnar flush vs DROP | `try_relation_open` no PRE_COMMIT pula OIDs dropados | Smoke: `BEGIN; CREATE … USING theodb_columnar; INSERT 1000; DROP TABLE; COMMIT;` → **COMMIT limpo, servidor vivo** (sem crash). Controle não-dropado: flush+read = count 1000 / sum 5005000 |
| **T2.2** sanitize Unicode | `ascii_ci_prefix` (um espaço de índice via `to_ascii_lowercase`) | `rustc --test` standalone (função pura): 3/3 pass. **RED→GREEN real:** `"İİ sk-…"` → antigo (HEAD) `"İİ sk«redacted»"` (resíduo `sk`) → novo `"İİ «redacted»"`. **Nota honesta:** brute-force 48+ inputs → nenhum vazamento do segredo inteiro; é fix de desalinhamento da redação, não vazamento total. ADR-3 |
| **T2.3** backoff no retry | `mark_failed` → `pending` com deadline futuro (2^attempts, cap 300); dead-letter mantém NULL | Smoke fila: attempts=2<max=5 → state=pending, **deadline futuro secs_ahead=4** (=2²); attempts=5>=max=5 → state=**failed, deadline NULL**; attempts=20 → **capped 300s**; owner-mismatch → **no-op** (fencing H1) |
| **T2.4** guard u32 CSR | `if s>u32::MAX || d>u32::MAX { err_input }` antes do cast | Smoke: id 1M (válido) → **constrói** (guard não misfire); id u32::MAX+1 → **ERROR `node ids must fit in u32 (max 4294967295)`** (aborta antes da alocação). **Nota honesta:** CSR é denso indexado por id → u32::MAX literal = ~34GB OOM; teste EDGE usa 1M factível. ADR-3 |
| **test-infra** | `test=false` no `[[bench]]` + split `scan_core.rs`(puro)/`scan_core_mem.rs`(pg_test) | `cargo check --features pg18,pg_test --tests` → **exit 0** (destrava TODOS os `#[pg_test]` do crate) |

## Descobertas honestas da validação real (Regra 3)

A validação de verdade (não só compilar) pegou **3 defeitos de teste** que `cargo pgrx test`
teria pegado se rodasse — corrigidos:

1. **T1.3**: `#[pg_test(error="vectorizer delete failed")]` nunca casaria — o pgrx faz longjmp do
   erro SQL cru (`spi.rs:400-427`). Corrigido para `error="does not exist"` + ADR-3 documentando
   que #76 é defense-in-depth (o audit já o marcara `heuristic`).
2. **T2.4 EDGE**: `csr_build_accepts_u32_max_boundary` construía CSR denso a u32::MAX → OOM (~34GB).
   Corrigido para id factível 1M (`csr_build_accepts_large_valid_u32_id`); o REJECT no limite exato
   fica no teste NEGATIVE (aborta antes da alocação).
3. **T2.2**: `!out.contains(token)` passava no código antigo também (o desync corrompe mas redige o
   segredo). Corrigido para asserção de saída limpa exata (`"İİ «redacted»"`), um RED→GREEN genuíno.

## Wiring

Todos os fixes são em símbolos JÁ chamados em produção (caller pillar (a) satisfeito por
construção — não há símbolo novo órfão): `_vectorizer_process_delete`/`_vectorizer_mark_failed`
são chamados pelo worker; `flush_pending` pelo xact callback; `graph_build` é `#[pg_extern]`
público; o REVOKE é `extension_sql!` aplicado no install. Integração (pillar b) = os smokes SQL
acima contra o PG real. Observabilidade (pillar c): `last_error`/estado da fila observáveis via
SQL (provado em T2.3).

## Artefatos de validação

- Smokes: `/tmp/…/scratchpad/t22_*.rs` (provas standalone T2.2), smokes SQL inline no droplet.
- Harness: `scripts/test-upgrade.sh` (T1.1) — re-executável, exit 0.
- Build: `cargo check --features pg18,pg_test --tests` exit 0; `cargo pgrx install` (o `.so` 1.2.0
  instalado) exit 0.
