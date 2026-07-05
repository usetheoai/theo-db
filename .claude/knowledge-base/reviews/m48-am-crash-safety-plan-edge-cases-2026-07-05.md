# Edge Case Review — m48-am-crash-safety-plan

Date: 2026-07-05
Plan analyzed: .claude/knowledge-base/plans/m48-am-crash-safety-plan.md
Tasks analyzed: 7 (T1.1, T2.1, T2.2, T2.3, T3.1, T4.1, T5.1, T6.1)
Cases found: 11 (EDGE: 4, NEGATIVE: 7 | MUST FIX: 3, SHOULD TEST: 5, DOCUMENT: 3)

## MUST FIX

### EC-1: Índice v1 (blob) no VACUUM pós-mudança — caminho indefinido
- **Affected task:** T2.1
- **Kind:** NEGATIVE (formato legado inesperado no caminho novo)
- **Family:** Format / State
- **Scenario:** usuário tem índice criado pré-M48 (blob v1 ou structured com meta v1). Roda VACUUM. O
  plano remove os rewrites in-place e diz (T3.1) "v1 blob → skip com WARN", mas o `vacuum_rebuild` de
  DELETE (ambulkdelete) sobre v1 fica indefinido: mantém o rewrite antigo (⇒ #47 continua aberto para
  v1!) ou falha?
- **Impact:** ou o bug #47 persiste silenciosamente para índices legados, ou VACUUM erra em produção.
- **Suggested fix (1 frase no plano):** o fold **auto-migra**: `vacuum_rebuild` lê o corpus vivo
  (formato-agnóstico — já lê ambos) e o fold SEMPRE escreve geração nova em meta v2 ⇒ o primeiro VACUUM
  pós-upgrade migra v1→v2 atomicamente (crash-safe pelo próprio meta-pivot); os rewrites in-place morrem
  de verdade.

### EC-2: `GetFreeIndexPage` pode devolver bloco inválido — re-init destruiria a meta
- **Affected task:** T2.2
- **Kind:** NEGATIVE (recurso externo advisory devolve valor fora do contrato)
- **Family:** Resource / Boundary
- **Scenario:** FSM é cache advisory não-WAL-logged (Blueprint §Q4); após crash/replay pode conter lixo —
  inclusive bloco 0 (meta) ou bloco ≥ nblocks. O `extend_page_with_item` FSM-first re-inicializa
  incondicionalmente ⇒ re-init do bloco 0 = **meta destruída** (data loss).
- **Impact:** corrupção do índice por consumo de FSM stale — exatamente a classe de bug que o M48 fecha.
- **Suggested fix (3 linhas):** guard no consumo:
  `if b == 0 || b >= RelationGetNumberOfBlocksInFork(rel, MAIN) { /* ignora FSM, extend normal */ }`
  (o nbtree faz re-verificação equivalente — `nbtpage.c:911-925`).

### EC-3: `amcostestimate` pode ler meta torn sob NoLock durante fold concorrente — planner NUNCA pode errar
- **Affected task:** T5.1
- **Kind:** NEGATIVE (leitura concorrente de página em transição)
- **Family:** Timing / State
- **Scenario:** costestimate lê a meta com NoLock (padrão pgvector `hnsw.c:166-168`) enquanto um fold
  concorrente pivota o bloco 0. Leitura torn/inconsistente → se o parse der `Err` e o código propagar
  como `error!`, **todo planejamento de query quebra** durante VACUUMs.
- **Impact:** DoS acidental do planner durante manutenção.
- **Suggested fix (1 frase):** contrato explícito no plano: costestimate trata QUALQUER falha de leitura
  da meta como fallback silencioso `ratio=1.0` (genericcost puro) — nunca `error!`; o teste negativo
  asserta que EXPLAIN funciona com meta ilegível (mock: índice v1).

## SHOULD TEST

### EC-4: Cancel de VACUUM mid-fold (aborto de transação SEM morte de processo)
- **Affected task:** T2.1/T2.3
- **Kind:** NEGATIVE
- **Suggested test:** `test_cancel_vacuum_mid_fold_leaves_old_generation` — `pg_cancel_backend` no
  VACUUM (responsivo via `vacuum_delay_point` do T4.1); assert: scan consistente (geração velha),
  páginas novas órfãs toleradas, re-VACUUM converge. Cobre o caminho "GenericXLog páginas já commitadas
  persistem mas a transação aborta" — estado distinto do crash (sem replay).

