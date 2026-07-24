# Edge Case Review — m148-flamegraph-spike

Date: 2026-07-24
Tasks analyzed: 2 (T1.1, T1.2)
Cases found: 4 (EDGE: 1, NEGATIVE: 3 | MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 1)

Spike de medição — a maioria dos riscos já está nos ADR/Risks do plano (R1 I/O-vs-CPU, R2 inlining, R3 perf
indisponível). Este review cobre o que o plano ainda não enxerga: a **honestidade da medição** (um
flamegraph errado prioriza os milestones errados — pior que não medir).

## MUST FIX

### EC-1: `perf record --call-graph dwarf` num backend curto pode capturar 0 amostras
- **Affected task:** T1.1
- **Kind:** NEGATIVE (falha)
- **Family:** Timing
- **Scenario:** o `perf record -p <pid>` é iniciado por um script; se a query terminar antes do `perf`
  anexar (ou se a query for curta demais para amostrar a 111 Hz), o `perf.data` sai vazio e o flamegraph é
  uma faixa branca. O harness reportaria "OK" sobre uma medição vazia.
- **Impact:** veredito sobre um flamegraph vazio → prioriza milestone errado. É o análogo do harness vácuo
  do #190.
- **Suggested fix:** T1.1 deve **abortar** se `perf script | wc -l` for menor que um piso (ex.: 500
  amostras) — `[ "$(perf script -i data.perf | wc -l)" -gt 500 ] || fail "perf capturou poucas amostras"`.
  Escolher uma query de ≥40s (q33/q34) garante janela ampla.

## SHOULD TEST

### EC-2: a query alvo (q33 REGEXP_REPLACE) pode ter o tempo dominado pelo regexp, não pelo scan
- **Affected task:** T1.2
- **Kind:** EDGE (extremo válido)
- **Suggested test:** profilar **duas** queries — a mais lenta (q33/q34) E uma de scan puro sem função cara
  (ex.: q1 `SELECT COUNT(*)... WHERE` ou uma projeção simples). Se o frame dominante da q33 for
  `regexp_replace` (função do PG, não nosso scan), o veredito deve dizer que **essa query específica não é
  representativa do gargalo do colunar** — e usar a query de scan puro para a priorização. O plano já prevê
  isso na Q1/Q2, mas o DoD de T1.2 deve **exigir** a comparação, não deixá-la opcional.

### EC-3: símbolos de `zstd`/`arrow` podem aparecer como `[unknown]` mesmo com nossos símbolos resolvidos
- **Affected task:** T1.1
- **Kind:** NEGATIVE (formato)
- **Suggested test:** se o frame dominante for `[unknown]` ou um endereço de uma lib de sistema (zstd
  dinâmico, libc), o veredito ainda consegue atribuir? Assertar que `perf script` resolve ao menos os
  frames de `theodb_rs` e do PG; se `zstd` vier sem símbolo, o doc deve dizer "descompressão (zstd, símbolo
  ausente)" honestamente, em vez de omitir o maior frame.

## DOCUMENT

### EC-4: o flamegraph é de UM box (c-8 DO), não do canônico — o % pode variar por CPU
- **Kind:** EDGE
- **Accepted risk:** as proporções de tempo (decode vs deform vs I/O) podem diferir num CPU diferente
  (cache maior, AVX-512). Para o M148 (priorizar entre 3 técnicas), a **ordem relativa** dos frames é
  robusta a isso; o valor absoluto não é o entregável. Documentar no doc que o % é do box de medição.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 0 | 2 | 1 (EC-1) | 1 (EC-3) | 0 |
| T1.2 | 1 | 1 | 0 | 1 (EC-2) | 1 (EC-4) |

**Verdict:** PLAN NEEDS ADJUSTMENT

Um MUST FIX — o mesmo perigo do #190: um harness que reporta verde sobre uma medição vazia. A defesa (piso
de amostras) é uma linha. Os SHOULD TEST reforçam a **honestidade do veredito**: profilar 2 queries (não
tomar a regexp como representativa) e não omitir frames sem símbolo. Cabem no DoD das tasks sem tocar as
ADRs.
