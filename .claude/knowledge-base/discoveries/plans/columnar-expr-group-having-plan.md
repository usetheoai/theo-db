# Discovery Plan: Rotear GROUP BY por expressão + HAVING ao CustomScan colunar (M157)

**Slug:** columnar-expr-group-having
**Owner:** paulohenriquevn
**Created:** 2026-07-25
**Time budget:** 5h (breakdown in ADR D1)

## Context

O M152 (routing-map, `docs/benchmarks/m152-routing-map.md`) mediu, além do text-WHERE (fechado no M156), duas classes
restantes de bloqueio: **GROUP BY por EXPRESSÃO** (chave de grupo que é `date_trunc`/`EXTRACT`/`CASE`, não um `Var`) e
**HAVING** (predicado pós-agregação). Hoje `classify_target_node` (`theodb_rs/src/am/columnar_agg.rs:490`) só admite
uma chave de grupo `T_Var`; qualquer chave-expressão ou HAVING declina. O M156 provou o mecanismo do 2º canal
`custom_private` (nós serializáveis) + o filtro DataFusion; o M157 estende esse mecanismo para chaves de grupo que são
expressões e para o filtro pós-agregação. O território GENUINAMENTE NOVO vs M156 é: (a) como o planner do PG representa
uma chave de grupo não-Var no nó `Agg`; (b) a semântica do `date_trunc` do DataFusion vs o PG sob o GUC `TimeZone`
(risco de divergência byte-wise, a mesma classe do cross-type temporal que o review M151 pegou); (c) como o HAVING
aparece no plano e se é representável no DataFusion. Cita `rules/discover-phd-rigor.md` (R0 web + acervo) e
`rules/architecture.md` (fronteiras do CustomScan).

## Objective

Produzir um blueprint que responda **como admitir e serializar uma chave de grupo por expressão (`date_trunc`/`EXTRACT`/
`CASE`) e um predicado HAVING, construindo o group-expr + o post-agg-filter equivalentes no DataFusion byte-idênticos ao
PG — ou declinando fail-closed quando a semântica (timezone do `date_trunc`, unificação de tipo do `CASE`) diverge** —
escopo suficiente para o M157 rotear as ~5-8 queries expr-group/HAVING do ClickBench que o M152 mediu.

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

- `.claude/knowledge-base/references/datafusion/datafusion/functions/src/datetime/date_trunc.rs` (+ `date_part.rs`) e o `logical-expr` (Aggregate + Filter).
- `.claude/knowledge-base/references/postgres/src/backend/utils/adt/timestamp.c` (`date_trunc` + timezone) e `src/backend/optimizer/plan/createplan.c` (representação da chave de grupo no `Agg`).
- `.claude/knowledge-base/discoveries/blueprints/columnar-text-where-pushdown-blueprint.md` (o 2º canal, prior art interno).
- `docs/benchmarks/m152-routing-map.md` (quais queries são expr-group/HAVING).

### Out-of-Scope (explicit)

- `references/datafusion` fora de `functions/src/datetime/` + `logical-expr` (Aggregate/Filter).
- `references/postgres` fora de `timestamp.c`/`createplan.c`/`planagg.c`.
- GROUPING SETS / ROLLUP / CUBE / window functions (fora do escopo — só GROUP BY simples por expressão + HAVING).
- `EXTRACT`/`CASE` além do que o M152 mediu no ClickBench (não generalizar — YAGNI).

## ADRs

### D1 — Time budget + stop conditions

- postgres: 2h (representação da chave de grupo no Agg + timezone do date_trunc). datafusion: 2h (date_trunc/CASE group-expr + HAVING). acervo/web (R0): 1h.
- Stop por questão: resposta cita `arquivo:linha` real OU `[BLOCKED]` com razão. Sem varredura além do budget.

### D2 — Extrair o PADRÃO do 2º canal (reusar M156), não reinventar

- O M156 já provou o 2º canal `custom_private` de nós serializáveis + o braço de filtro DataFusion. O M157 REUSA esse
  mecanismo (Regra 9): a chave-expressão e o HAVING viajam pelo mesmo canal (nós `Const`/`FuncExpr` copyObject-safe ou
  a expr reconstruída no exec). NÃO reinventar um novo canal. O valor da pesquisa é a decisão de correção (timezone/
  tipo), não a mecânica de serialização (já resolvida).

## Research Questions

