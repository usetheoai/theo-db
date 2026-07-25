# Review — M153 (GROUP BY texto → CustomScan colunar)

**Data:** 2026-07-25 · **Branch:** develop · **Commit:** e5766d2 (feat) + o fix bpchar (review remediation)
**Councils:** council-rust-pgrx, council-index-storage (semântica adversarial), council-benchmark (paralelos, código/artefatos reais).

## Verdict: READY_TO_MERGE

Sem BLOCKER. 1 MEDIUM de correção (bpchar) — **corrigido e provado**. 1 MEDIUM de doc (q17) + LOWs de doc — corrigidos.
O argumento semântico central foi PROVADO correto por fonte primária para text/varchar.

## Achados e resolução

### MEDIUM (index-storage) — `bpchar` (OID 1042) divergiria em contagem — CORRIGIDO
- **Defeito:** o guard de determinismo equaciona "collation determinística" com "byte-equality = PG-equality", falso
  para `bpchar`: o `bpchareq` apara espaços à direita (semântica de TIPO, `varchar.c:756-773`), então `'ab'`=`'ab '`
  no PG mas byte-diferentes → o hash byte-wise do DataFusion contaria a mais. Pré-existente (bpchar já roteava via
  AGG_HASHED) e afetava TAMBÉM o M154 (COUNT(DISTINCT bpchar), mesmo gate). Não exercitado pelo ClickBench (só text/varchar).
- **Fix:** removido `1042` de `arrow_supported_group_type` (`df_executor.rs:141`) → `GROUP BY bpchar` E
  `COUNT(DISTINCT bpchar)` declinam ao nativo. **Provado (EC-6):** ambos mostram plano nativo + A/B 2=2 (byte-wise daria 4/3).
  Fecha o buraco para M153 e M154 de uma vez. char(n)-com-tamanho (padded, seria seguro) é indistinguível por OID → exclusão conservadora.

### Argumento semântico central — PROVADO correto (index-storage, fonte primária)
- **Ordem (guard 2 — re-sort acima):** o CONJUNTO de linhas emitido == o do GroupAgg nativo (sob determinismo), e o
  `Sort` pai é o MESMO nó em ambos os planos → mesma saída ordenada. `parent==T_Sort` direto, fail-closed; `IncrementalSort`
  corretamente excluído (depende de pré-ordenação). Todo outro pai order-consumidor (MergeJoin/Unique/SetOp/GatherMerge/MergeAppend) declina.
- **Contagem (guard 1 — determinismo):** `varlena.c:1617-1628` — sob collation determinística, o desempate `memcmp`
  garante que byte-diferente NUNCA é reportado igual (inclui normalização Unicode). Logo o hash byte-keyed parte os grupos
  como o PG. Cobre a chave composta de q16 (checado por-coluna) e o valor-chave emitido (bytes exatos).
- **Empates no `Sort(count DESC)`:** não-determinismo INERENTE do PG (tuplesort instável), não regressão; o oráculo
  `run_m128` remove o LIMIT + canonicaliza order-insensitive → mede a invariante certa (igualdade de conjunto).

### rust-pgrx — LIMPO (sem BLOCKER/HIGH/MEDIUM)
- Parent-threading correto em TODOS os caminhos (o hipotético bug do `swap_walk_list` foi refutado — passa o próprio
  Append como pai, que É o pai real do membro). Numérico byte-preservado (else branch verbatim). Guard de collation no
  admit cobre AGG_HASHED **e** AGG_SORTED (fecha bug latente do HASHED-texto não-determinístico). Sem panic-atravessa-C/UB/UAF.
- INFO: `get_collation_isdeterministic` elog-on-bad-OID = mesma classe pré-existente já aceita (`get_ordering_op_properties`), não dispara.

### MEDIUM (benchmark, doc) — narrativa da q17 errada — CORRIGIDO
- O doc confundiu q17 com q12. q17 real = `GROUP BY UserID, SearchPhrase LIMIT 10` (sem ORDER BY/sem WHERE) → declina
  pelo **guard de ordem** (sem Sort pleno acima = o caso EC-3), a demonstração mais limpa do guard. Corrigido no doc.
- LOWs (doc) corrigidos: breakdown 21 = 16 agg + 5 projeção restabelecido; frase conectando set-equality ↔ guard de ordem
  (o A/B order-blind prova o conjunto porque as roteadas têm Sort acima; o EC harness prova o guard de ordem); cross-ref do contrato bounded-work_mem M154/M100.
- Manchetes auditadas HONESTAS: 21, +3 (q16/q33/q38 100% agg), diverged=0 em DOIS regimes, ablação 32× (isolação-de-lever correta, mesmo binário), q17 honestamente não-roteada.

## Hard gates (cycle-review)
- Testes: A/B diverged=0 (head+systematic); EC guards todos corretos. Sem secrets. Sem commit em main. Sem trailer Co-Authored-By.
  CHANGELOG `[Unreleased]` (Added M153 + Fixed bpchar). ✓

## Evidência
- `docs/benchmarks/m153-groupby-text.md` + `docs/benchmarks/m153-artifacts/{m153_agg_head.json, m153_agg_sys.json, m153_ec_guards.txt}` (com EC-6 bpchar)
- `benchmarks/m153_ec_harness.sql` (reprodutível). Fonte primária: `references/postgres/src/backend/utils/adt/{varlena.c,varchar.c}`.
