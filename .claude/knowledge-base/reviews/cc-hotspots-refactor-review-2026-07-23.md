---
slug: cc-hotspots-refactor
milestone_id: M145
date: 2026-07-23
verdict: READY_TO_MERGE
agents: [council-rust-pgrx, cross-validation]
---

# Review — M145 Refactor dos hotspots de CC

**Veredito consolidado: READY_TO_MERGE** — 0 BLOCKER/HIGH/MEDIUM. 2 agentes adversariais.

## Confirmações (com evidência independente)

- **council-rust-pgrx** (comparação byte-a-byte vs `HEAD~5`): as **3 extrações `unsafe` são refatorações PURAS** (zero mudança de comportamento). admit: `classify_target_node` rejeita EXATAMENTE os mesmos nós, `layout.push` na ordem/índices, columnar-antes-de-heap, nenhum deref `*mut` partido. main_index_pages: cut-and-move VERBATIM de todo offset/stride/guard, `match ver` exaustivo, NÃO unificado (ADR-2), buffer invariant intacto. worker: todo txn/subtxn boundary movido INTACTO (M122 xmin/H-1/H1), PgTryBuilder no escopo sem-txn, semântica sigterm-break genuinamente equivalente, `owner:&str` correto.
- **cross-validation** (re-mediu CC com lizard): CC **independentemente verificado** — `write_parquet_impl` 35→19, `theodb_embed_worker_main` 41→14, `main_index_pages` 34→11, `admit` 59→17 (bate exato). Nenhuma NOVA fn>25 (`begin_custom_scan`=26 confirmado pré-existente em HEAD~6). Byte-idêntico estruturalmente substanciado. Honestidade do `enable_columnar_agg=on` no A/B correta. Zero mudança de superfície SQL (`#[pg_extern]` inalteradas). T1.2 no-smoke honestamente admitido (limitação legítima de extração pura; fns SQL inalteradas, testadas em M122/M144). Coverage 100%. O "delta" worker 14→13 explicado: −1 = linha de comentário removida, não txn dropado (9 wrappers antes e depois).

## Findings não-bloqueantes

| Sev | Finding | Ação |
|---|---|---|
| LOW | `classify_target_node` CC=24 (um do teto de 25) | Registrado; sem violação agora — um edit futuro deve vigiar |
| INFO | Números A/B (0/0, 2/2, 20/20) vêm do run no droplet, não reproduzíveis no env de review | Esperado (pgrx/PG18; `cargo pgrx test` inexecutável local); a re-medição de CC + prova estrutural compensam |
| INFO | Boilerplate do header do CHANGELOG ("pré-código") stale | Pré-existente, fora do escopo M145 |

## Gate

READY_TO_MERGE (0 BLOCKER, 0 HIGH — critério `cycle-review.md`). O risco #1 do plano (regredir o
byte-idêntico M115 do admit) foi provado NÃO-realizado: extração pura + A/B `col EXCEPT heap`=0/0 com
`Custom Scan (theodb_columnar_agg)` ativo. Nenhum fix necessário.