| # | Question | Corner | Reference | Fase A (structural method) | Fase B (read method) | Answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o planner do PG representa uma chave de grupo que é uma EXPRESSÃO (`date_trunc(ts)`) no nó `Agg` — `grpColIdx` aponta p/ um `TargetEntry` cuja `expr` é um `FuncExpr`/`CaseExpr`, não um `Var`? | techniques | `.claude/knowledge-base/references/postgres/src/backend/optimizer/plan/createplan.c` | `grep -nE "grpColIdx|numCols|GROUP BY|make_agg|Agg" createplan.c` | Read a construção do nó Agg + grpColIdx | Estrutura exata da chave-expressão no Agg → informa o que `classify_target_node` deve reconhecer |
| Q2 | Qual o DELTA no NOSSO caminho — como `classify_target_node`/`build_admission` (`columnar_agg.rs:490`) admite uma chave `FuncExpr`/`CaseExpr` + serializa via 2º canal (M156) + reconstrói como group-expr do DataFusion? | techniques | `.claude/knowledge-base/discoveries/blueprints/columnar-text-where-pushdown-blueprint.md` | Read `columnar_agg.rs:411-560` (Admitted/classify/build_admission) + o 2º canal do M156 | Read o blueprint M156 (serialização de nós) | Desenho do delta: admissão da chave-expr + serialização + group-expr no DataFusion |
| Q3 | Como o HAVING aparece no plano do PG (um qual/`Filter` acima do `Agg`) e é representável como `.aggregate(...).filter(...)` no DataFusion, ou deve declinar? | techniques | `.claude/knowledge-base/references/postgres/src/backend/optimizer/plan/createplan.c` | `grep -nE "qual|Having|Filter|Result" createplan.c` | Read a colocação do HAVING no plano + o `logical-expr` do DataFusion | Regra: HAVING empurrável (filtro sobre agregados) vs declina |
| Q4 | Qual a API do DataFusion 54 p/ `date_trunc`/`EXTRACT`/`CASE` como GROUP-expr + o filtro HAVING sobre saídas de agregado, e a semântica do `date_trunc` (timezone) casa o PG? | deps | `.claude/knowledge-base/references/datafusion/datafusion/functions/src/datetime/date_trunc.rs` | `grep -nE "date_trunc|timezone|tz|Timestamp" date_trunc.rs` | Read `date_trunc.rs` + `logical-expr` Aggregate/Filter; WEB (R0): docs DataFusion date_trunc/GROUP BY expr | API concreta + nota de semântica timezone (casa ou declina) |
| Q5 | Qual a regra EXATA do `date_trunc` do PG sob o GUC `TimeZone` (`timestamp` vs `timestamptz`) que devemos casar ou declinar? | tools | `.claude/knowledge-base/references/postgres/src/backend/utils/adt/timestamp.c` | `grep -nE "date_trunc|timestamptz|session_timezone|DecodeTimezone" timestamp.c` | Read `timestamp_trunc`/`timestamptz_trunc`; WEB (R0): docs PG date_trunc timezone | Regra timezone exata → o guard de decline (timestamptz sob TZ não-UTC declina, como o cross-type temporal do M151) |
| Q6 | Quais queries do ClickBench são expr-group/HAVING (o alvo do M157) e como o oráculo A/B (`run_m128` limit-strip+canonicalize) verifica a correção? | tests | `docs/benchmarks/m152-routing-map.md` | `grep -nE "date_trunc|EXTRACT|CASE|HAVING|GROUP BY" docs/benchmarks/m152-routing-map.md` + `benchmarks/run_m128_clickbench.py` | Read o routing-map + o oráculo A/B | Lista das queries-alvo + mapeamento ao A/B |

## Coverage Matrix

| Coverage Corner | Questions | Status |
|---|---|---|
| Integration tests | Q6 | Covered |
| Dependencies | Q4 | Covered |
| Tools | Q5 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

Z = 100% — nenhum corner vazio; techniques ≥ 2 (perfil frontier, `discover-phd-rigor.md`).

## Halt-loop Checkpoints

- Antes de marcar uma questão DONE: a resposta cita `arquivo:linha` real (resolve no disco) OU `[BLOCKED]` com razão.
- Q4/Q5: a semântica timezone do `date_trunc` DEVE ser resolvida (casa OU declina) — não deixar ambígua (é o risco ALTA).
- Q2: o delta DEVE ter o mecanismo concreto (admissão + serialização + group-expr), não "a definir".

## Acceptance Criteria

- [ ] Toda questão respondida com ≥1 citação `references/…:linha` que resolve (Q1/Q3/Q5 postgres; Q4 datafusion; Q2 blueprint).
- [ ] R0 honrado: ≥1 fonte web citada nos métodos de Q4/Q5 (`rules/discover-phd-rigor.md`).
- [ ] O blueprint dá o desenho do M157: admissão da chave-expr + HAVING + group-expr/post-filter no DataFusion + guards (timezone do date_trunc, tipo do CASE).
- [ ] 4 coverage corners populados; ≥1 ADR no blueprint sintetiza as decisões.

## Global Definition of Done

- [ ] `/discover-plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS (sem citação fabricada, corners não-vazios, ≤15 questões).
- [ ] `/discover-execute` produz `knowledge-base/discoveries/blueprints/columnar-expr-group-having-blueprint.md`.
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS.
- [ ] ADRs referenciam ≥1 princípio/arquivo de rule (`architecture.md`, `discover-phd-rigor.md`).
