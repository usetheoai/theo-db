# Edge Case Review — m154-count-distinct

Date: 2026-07-25
Tasks analyzed: 3 (T1.1, T2.1, T3.1)
Cases found: 5 (EDGE: 3, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 3, DOCUMENT: 1)

## MUST FIX

### EC-1: COUNT(DISTINCT text) sob collation NÃO-determinística diverge do PG
- **Affected task:** T2.1
- **Kind:** NEGATIVE (resultado errado silencioso)
- **Family:** Format / Boundary
- **Scenario:** q5 é `COUNT(DISTINCT SearchPhrase)` (texto). O `count_distinct` do DataFusion usa igualdade **byte-wise** (`memcmp`). PG COUNT(DISTINCT text) usa a **igualdade da collation da coluna**. Para collation **determinística** (C/POSIX/default deterministic) as duas coincidem (deterministic ⇒ igualdade = bytes idênticos). Mas para collation **não-determinística** (ICU `deterministic=false`, ex. case-insensitive) duas strings distintas em bytes são IGUAIS no PG → o DataFusion conta a MAIS → A/B diverge silenciosamente.
- **Impact:** resultado errado (viola o gate diverged=0) exatamente na classe texto (q5).
- **Suggested fix:** em `classify_target_node`, ao aceitar `count(DISTINCT var)` de coluna colacionável, olhar a collation da `Var` (`varcollid`) e **declinar** (`admit_trace("count_distinct_nondeterministic_collation")`) quando `!get_collation_isdeterministic(collid)` (uma consulta de catálogo; default determinística passa). Análogo ao guard de collation do M152.

## SHOULD TEST

### EC-2: COUNT(DISTINCT col) com todos os valores NULL → 0
- **Affected task:** T2.1
- **Kind:** EDGE (extremo válido)
- **Suggested test:** `count_distinct_all_null_returns_zero` — coluna 100% NULL; assere A/B == 0 (ambos excluem NULL). EDGE → resultado correto no extremo.

### EC-3: COUNT(DISTINCT a, b) multi-arg e count(DISTINCT expr) DECLINAM
- **Affected task:** T2.1
- **Kind:** NEGATIVE (input inválido p/ o fast-path)
- **Suggested test:** `count_distinct_multiarg_and_expr_decline` — `COUNT(DISTINCT col+1)` e (se o parser aceitar) qualquer aggdistinct com nº de args ≠ 1 ou arg não-`Var` cai no nativo; assere EXPLAIN sem `theodb_columnar_agg` + A/B correto. Guard: aceitar só `aggdistinct` + `args.len()==1` + arg é `Var` de coluna base. NEGATIVE → declina limpo, não roteia errado.

### EC-4: COUNT(DISTINCT col) COM GROUP BY (grouped path) declina neste slice
- **Affected task:** T2.1
- **Kind:** EDGE (o caminho agrupado)
- **Suggested test:** `count_distinct_with_group_by_declines_this_slice` — `SELECT k, COUNT(DISTINCT c) FROM t GROUP BY k` NÃO precisa rotear neste slice (o `run_columnar_grouped_aggs` só ganha CountDistinct se explicitamente ligado); assere que ou roteia com A/B==heap OU declina ao nativo (nunca roteia com resultado errado). Escopo scalar-first (M152: q13 é composto).

## DOCUMENT

### EC-5: Alta cardinalidade — HashSet do count_distinct pode não ganhar do hash-agg do PG
- **Affected task:** T3.1
- **Kind:** EDGE (extremo de escala válido)
- **Accepted risk:** `COUNT(DISTINCT UserID)` (~milhões de distintos) materializa um HashSet grande no DataFusion. Se num regime medido for pior que o hash-agg nativo do PG, é honest-negative — medir em T3.1 e declinar/documentar (não é correção, é custo). O gate de correção (diverged=0) permanece; o de performance é medido, não presumido.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 0 | 0 | 0 | 0 | 0 |
| T2.1 | 2 | 2 | 1 | 3 | 0 |
| T3.1 | 1 | 0 | 0 | 0 | 1 |

**Coverage check:** T2.1 (a fronteira de admissão) tem EDGE (empty/all-null, grouped) e NEGATIVE (collation não-determinística, multi-arg/expr) cobertos. T1.1 é construção pura de Expr (sem input externo). T3.1 é medição.

**Verdict:** PLAN NEEDS ADJUSTMENT — absorver EC-1 (MUST FIX: guard de collation determinística no texto) como sub-tarefa de T2.1 + EC-2/3/4 nos TDDs de T2.1.
