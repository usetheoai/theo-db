# Review — docs de features 13–19 (diff origin/main..develop) — 2026-07-22

**Escopo:** 2 commits docs-only (+1948 linhas): `docs/features/13-19` (7 novos docs), `CHANGELOG.md`,
`.claude/knowledge-base/releases/v0.131.2-release.md`.

**Pipeline:** review-cycle 10 pilares (agentes paralelos, 9/9 arquivos cada) → júri adversarial 3×
sobre 18 findings ≥ MEDIUM → consolidação determinística → fix TDD → revalidação (2 iterações).

**Verdict:** READY_TO_MERGE

## Trajetória

| Fase | Resultado |
|---|---|
| `/review` | `NEEDS_FIXES` — 1 BLOCKER + 2 HIGH confirmados (precision do júri 0.39; 11/18 dispensados) |
| `/fix` | `FIX_COMPLETE` — 3/3 bloqueantes corrigidos sob TDD (`f258ca2`, `f911f96`, `3c8c0e4`) |
| `/revalidate` | `CONVERGED` iter 2/3 — `proven_fixed=3`, `regressed=0`, `new_high=0`; 1 LOW novo achado e corrigido (`f07315c`) |

## Findings bloqueantes corrigidos (com teste de regressão em `scripts/docs-features-lint.sh`)

1. **BLOCKER** `573f6c40` — doc 13 §5: `SELECT node FROM theodb.graph_expand(...)` quebrava copiado
   (coluna default é `graph_expand`; `RETURNS SETOF bigint`, `graph.rs:557`). Fix: alias `AS t(node)`.
2. **HIGH** `d2d82323` — doc 14: caveat DML append-only/INSERT-only não divulgado
   (`UPDATE`/`DELETE`/etc. são stubs de erro tipado, `columnar.rs:15`/`:237`). Fix: caveat (4).
3. **HIGH** `52f0c103` — doc 14 §4: claim falso de decode só das colunas projetadas no seqscan plano
   (decode-all, `columnar.rs:1015-1021`; paridade-ou-mais-lento medido em `m99-columnar-tam.md`). Fix:
   seção reescrita com o limite medido.

Correção pós-revalidação: caveat (4) ajustado — bitmap scan é desviado pelo planner (callbacks `NULL`,
`columnar.rs:304-310`), não erro tipado.

## Advisórios remanescentes (não bloqueiam)

4 MEDIUM confirmados no backlog (REVOKE não documentado no doc 13; nome de teste citado errado no doc 18
— o real é `hybrid_bm25_without_text_col_errors`; padrão metadata-only atribuído errado ao `count(*)` no
doc 14; 1 de idiomaticity) + LOW/INFO. Ver `develop-2026-07-22-artifacts/backlog.md`.

## Artefatos (audit trail)

- Manifest final (SoT do veredito, revision 6): `develop-2026-07-22-artifacts/review-manifest.json`
- Report da fase de review (snapshot pré-fix, verdict à época `NEEDS_FIXES`):
  `develop-2026-07-22-artifacts/review-2026-07-22.md`
- Dispensados pelo júri: `develop-2026-07-22-artifacts/review-2026-07-22-dismissed.json`
- Backlog: `develop-2026-07-22-artifacts/backlog.md`

Nota honesta: a suíte Rust não foi rodada — o diff inteiro é documentação + script de lint + CHANGELOG
(zero arquivos de produção tocados desde `origin/main`).