### EC-5: Fold para índice vazio (DELETE ALL + VACUUM)
- **Affected task:** T2.1
- **Kind:** EDGE (menor corpus válido: zero)
- **Suggested test:** `test_fold_empty_corpus` — DELETE 100% + VACUUM; assert scan retorna 0 rows sem
  erro; INSERT novo funciona; nova geração = meta-only válida.

### EC-6: Boundary exato do threshold da pending
- **Affected task:** T3.1
- **Kind:** EDGE
- **Suggested test:** `test_pending_threshold_boundary` — pending == threshold ⇒ NÃO folda (semântica
  `>`); threshold+1 ⇒ folda. Assert dos dois lados da fronteira.

### EC-7: `ALTER TABLE ... SET UNLOGGED` também chama ambuildempty
- **Affected task:** T1.1
- **Kind:** EDGE (segundo caminho válido para o mesmo código)
- **Suggested test:** `test_alter_set_unlogged_survives_crash` — tabela LOGGED com índice → `ALTER TABLE
  SET UNLOGGED` → crash/restart → reset válido + INSERT ok (mesmo assert do T1.1 via caminho ALTER).

### EC-8: Anti-flake do teste de cancel (T4.1)
- **Affected task:** T4.1
- **Kind:** EDGE (timing do teste, não do produto)
- **Suggested test:** calibração no próprio teste: medir o build baseline 1× (sem cancel); `pytest.skip`
  com WARN se baseline < 10s (box rápida demais para cancel confiável); senão cancela em baseline/4.
  Evita flake sem esconder o assert.

## DOCUMENT

### EC-9: ENOSPC durante shadow-write (pico ~2× de disco)
- **Kind:** NEGATIVE
- **Accepted risk:** extend falha → ERROR → transação aborta → geração velha intacta (mesma classe do
  crash pré-pivot, já testada). ENOSPC determinístico não é reproduzível de forma confiável no container
  (tmpfs/quota flakiness > valor). Risco aceito; comportamento coberto por equivalência com EC-4/T2.3.

### EC-10: Réplica streaming do INIT fork
- **Kind:** EDGE
- **Accepted risk:** sem harness de 2 nós no M48. O WAL emitido (`log_newpage_range` FPIs) é o MESMO
  mecanismo que a suíte TAP do pgvector valida por proxy de réplica (Blueprint §Q7). Seed do backlog
  para harness futuro.

### EC-11: pg_upgrade entre formatos meta v1/v2
- **Kind:** NEGATIVE
- **Accepted risk:** fora de escopo do M48 — o caminho suportado é REINDEX (erro tipado instrui) OU o
  auto-migrate do EC-1 no primeiro VACUUM. pg_upgrade preserva bytes on-disk; ambos os caminhos cobrem.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 | 0 | 0 | 1 (EC-7) | 1 (EC-10) |
| T2.1 | 1 | 2 | 1 (EC-1) | 2 (EC-4, EC-5) | 1 (EC-9) |
| T2.2 | 0 | 1 | 1 (EC-2) | 0 | 0 |
| T2.3 | 0 | 0 | 0 | 0 | 0 (coberto no plano) |
| T3.1 | 1 | 0 | 0 | 1 (EC-6) | 0 |
| T4.1 | 1 | 0 | 0 | 1 (EC-8) | 0 |
| T5.1 | 0 | 1 | 1 (EC-3) | 0 | 0 |
| T6.1 | 0 | 0 | 0 | 0 | 0 |
| (plan-wide) | 0 | 1 | 0 | 0 | 1 (EC-11) |

**Coverage check:** toda task com fronteira de input tem ≥1 EDGE e ≥1 NEGATIVE considerados (T2.3 e
T6.1: lentes cobertas pelos cenários já embutidos no plano — crash states e load-guard).

**Verdict:** PLAN NEEDS ADJUSTMENT (3 MUST FIX, todos ≤3 linhas/1 frase — absorver e bump v1.0 → v1.1)
