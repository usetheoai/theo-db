# Edge Case Review — m153-groupby-text

Date: 2026-07-25
Tasks analyzed: 2 (T1.1, T2.1)
Cases found: 4 (EDGE: 2, NEGATIVE: 2 | MUST FIX: 0, SHOULD TEST: 3, DOCUMENT: 1)

## SHOULD TEST

### EC-1: chave de grupo MISTA (texto + int) com o texto NÃO-determinístico
- **Affected task:** T1.1
- **Kind:** NEGATIVE (input inválido p/ o fast-path)
- **Suggested test:** `groupby_mixed_keys_nondet_text_declines` — `GROUP BY int_col, ci_text_col` (ci = ICU não-determinística) DECLINA. O guard deve checar `get_collation_isdeterministic` em TODA chave texto (não só a primeira). Assere EXPLAIN sem theodb_columnar_agg + A/B correto.

### EC-2: chave de grupo texto NULL
- **Affected task:** T1.1
- **Kind:** EDGE (extremo válido)
- **Suggested test:** `groupby_text_null_group` — linhas com `phrase IS NULL` formam um grupo NULL; DataFusion e PG agrupam NULLs juntos; após o re-sort do pai, o NULL ordering é do Sort pai. Assere A/B == heap (conjunto+ordem), incluindo o grupo NULL.

### EC-3: `parent` é `Sort` mas com nó intermediário (Result/Projection) entre Sort e Agg
- **Affected task:** T1.1
- **Kind:** NEGATIVE (forma de plano inesperada)
- **Suggested test:** confirmado por EXPLAIN no droplet (Unresolved Q do plano). Se o pai imediato do Agg não for `T_Sort`, o guard fail-closed DECLINA (correto — não roteia com ordem possivelmente errada). Assere que a forma real das q16/17/33 é `Limit→Sort→Agg` (pai imediato = Sort); se não, o decline mantém a correção.

## DOCUMENT

### EC-4: `ORDER BY count DESC LIMIT k` com contagens EMPATADAS
- **Affected task:** T2.1
- **Kind:** EDGE (ambiguidade inerente)
- **Accepted risk:** com muitos empates na contagem, o corte do LIMIT escolhe um subconjunto arbitrário — o PG **também** é não-determinístico aqui (sem ORDER BY total). O oráculo A/B do `run_m128` remove o LIMIT e canonicaliza order-insensitive (M152), então empates NÃO causam falsa divergência. Não é bug de correção; é a semântica do próprio PG. Se o A/B fosse sobre o corte exato do LIMIT, precisaria de tie-breaker total (isso é o escopo do M155/Top-N, não do M153).

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 | 2 | 0 | 3 | 0 |
| T2.1 | 1 | 0 | 0 | 0 | 1 |

**Coverage check:** T1.1 (a fronteira de admissão) tem EDGE (NULL group) e NEGATIVE (mista não-det, forma de plano) cobertos pelos guards ADR-1; T2.1 cobre a ambiguidade de LIMIT-tie via o oráculo do harness.

**Verdict:** PLAN OK — os 2 guards do ADR-1 (determinismo + re-sort acima) já cobrem os MUST-level; EC-1/2/3 reforçam os TDDs de T1.1 (checar TODA chave texto, grupo NULL, forma do plano confirmada por EXPLAIN antes do GREEN).
