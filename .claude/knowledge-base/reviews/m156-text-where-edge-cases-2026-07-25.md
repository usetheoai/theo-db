# Edge Case Review — m156-text-where

Date: 2026-07-25
Tasks analyzed: 4 (T1.1, T2.1, T3.1, T4.1)
Cases found: 5 (EDGE: 2, NEGATIVE: 3 | MUST FIX: 0, SHOULD TEST: 4, DOCUMENT: 1)

Os guards do blueprint (ADR-2) + os riscos do plano já cobrem os MUST-level. Reforços nos TDDs:

## SHOULD TEST

### EC-1: LIKE com escape do PG (`LIKE 'a\%b'`) — o `\` literal
- **Affected task:** T3.1
- **Kind:** NEGATIVE (semântica divergente)
- **Suggested test:** `like_escape_matches_pg` — `WHERE col LIKE 'a\%b'` (casa `a%b` literal) byte-idêntico ao heap. O `escape_char` do `Expr::Like` DEVE ser `\` (default do PG). Assere A/B == heap.

### EC-2: round-trip de const com `%`/`_`/vazio/UTF-8 multibyte
- **Affected task:** T2.1
- **Kind:** EDGE (extremos válidos)
- **Suggested test:** `text_const_roundtrip_special` — const `''`, `'%'`, `'_'`, `'café'` (multibyte), `'a\b'` sobrevivem o copyObject+decode byte-a-byte. Assere A/B == heap para cada.

### EC-3: ILIKE / regex / bpchar / collation não-determinística DECLINAM
- **Affected task:** T1.1
- **Kind:** NEGATIVE (não-empurrável)
- **Suggested test:** `text_where_declines_unsafe` — `ILIKE '%x%'`, `~ 'x'` (regex), `bpchar_col = 'x'`, `ci_text = 'x'` (ICU não-det) → EXPLAIN nativo + A/B correto. Fail-closed.

### EC-4: const NULL (`WHERE col = NULL` / const nulo) não empurra
- **Affected task:** T1.1
- **Kind:** NEGATIVE (SQL NULL semantics)
- **Suggested test:** `text_where_null_const_declines` — um `Const` com `constisnull` → declina (o filtro DataFusion sobre NULL tem semântica 3-valued; declinar ao nativo é fail-safe). Assere decline + A/B.

## DOCUMENT

### EC-5: WHERE misto texto + numérico (`col_txt = 'x' AND col_int > 5`)
- **Affected task:** T3.1
- **Kind:** EDGE (combinação válida)
- **Accepted risk:** o `build_filter_expr` combina os predicados numéricos E texto com `and`. Coberto pelo REFACTOR de T3.1 (A/B com WHERE misto). Não é MUST — é a composição natural; o A/B prova.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 0 | 2 | 0 | 2 | 0 |
| T2.1 | 1 | 0 | 0 | 1 | 0 |
| T3.1 | 1 | 1 | 0 | 1 | 1 |
| T4.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** T1.1 (admissão) cobre NEGATIVE (unsafe declines, NULL const) + T2.1/T3.1 cobrem EDGE (round-trip, escape). Os guards do ADR-2 + os riscos do plano já são os MUST-level.

**Verdict:** PLAN OK — os ADRs + riscos do plano cobrem os MUST; EC-1/2/3/4 reforçam os TDDs (escape, round-trip, declínios, NULL const).
