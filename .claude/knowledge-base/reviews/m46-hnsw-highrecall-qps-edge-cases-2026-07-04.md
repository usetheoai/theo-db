# Edge Case Review — m46-hnsw-highrecall-qps

Date: 2026-07-04
Tasks analyzed: 3 (T2.1 código, T1.1 baseline, T3.1 veredito)
Cases found: 5 (EDGE: 2, NEGATIVE: 3 | MUST FIX: 0, SHOULD TEST: 2, DOCUMENT: 3)

O milestone é **recall-neutro** (só muda alocação, não a ordem de visita). As bordas de correção de
resultado já são guardadas pelo teste-âncora byte-exato + pages_read idêntico que o plano exige. A varredura
foca no que a mudança de alocação pode quebrar sutilmente.

## MUST FIX

(nenhum — as bordas de crash/corrupção reais já estão cobertas: índice vazio short-circuita em
`hnsw_page.rs:485`; a GUC `ef_search` é clamped a [1,1000] pelo `GucRegistry` (`guc.rs:22-23`), então
`ef*m0*2 ≤ ~128k` slots — sem OOM/overflow; `ef.max(1)` em `:491` cobre ef=0.)

## SHOULD TEST

### EC-1: as duas variantes de decode_neighbors devem concordar (L1-B)
- **Affected task:** T2.1
- **Kind:** NEGATIVE (regressão silenciosa se a variante `_into` divergir da original)
- **Suggested test:** `test_decode_neighbors_into_matches_original` — para um buffer de neighbor tuple fixo,
  asseverar `decode_neighbors_into(&mut v)` produz um `Vec<Addr>` **byte-idêntico** ao `decode_neighbors`
  original (property de equivalência). Pega o bug clássico do scratch-não-limpo (neighbors do nó anterior
  vazando) diretamente na unidade, sem depender só do end-to-end.

### EC-2: scratch reusado limpo entre nós (L1-B)
- **Affected task:** T2.1
- **Kind:** NEGATIVE (se o `scratch.clear()` faltar, o resultado é ERRADO — recall quebra)
- **Suggested test:** já coberto indiretamente pelo teste-âncora byte-exato (`test_traverse_presize_is_recall_neutral`)
  — se o scratch não for limpo, a ordem muda e a saída diverge → o teste falha. Manter o teste-âncora com ef
  alto (ef=200+) e ≥2 nós expandidos para exercitar o reuse através de múltiplos nós. (Nenhuma mudança de
  plano — reforço da assertion existente.)

## DOCUMENT

### EC-3: overflow/OOM de `with_capacity` é impossível pela GUC bound
- **Kind:** EDGE (maior valor válido de ef)
- **Accepted risk:** `ef ≤ 1000` (GUC clamp) × `m0 ≤ 32` × 2 = ~64k slots máx → alocação trivial. Não há
  caminho para um `with_capacity` gigante. Nenhuma validação extra necessária.

### EC-4: baseline e pós devem rodar back-to-back (metodologia)
- **Kind:** NEGATIVE (comparação inválida se o ambiente driftar entre as medições)
- **Accepted risk / nota de plano:** rodar o baseline (T1.1) e o pós (T3.1) **na mesma sessão, consecutivos**
  (idealmente intercalando runs), para que o ruído da dev box afete ambos igualmente. O `effect>variância` +
  median já mitiga; documentar explicitamente no report M46 que baseline e pós foram medidos back-to-back.

### EC-5: m0=0 (índice degenerado)
- **Kind:** EDGE (menor valor)
- **Accepted risk:** `with_capacity(0)` = equivalente a `new()`; scratch vazio. Sem problema. E um índice
  real nunca tem m0=0 (default 15). Nenhum fix.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T2.1 | 2 | 2 | 0 | 2 | 2 |
| T1.1 | 0 | 1 | 0 | 0 | 1 |
| T3.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** T2.1 (o único que toca lógica de scan) tem EDGE (ef máx/mín, m0=0) e NEGATIVE (scratch
não-limpo, variantes divergentes) cobertos. T1.1 tem o NEGATIVE de I/O (container down) já no plano.

**Verdict:** PLAN OK

As 2 SHOULD TEST são reforços (EC-1 é um property test barato que vale adicionar à TDD da T2.1; EC-2 já está
coberto). Os 3 DOCUMENT não exigem mudança de código. Nenhum MUST FIX — as bordas de crash já estão guardadas
por código existente. O plano pode seguir para `/deps-audit` → `/plan-confidence`.
