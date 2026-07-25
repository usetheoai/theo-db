# Discovery Plan: Rotear predicados de texto no WHERE ao CustomScan colunar (M156)

**Slug:** columnar-text-where-pushdown
**Owner:** paulohenriquevn
**Created:** 2026-07-25
**Time budget:** 5h (per-project breakdown in ADR D1)

## Context

O M152 (routing-map, `docs/benchmarks/m152-routing-map.md`) mediu `unpushable_where_qual` como o MAIOR first-blocker das
não-roteadas (8 queries: q12,14,20,27,30,31,36,37) — predicados de **texto** no WHERE (`SearchPhrase <> ''`,
`URL LIKE '%…%'`, regex). O nosso `extract_zone_predicate` (`theodb_rs/src/am/columnar_agg.rs:160`) só serializa
predicados NUMÉRICOS (const em `u64` zone-bits — `ZonePredicate`); texto (comprimento variável) não cabe. O
`build_filter_expr` (`df_executor.rs:269`) constrói o filtro DataFusion só desses. O M151 marcou "serialização de
const-texto no custom_private" como honest-negative adiado — agora é o M156, a maior fatia de cobertura. O
`ClickHouse/pg_clickhouse` (Apache-2.0, FDW) tem a lógica canônica de shippability + collation-safety de predicados; o
guard de collation determinística que já validamos no M153/M154 (`get_collation_isdeterministic`) precisa da fonte
canônica antes do M156. Cita `rules/discover-phd-rigor.md` (R0 web + acervo) e `rules/architecture.md` (fronteiras do CustomScan).

## Objective

Produzir um blueprint que responda **como serializar um predicado de texto (`=`/`<>`/LIKE) no `custom_private` e
construir o filtro DataFusion equivalente, com os guards de correção (collation determinística; semântica LIKE/regex),
byte-idêntico ao PG** — escopo suficiente para o M156 rotear as ~4-8 queries text-WHERE do ClickBench. Sucesso medível:
o blueprint dá o desenho concreto (formato de serialização + expr DataFusion + guards) com ≥2 fontes primárias por técnica.

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

- `.claude/knowledge-base/references/pg_clickhouse/` (FDW, Apache-2.0): `src/shipable.c`, `src/deparse.c`, `test/`.
- `.claude/knowledge-base/references/datafusion/`: `datafusion/physical-expr/src/expressions/like.rs`, `datafusion/functions/src/regex/`, `datafusion/functions/src/string/`.
- `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf` (pushdown + late materialization de string).

### Out-of-Scope (explicit)

- `pg_clickhouse/vendor/`, `pg_clickhouse/doc/`, `pg_clickhouse/docker-*`, `pg_clickhouse/sql/` (DDL do FDW — irrelevante ao CustomScan próprio).
- Todo o datafusion fora de filtro/string/regex.
- `deparse.c` verbatim como CÓDIGO (emite SQL de texto; nós emitimos `Expr` — ver ADR D2).

## ADRs

### D1 — Time budget + stop conditions

- pg_clickhouse: 3h (o goldmine — shipable.c/deparse.c/test). datafusion: 1h (checagem de API). acervo/web: 1h.
- Stop por questão: resposta cita `arquivo:linha` real OU `[BLOCKED]` com razão. Sem varredura além do budget.

### D2 — Extrair o PADRÃO, não o código (pg_clickhouse é FDW, nós somos CustomScan)

- Estudar o `shipable.c` pela LÓGICA de shippability/collation-safety (reusável); NÃO copiar `deparse.c` verbatim (ele
  emite SQL de texto remoto; nós emitimos `Expr` do DataFusion + serializamos bytes em `custom_private`). D1 (Apache-2.0)
  permite reuso de padrão, mas o projeto reimplementa do zero (Regra 9 aplicada ao PADRÃO, não ao código). O valor está
  na decisão "este qual é seguro?" (collation/tipo/operador), não na string SQL.

## Research Questions

