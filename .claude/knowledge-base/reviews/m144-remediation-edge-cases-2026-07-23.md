# Edge Case Review — m144-remediation

Date: 2026-07-23
Tasks analyzed: 8 (1.A, 1.B, 1.C, 2.1, 2.2, 2.3, 2.4, 3.1)
Cases found: 5 (EDGE: 3, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 3, DOCUMENT: 1)

Verificação de estado do código (lição #46/#47): o delete em `theodb_rs/src/vectorizer.rs:455` **ainda** tem `let _ = Spi::run_with_args(...)` nos dois braços (`:460`, `:469`) — fix 1.C é real, não já corrigido. O upsert sibling (`:377,:389,:447`) já usa `.unwrap_or_else(err_input)` — é o shape-alvo.

## MUST FIX

### EC-1: delta de upgrade deve usar `CREATE OR REPLACE`, não `CREATE`
- **Affected task:** T1.A
- **Kind:** NEGATIVE (idempotência quebrada)
- **Family:** State / Format
- **Scenario:** o oráculo IDEM do `scripts/test-upgrade.sh` roda o script de upgrade **duas vezes**. Se o delta `1.1.0--1.2.0.sql` emitir `CREATE FUNCTION read_parquet(...)` (bare), a 2ª aplicação erra "function already exists" → IDEM vermelho. O script `1.0.0--1.1.0.sql` já usa `CREATE OR REPLACE` (129×) exatamente por isso.
- **Impact:** DoD bullet-1 (IDEM OK) falha; upgrade não-idempotente.
- **Suggested fix:** Task 1.A — especificar `CREATE OR REPLACE FUNCTION` para as 3 funções lakehouse (copiar verbatim o bloco gerado, que já é OR REPLACE); os REVOKEs são idempotentes por natureza.

## SHOULD TEST

### EC-2: delete de doc já ausente (0 linhas afetadas) deve marcar done, não falhar
- **Affected task:** T1.C
- **Kind:** EDGE (extremo válido — doc já removido)
- **Suggested test:** `process_delete_of_absent_doc_marks_done` — enfileira um delete de um `source_pk` inexistente; o SPI retorna `Ok` com 0 linhas; assert que o job vira `done` (NÃO falha). Garante que o fix propaga só o `Err` do SPI, nunca trata "0 rows" como falha — protege contra um refactor futuro que confunda vazio com erro.

### EC-3: backoff exponencial não pode estourar com `attempts` grande
- **Affected task:** T2.3
- **Kind:** EDGE (extremo válido — muitas tentativas antes do dead-letter)
- **Suggested test:** `backoff_saturates_for_large_attempts` — `attempts = 60`; assert que `2^attempts` satura no cap (não overflow de `i64`/panic). Fix: computar `least(1i64.checked_shl(attempts.min(30)).unwrap_or(cap), cap)` — limitar o expoente antes do shift.

### EC-4: exatamente `u32::MAX` é node-id válido (boundary), `u32::MAX + 1` é rejeitado
- **Affected task:** T2.4
- **Kind:** EDGE + NEGATIVE (o par de fronteira)
- **Suggested test:** estender `csr_build_rejects_node_id_over_u32` com 2 asserts: (EDGE) node-id `= u32::MAX` constrói a CSR OK; (NEGATIVE) node-id `= u32::MAX as i64 + 1` → erro tipado. Cobre os dois lados da fronteira, não só o inválido.

## DOCUMENT

### EC-5: upgrade encadeado 1.0.0→1.1.0→1.2.0 é aplicado pelo PG automaticamente
- **Kind:** EDGE (caminho válido não testado diretamente)
- **Accepted risk:** o teste do DoD-1 cobre `FROM_VER=1.1.0`. Um usuário em 1.0.0 que roda `ALTER EXTENSION theodb_rs UPDATE` (sem `TO`) faz o PG aplicar a cadeia 1.0.0→1.1.0→1.2.0 em sequência. A perna 1.0.0→1.1.0 já é provada pelo M137 (`test-upgrade.sh` default); a perna 1.1.0→1.2.0 é o alvo desta milestone. Aceito documentar que o PG encadeia — não precisa de um teste `FROM_VER=1.0.0 TO_VER=1.2.0` dedicado (seria redundante com as duas pernas já provadas). Registrar como nota no `docs/benchmarks/m144-upgrade-1.2.0.md`.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| 1.A | 1 | 1 | 1 | 0 | 1 |
| 1.B | 0 | 0 | 0 | 0 | 0 |
| 1.C | 1 | 0 | 0 | 1 | 0 |
| 2.1 | 0 | 0 | 0 | 0 | 0 |
| 2.2 | 0 | 0 | 0 | 0 | 0 |
| 2.3 | 1 | 0 | 0 | 1 | 0 |
| 2.4 | 1 | 1 | 0 | 1 | 0 |
| 3.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** 1.B (gate-out) e 2.1/2.2 já têm EDGE+NEGATIVE cobertos no plano (RED test + failure scenario). Os fixes restantes ganham os testes de fronteira acima.

**Verdict:** PLAN NEEDS ADJUSTMENT (1 MUST FIX — `CREATE OR REPLACE` no delta; trivial) — depois PLAN OK.