| # | Question | Corner | Reference | Fase A (structural method) | Fase B (read method) | Answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o `foreign_expr_walker`/`shipable.c` decide que um qual de TEXTO é empurrável e guarda a collation-safety (empurra só se collation default/determinística)? | techniques | `.claude/knowledge-base/references/pg_clickhouse/src/shipable.c` | `grep -nE "collation|inputcollid|foreign_expr_walker|OpExpr" src/shipable.c` | Read as funções de walker + a checagem de collation; WEB (R0): postgres `foreign_expr_walker` collation-safety | Regra exata de collation-safety + confirmação de que casa o guard M153/M154 |
| Q2 | Como o `deparse.c` serializa um `Const` de texto + um padrão LIKE (`~~`) — quoting/escaping/mapeamento de operador? | techniques | `.claude/knowledge-base/references/pg_clickhouse/src/deparse.c` | `grep -nE "deparseConst|LIKE|~~|quote|escape" src/deparse.c` | Read `deparseConst` + o tratamento de LIKE | Padrão de serialização de const-texto (informa o formato `custom_private` de bytes variáveis) |
| Q3 | Qual o DELTA no NOSSO caminho — carregar um const de texto (bytes variáveis) no `custom_private` (hoje só `u64`) + construir o `Expr` de filtro DataFusion sobre a coluna Utf8? | techniques | `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf` | Read `theodb_rs/src/am/columnar_agg.rs:160,262,298` + `df_executor.rs:269` + `zonemap.rs` | Read o paper MonetDB/X100 (pushdown+late materialization de string); WEB (R0): pushdown de string em engines colunares | Desenho do delta: formato de serialização texto + braço `Expr::Like`/`BinaryExpr` + gate operador/collation |
| Q4 | Qual módulo/função do DataFusion 54 dá o filtro `LIKE`/`=`/`<>`/regex sobre Utf8, e a semântica casa SQL LIKE + operadores PG (RE2 vs POSIX)? | deps | `.claude/knowledge-base/references/datafusion/datafusion/physical-expr/src/expressions/like.rs` | `grep -rnE "like|Expr::Like|regexp" datafusion/functions/src/regex/ datafusion/physical-expr/src/expressions/like.rs` | Read `like.rs` + `functions/src/regex`; WEB (R0): docs DataFusion LIKE/regexp_match | API concreta + nota de semântica (LIKE seguro; regex RE2≠POSIX → declina, confirma M152) |
| Q5 | Como o `pg_clickhouse/test/` verifica a CORREÇÃO do pushdown de texto/LIKE/collation — qual o oráculo? | tests | `.claude/knowledge-base/references/pg_clickhouse/test/` | `ls .claude/knowledge-base/references/pg_clickhouse/test/ ; grep -rniE "like|collate|<>|pushdown" pg_clickhouse/test/` | Read os testes de WHERE/LIKE/collation | Descrição do oráculo + mapeamento ao nosso A/B (`run_m128` limit-strip+canonicalize) |
| Q6 | Como o `shipable.c` MANTÉM a whitelist de operadores + o override dos regex (`~`,`!~`,`~*`,`!~*`)? | tools | `.claude/knowledge-base/references/pg_clickhouse/src/shipable.c` | `grep -nE "custom_operator|is_shippable|builtin|~" src/shipable.c` | Read `chfdw_is_shippable` + `chfdw_check_for_custom_operator` | Padrão de whitelist/override → mapear ao nosso gate (aceitar `=`/`<>`/LIKE, declinar regex) |

## Coverage Matrix

| Coverage Corner | Questions | Status |
|---|---|---|
| Integration tests | Q5 | Covered |
| Dependencies | Q4 | Covered |
| Tools | Q6 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

Z = 100% — nenhum corner vazio.

## Halt-loop Checkpoints

- Antes de marcar uma questão DONE: a resposta cita `arquivo:linha` real (resolve no disco) OU `[BLOCKED]` com razão.
- Q4: a semântica regex (RE2≠POSIX) DEVE ser resolvida (empurra ou declina) — não deixar ambígua.
- Q3: o delta DEVE ter o formato de serialização concreto + o braço DataFusion, não "a definir".

## Acceptance Criteria

- [ ] Toda questão respondida com ≥1 citação `references/…:linha` que resolve (Q1/Q2/Q5/Q6 pg_clickhouse; Q4 datafusion).
- [ ] R0 honrado: ≥1 fonte web citada nos métodos de Q1/Q3/Q4 (`rules/discover-phd-rigor.md`).
- [ ] O blueprint dá o desenho do M156: formato de serialização texto + expr DataFusion + guards (collation determinística; `=`/`<>`/LIKE seguros, regex declina).
- [ ] 4 coverage corners populados; ≥1 ADR no blueprint sintetiza as decisões.

## Global Definition of Done

- [ ] `/discover-plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS (sem citação fabricada, corners não-vazios, ≤15 questões).
- [ ] `/discover-execute` produz `knowledge-base/discoveries/blueprints/columnar-text-where-pushdown-blueprint.md`.
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS.
- [ ] ADRs referenciam ≥1 princípio das rules (Regra 9 / KISS) ou arquivo de rule (`architecture.md`, `discover-phd-rigor.md`).
