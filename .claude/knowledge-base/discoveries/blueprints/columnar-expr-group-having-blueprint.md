# Blueprint: Rotear GROUP BY por expressão (`date_trunc`) + HAVING ao CustomScan colunar (M157)

**Slug:** columnar-expr-group-having
**Cycle:** discover (execute) — 2026-07-25
**Plan:** `.claude/knowledge-base/discoveries/plans/columnar-expr-group-having-plan.md`
**Owner:** paulohenriquevn

## Objective

Dar o desenho concreto do M157: **como admitir e serializar uma chave de grupo por expressão (`date_trunc`) e
construir o group-expr equivalente no DataFusion byte-idêntico ao PG — declinando fail-closed quando a semântica
diverge (`date_trunc` sob `timestamptz`+TimeZone, `CASE`/`EXTRACT` com unificação de tipo)** — suficiente para o
M157 rotear a ÚNICA query ClickBench genuinamente-alcançável desta classe (q42, `date_trunc('minute', EventTime)`).
**Critério de sucesso:** o blueprint diz o que ADMITE (date_trunc sobre `timestamp` sem tz), o que SERIALIZA (o
descritor de group-expr no 2º canal do M156) e o que DECLINA com razão medida (timestamptz, HAVING, CASE, EXTRACT,
const, aritmética) — com o guard de timezone provado por fonte primária, não por observação de um A/B TZ=UTC.

## Context

O M152 (`docs/benchmarks/m152-routing-map.md:22,25,66`) mediu, além do text-WHERE (fechado no M156), duas classes
restantes: **GROUP BY por EXPRESSÃO** (`target_grouping_expression_or_other` — q18,34,35,39,42) e **HAVING** (q27,q28).
Hoje `admit` (`theodb_rs/src/am/columnar_agg.rs:687-695`) declina qualquer query com `havingQual`, e
`classify_target_node` (`columnar_agg.rs:490`) só admite uma chave de grupo `T_Var`; `admit:734-736` declina GROUP BY
cuja chave não é um `Var` puro. O M156 já provou o mecanismo do **2º canal `custom_private`** (nós serializáveis:
`encode_text_preds`, `columnar_agg.rs:1007-1019`; decode em `begin_custom_scan`, `columnar_agg.rs:1322`) + o filtro
DataFusion. O M157 estende esse mecanismo para uma **chave-expressão** e (opcionalmente) o filtro pós-agregação.

O território GENUINAMENTE NOVO vs M156 é a **CORREÇÃO**, não a mecânica: (a) a representação da chave não-`Var` no
target; (b) a semântica do `date_trunc` do DataFusion vs o PG sob o GUC `TimeZone` — o **maior risco de divergência
byte-wise**, a mesma classe do cross-type temporal que o review M151 pegou (memória `m151-datafusion-coverage-released`);
(c) a realidade de **cobertura composta** do M152 (lição M155 `m155-topn-honest-negative`: implementar um lever cuja
cobertura medida é 0 não roteia nada).

**Escopo honesto (o que empurra vs. declina) — ancorado nas queries REAIS do ClickBench (`benchmarks/clickbench/theodb/queries.sql`):**

| Shape (query) | Node no target | M157 | Razão |
|---|---|---|---|
| `date_trunc('minute', EventTime)` (q42, `queries.sql:43`) | `FuncExpr` sobre `timestamp` (1114) | **empurra** | `timestamp` sem tz → `date_trunc` é timezone-independente em AMBOS (prova abaixo) |
| `date_trunc(unit, timestamptz)` (hipotético) | `FuncExpr` sobre `timestamptz` (1184) | **DECLINA (honest-negative)** | PG trunca sob `TimeZone` GUC; DataFusion trunca em UTC/config-própria → diverge sob TZ≠UTC (invisível a um A/B TZ=UTC) |
| `extract(minute FROM EventTime)` (q18, `queries.sql:19`) | `FuncExpr` `date_part` → numeric | **DECLINA** | saída `numeric` → tipo de group-key não suportado (`arrow_supported_group_type` exclui numeric) — morre antes do timezone |
| `CASE WHEN … THEN Referer ELSE '' END` (q39, `queries.sql:40`) | `CaseExpr` → text | **DECLINA (escopo)** | condição `WHEN` = predicado arbitrário (reabre o problema WHERE) + risco de unificação de tipo cross-branch |
| `GROUP BY 1, URL` — o `1` (q34, `queries.sql:35`) | `Const` no target | **DECLINA** | `T_Const` não é `Var` nem `Aggref` → `classify_target_node` retorna `None` |
| `ClientIP - 1` (q35, `queries.sql:36`) | `OpExpr` (subtração) | **DECLINA (escopo)** | aritmética arbitrária no group-key — fora do escopo (YAGNI; M152 fatia menor) |
| `… HAVING COUNT(*) > 100000` (q27, `queries.sql:28`) | qual acima do `Agg` | **DECLINA (measured honest-negative)** | q27 morre **independente** em `AVG(length(URL))` (agg-sobre-expressão); q28 em `REGEXP_REPLACE` — HAVING-sozinho = cobertura 0 |

---

## Coverage Corner 1 — Integration Tests

**Q6 — Quais queries do ClickBench são expr-group/HAVING (o alvo do M157) e como o oráculo A/B verifica a correção?**

O M152 (`docs/benchmarks/m152-routing-map.md:22`) marca `target_grouping_expression_or_other` = **q18,34,35,39,42**
(5 queries) e (`:25,:66`) **HAVING** = **q27,q28**. Lendo os SQLs REAIS (`benchmarks/clickbench/theodb/queries.sql`),
a decomposição por shape corrige o mapa (a mesma lição M152 "instrumentar razões > adivinhar por SQL"):

| # (0-idx) | Linha | SQL (resumo) | Classe real |
|---|---|---|---|
| q42 | `queries.sql:43` | `DATE_TRUNC('minute', EventTime) AS M, COUNT(*) … GROUP BY DATE_TRUNC('minute', EventTime) ORDER BY DATE_TRUNC(…)` | **date_trunc(timestamp) — O ALVO** |
| q18 | `queries.sql:19` | `UserID, extract(minute FROM EventTime) AS m, … GROUP BY UserID, m` | EXTRACT → numeric (declina) |
| q39 | `queries.sql:40` | `CASE WHEN (SearchEngineID=0 AND AdvEngineID=0) THEN Referer ELSE '' END AS Src, … GROUP BY … Src, Dst` | CASE→text (declina, escopo) |
| q34 | `queries.sql:35` | `SELECT 1, URL, COUNT(*) … GROUP BY 1, URL` | Const target (declina) |
| q35 | `queries.sql:36` | `ClientIP, ClientIP-1, … GROUP BY ClientIP, ClientIP-1, …` | aritmética (declina, escopo) |
| q27 | `queries.sql:28` | `CounterID, AVG(length(URL)) AS l, COUNT(*) AS c … GROUP BY CounterID HAVING COUNT(*) > 100000` | HAVING **+ AVG(length(URL))** |
| q28 | `queries.sql:29` | `REGEXP_REPLACE(Referer,…) AS k, AVG(length(Referer)), … GROUP BY k HAVING COUNT(*) > 100000` | HAVING **+ REGEXP + AVG(expr)** |

**Achado de cobertura (adversarial, o mais importante):** das 7 queries desta classe, **apenas q42 é
genuinamente-alcançável**. As duas queries HAVING têm um **first-blocker INDEPENDENTE do HAVING**: q27 usa
`AVG(length(URL))` (agg-sobre-expressão — `agg_over_expression`, `columnar_agg.rs:553`) e q28 usa `REGEXP_REPLACE`
como group-key + `AVG(length(Referer))`. Logo **implementar HAVING sozinho roteia ZERO queries** — exatamente a lição
M155 (`m155-topn-honest-negative`: cobertura 0 → não rotear; esforço≠complexidade) e M152 (bloqueios compostos →
marginal 2-4 por fatia). O lever real desta classe é **date_trunc-group (q42)**, não HAVING.

**Tipo de `EventTime` (o fato que decide o guard):** `benchmarks/clickbench/theodb/create.sql:8` —
`EventTime TIMESTAMP NOT NULL` (SEM time zone). Portanto q42 é `date_trunc` sobre `timestamp` (1114) →
**timezone-independente → byte-safe** (prova no Corner 3). `EventDate` é `Date` (`create.sql:9`); os predicados
`EventDate >= '…'` do WHERE de q42 são zone-preds temporais já pushados (columnar-zonemap, memória).

**O oráculo A/B (`benchmarks/run_m128_clickbench.py`):** para cada query, `EXPLAIN (FORMAT TEXT)` (prova o CustomScan
colunar — `run_m128:167`) + a MESMA agregação FULL (limit-strip) rodada sobre `hits` (colunar) e `hits_heap`
(`run_m128:178-181`), canonicalizada order-insensitive (`_canonical`, `run_m128:141-144`) e exigindo `diverged=0`
(`run_m128:244,251-252`). **Casos negativos obrigatórios** (`.claude/rules/testing.md` §4.1) que um A/B TZ=UTC NÃO
pega e que o teste do M157 DEVE forçar:
1. Uma tabela com coluna `timestamptz` + `date_trunc('day', tz_col)` sob `SET TimeZone='America/Sao_Paulo'` → o
   CustomScan DEVE **declinar** (cair no nativo); o A/B roda com TZ não-UTC e exige `diverged=0` — provando que a
   decisão é por TIPO estático, não por observação.
2. `date_trunc('minute'|'hour'|'day'|'month'|'quarter'|'year', ts_sem_tz)` → `diverged=0` (a whitelist de
   granularidade do Corner 3).
3. `date_trunc('decade'|'century'|'millennium', …)` → declina (granularidade fora do DataFusion — Corner 3).
4. Edge: `EventTime` com valores no boundary exato do minuto; NULL timestamp (`date_trunc(NULL)` → NULL group).

---

## Coverage Corner 2 — Dependencies

**Q4 — API do DataFusion p/ `date_trunc`/`CASE` como GROUP-expr + filtro HAVING sobre agregados, e a semântica do
`date_trunc` casa o PG?**

- **`date_trunc` como group-expr:** DataFusion expõe `date_trunc(unit, ts)` como `ScalarUDF`
  (`.claude/knowledge-base/references/datafusion/datafusion/functions/src/datetime/date_trunc.rs`). No nosso caminho
  NÃO chamamos o parser SQL — construímos o `Expr` diretamente. Hoje `run_columnar_grouped_aggs`
  (`theodb_rs/src/am/df_executor.rs:460`) faz `group_exprs = group_cols.iter().map(|(n,_)| col(n))` — chaves = colunas
  puras. O DELTA: para a chave-expr, o group_expr vira uma **`Expr` de função** — `date_trunc(lit(unit), col(base))` via
  o builder de função escalar do DataFusion (`datafusion::functions::expr_fn::date_trunc`, o mesmo namespace de
  `col`/`lit` já usados em `df_executor.rs`). O `.aggregate(group_exprs, agg_exprs)` (`df_executor.rs:476`) aceita
  QUALQUER `Expr` como chave de grupo (não só `col`) — é a mesma API, group-expr é gratuito.
- **Granularidades suportadas** (`date_trunc.rs:57-85`, `DateTruncGranularity` + `SUPPORTED_GRANULARITIES`):
  `microsecond, millisecond, second, minute, hour, day, week, month, quarter, year`. Uma unidade fora disso →
  `exec_err!` (`date_trunc.rs:101-106`) em RUNTIME → **DECLINAR no admit** as granularidades que o DataFusion não tem
  (PG tem `decade/century/millennium` a mais — `timestamp.c:4881-4913`). Ver Corner 3 para o casamento exato.
- **Tipo de retorno:** `date_trunc.rs:254-266` (`return_type`/`return_field_from_args`) → para input sem tz,
  `Timestamp(Nanosecond, None)`. PG `timestamp_trunc` retorna `timestamp` (1114, precisão µs). Como o input é µs, o
  valor truncado é sempre múltiplo de 1000 ns → a conversão Arrow(ns)→Datum(µs) em `arrow_value_to_datum` é exata
  (MUST-verify pelo A/B: `arrow_value_to_datum` DEVE ler a UNIT Arrow real, não assumir µs).
- **HAVING como filtro pós-agregação:** o DataFusion tem `.filter(expr)` sobre um `DataFrame` já agregado — o M157
  faria `df.aggregate(group_exprs, agg_exprs)?.filter(having_expr)?`, onde `having_expr` referencia o alias `a{k}` do
  agregado (`push_agg_exprs`, `df_executor.rs:206-227`). API disponível (mesmo `.filter` já usado no WHERE,
  `df_executor.rs:473`). **Mas** — ver ADR D3 — a tradução de um HAVING arbitrário reabre o problema de tradução de
  predicado, e a cobertura medida é 0. Recomendação: NÃO no M157.
- **`CASE`:** o DataFusion tem `Expr::Case`; construível. O risco não é a API, é a **unificação de tipo cross-branch**
  (coerção do DataFusion ≠ coerção do PG `select_common_type`) + a condição `WHEN` ser um predicado arbitrário. Ver
  ADR D3 (decline no M157).

**Nenhuma dependência nova** (Regra 9 / parsimony-ladder rung 4): DataFusion/Arrow já estão no binário (M98/M100/M143);
`date_trunc`/`.filter`/`Expr::Case` são APIs já disponíveis. O delta é usar `date_trunc(lit,col)` como group-expr.

**WEB (R0, fetch ao vivo 2026-07-25):** DataFusion — *Scalar Functions* (`date_trunc` na lista de datetime; as funções
de data operam na *session time zone* configurada por `datafusion.execution.time_zone`, uma config do DataFusion, NÃO
o GUC do PG): https://datafusion.apache.org/user-guide/sql/scalar_functions.html (verificado: 797 KB, `date_trunc`
presente; texto extraído: *"Returns the current date in the session time zone … SET datafusion.execution.time_zone =
'+00:00'"*). Isto confirma que o "session time zone" do DataFusion é desacoplado do `TimeZone` do PG — a raiz da
divergência do timestamptz.

---

## Coverage Corner 3 — Tools

**Q5 — A regra EXATA do `date_trunc` do PG sob o GUC `TimeZone` (`timestamp` vs `timestamptz`) que devemos casar ou declinar.**

**Fonte primária local — `timestamp.c`:**

1. **`timestamp` (SEM tz), `timestamp_trunc`** (`.claude/knowledge-base/references/postgres/src/backend/utils/adt/timestamp.c:4621-4645`):
   ```c
   Datum timestamp_trunc(PG_FUNCTION_ARGS) {
       Timestamp timestamp = PG_GETARG_TIMESTAMP(1);
       ...
       if (timestamp2tm(timestamp, NULL, tm, &fsec, NULL, NULL) != 0)   // <-- tzp = NULL
   ```
   O `timestamp2tm` é chamado com `tzp = NULL` → **NENHUMA conversão de timezone**: o valor é quebrado em campos de
   parede (wall-clock) e truncado por campo (`timestamp.c:4650-4939`, o switch `DTK_*`). **`date_trunc` sobre
   `timestamp` é TIMEZONE-INDEPENDENTE** — o GUC `TimeZone` não entra.

2. **`timestamptz` (COM tz), `timestamptz_trunc_internal`** (`timestamp.c:4824-4850`):
   ```c
   /* Common code for timestamptz_trunc() and timestamptz_trunc_zone().
    * tzp identifies the zone to truncate with respect to. */
   timestamptz_trunc_internal(text *units, TimestampTz timestamp, pg_tz *tzp) {
       ...
       if (timestamp2tm(timestamp, &tz, tm, &fsec, NULL, tzp) != 0)     // <-- tzp = a zona
   ```
   O `timestamptz_trunc` (o `date_trunc(field, timestamptz)` de 2 args) passa `tzp = session_timezone` (o GUC
   `TimeZone`). O comentário `:4824-4826` é explícito: *"tzp identifies the zone to truncate with respect to."*
   **`date_trunc` sobre `timestamptz` DEPENDE do GUC `TimeZone`** — trunca no relógio de parede DAQUELA zona.

**WEB (R0, fetch ao vivo 2026-07-25) — docs oficiais do PG confirmam:**
https://www.postgresql.org/docs/current/functions-datetime.html — *"truncation is performed with respect to a
particular time zone; for example, truncation to day produces a value that is midnight in that zone. By default,
truncation is done with respect to the current TimeZone setting, but the optional time_zone argument can be
provided…"* (extraído do HTML ao vivo, 109 KB). Isto é literalmente a variante `timestamptz`.

**O casamento vs. DataFusion (o guard):** o `date_trunc.rs:301-329` (`process_array`) faz `parsed_tz = parse_tz(tz_opt)`
onde `tz_opt` vem da **timezone declarada da coluna Arrow**, não de um GUC do PG. Quando `parsed_tz.is_none()`
(`date_trunc.rs:314-315`) a truncação é aritmética UTC/naive. Nosso decode de uma coluna PG `timestamp` produz Arrow
`Timestamp(µs, None)` → `parsed_tz = None` → truncação naive = **idêntica ao `timestamp_trunc` (tzp=NULL) do PG**.
Mas nosso decode de um `timestamptz` (que o PG armazena internamente em UTC) produziria `Timestamp(µs, None|"UTC")` →
o DataFusion trunca em **UTC**, jamais no GUC `TimeZone` do PG. Sob `SET TimeZone='America/Sao_Paulo'`, o PG trunca
o dia às 00:00 de São Paulo (UTC-03), o DataFusion às 00:00 UTC → **valores de grupo diferentes** (fronteiras de dia
deslocadas 3h). Um A/B ClickBench (TZ=UTC, `EventTime` é `timestamp`) NUNCA exercita isso.

**Regra do guard (fail-closed):**
- ADMITE `date_trunc(unit_const, Var)` **sse e somente se** `vartype == 1114` (`timestamp` sem tz) E `unit` ∈
  whitelist de granularidade casada { `second, minute, hour, day, month, quarter, year` } — o subconjunto que existe
  em AMBOS (`date_trunc.rs:74-85` ∩ `timestamp.c` switch) e é provado byte-idêntico pelo A/B.
- DECLINA `timestamptz` (1184) INCONDICIONALMENTE (dependência do GUC `TimeZone`; DataFusion trunca em UTC/config →
  fail-wrong sob TZ≠UTC). Mesmo passar o GUC como tz do Arrow NÃO resolve: `date_trunc.rs:310-312` IGNORA o viés
  histórico não-minuto das zonas (ex.: Asia/Kathmandu pré-1919 UTC+05:41:16), enquanto o `pg_localtime` do PG usa a
  tzdata completa (DST + histórico) → divergiria em transições DST/históricas mesmo com tz anexada.
- DECLINA `week` (verify-first): PG usa ISO-week (`timestamp.c:4652-4657`, `date2isoweek`); o DataFusion trunca via
  chrono — provável casamento (ambos segunda-feira ISO), mas não provado byte-a-byte → fora do M157 até o A/B provar.
- DECLINA `microsecond/millisecond/decade/century/millennium`: `decade/century/millennium` inexistem no DataFusion
  (`exec_err!`, `date_trunc.rs:101-106`); micro/milli são triviais mas raras (YAGNI, adicionar quando o benchmark pedir).

Isto é a MESMA disciplina do M151 (`m151-datafusion-coverage-released`): "restringir à classe provadamente-segura >
relaxar amplo" — o review pegou um cross-type temporal que o A/B TZ=UTC não pegava; aqui a fonte primária (`timestamp.c`
+ docs PG) prova o timestamptz-decline ANTES do review.

---

## Coverage Corner 4 — Techniques

### Q1 — Como o planner do PG representa uma chave de grupo por EXPRESSÃO

O `create_agg_plan` (`.claude/knowledge-base/references/postgres/src/backend/optimizer/plan/createplan.c:2308-2344`)
monta o `Agg` com:
```c
tlist = build_path_tlist(root, &best_path->path);        // o target de saída: group-keys + Aggrefs
quals = order_qual_clauses(root, best_path->qual);       // <-- o HAVING (Q3)
plan = make_agg(tlist, quals, best_path->aggstrategy, best_path->aggsplit,
                list_length(best_path->groupClause),
                extract_grouping_cols(best_path->groupClause, subplan->targetlist),  // grpColIdx
                ...);
```
`grpColIdx` (via `extract_grouping_cols`) é um array de `AttrNumber` que aponta para posições no **targetlist do
FILHO** (`subplan->targetlist`). Quando a chave de grupo é uma EXPRESSÃO (`date_trunc(EventTime)`), o planner injeta a
expressão computada como um `TargetEntry` do filho (um `Result`/`SubqueryScan` ou o próprio scan projetado), e
`grpColIdx[i]` aponta para ESSA entry — cuja `expr` é um `FuncExpr`, não um `Var`. No **parse tree** (que é o que o
NOSSO `admit` lê, não o plano), o `output_rel->reltarget->exprs` (`columnar_agg.rs:712`) contém o `FuncExpr`
`date_trunc('minute', EventTime)` DIRETAMENTE como o nó do target. É aí que `classify_target_node` (`columnar_agg.rs:490`)
vê `T_FuncExpr` em vez de `T_Var` e hoje retorna `None` (via o `else`/decline implícito → `admit:734-736` "GROUP BY
with no bare-column key → native plan").

**Conclusão Q1:** a chave-expr aparece no parse-tree target como o próprio `FuncExpr`; não precisamos do `grpColIdx`
do plano (a arquitetura M115 re-deriva do parse tree, `columnar_agg.rs:742-743`). O que `classify_target_node` deve
reconhecer é um `FuncExpr` `date_trunc(Const-text, Var-timestamp)`.

### Q2 — O DELTA no NOSSO caminho (admissão + serialização + group-expr no DataFusion)

Reusa o PADRÃO do M156 (ADR D2 do plano; Regra 9), não reinventa canal. Três pontos de edição:

**(a) Admissão — novo tipo `GroupExpr` + ramo em `classify_target_node` (`columnar_agg.rs:485-518`).** Hoje o `TargetSlot`
(`columnar_agg.rs:441-444`) é `Group(i32,u32)` (attno, vartype) ou `Agg(ParsedAgg)`. O M157 adiciona um terceiro
sabor: `GroupExprSlot { base_attno: i32, func: GroupFunc, unit: String, out_typoid: u32 }` com
`enum GroupFunc { DateTrunc }`. No `classify_target_node`, ANTES do decline, um ramo `T_FuncExpr`:
1. `get_func_name((*fe).funcid)` == `"date_trunc"` (por NOME, não OID hardcoded — precedente D5, `columnar_agg.rs:204-234`);
2. 2 args: arg0 = `T_Const` text não-NULL (a unidade) ∈ whitelist de granularidade (Corner 3); arg1 = `T_Var` base-rel
   (`varno==relid`, `varattno>0`) de tipo **1114** (`timestamp` sem tz) — `1184` (timestamptz) → `admit_trace("group_expr_date_trunc_timestamptz"); return None` (o guard do Corner 3);
3. `out_typoid = 1114` (o retorno de `date_trunc(timestamp)`).
   Qualquer falha → `None` → declina (invariante `admit` preservado). O `group_cols` de hoje (`Vec<(i32,u32)>`) ganha
   um irmão `group_exprs: Vec<GroupExprSpec>`, e o `layout` (`columnar_agg.rs:411,720`) ganha um novo `kind` (2 = group-expr)
   além dos 0=group/1=agg — para o exec emitir na ordem do target (ADR-2 preservada).

**(b) Serialização — 3º sub-canal em `custom_private` (extensão natural do 2º canal M156).** Hoje `custom_private` é
`List[ IntList_numérica, List_text_preds ]` (`columnar_agg.rs:1145-1152`). O M157 acrescenta um 3º elemento
`List_group_exprs`, cada entry = `[ makeInteger(base_attno), makeInteger(func_code), makeString(unit), makeInteger(out_typoid) ]`
— EXATAMENTE o padrão de `encode_text_preds` (`columnar_agg.rs:1007-1019`: `makeInteger`+`makeString`, todos leaf
`Value` nodes, copyObject/out/read-safe). O decode em `begin_custom_scan` (`columnar_agg.rs:1322-1350`) lê o 3º canal
como já lê o 2º (`text_list`). **Nenhum novo mecanismo** — só um 3º canal irmão (Regra 9).

**(c) Group-expr no DataFusion (`df_executor.rs:460`).** Hoje `group_exprs = group_cols.iter().map(|(n,_)| col(n))`.
O M157 estende o vetor de group-exprs: para cada `GroupExprSpec { base_name, DateTrunc, unit, out_typoid }`, empurra
`date_trunc(lit(ScalarValue::Utf8(unit)), col(base_name))` em vez de `col`. O `.aggregate(group_exprs, agg_exprs)`
(`df_executor.rs:476`) aceita `Expr` arbitrário como chave. A materialização de volta (`df_executor.rs:485-491`) usa
o `out_typoid` (1114) para `arrow_value_to_datum` — a MESMA reversão Arrow→Datum já usada para group-keys `Var`
temporais, só com atenção à UNIT ns→µs (Corner 2, MUST-verify).

**Delta mínimo, byte-idêntico (Esforço≠Complexidade):** o esforço (novo TargetSlot + 3º canal + group-expr builder) é
bem-vindo porque a necessidade (rotear q42) é real; o COMO é a extensão mais simples do mecanismo M156 já provado —
zero abstração nova.

### Q3 — Como o HAVING aparece no plano e se é empurrável

**Representação (fonte primária):** `create_agg_plan` (`createplan.c:2324-2326`) coloca o HAVING como o **`qual` do
próprio nó `Agg`** (`quals = order_qual_clauses(root, best_path->qual)` → `make_agg(tlist, quals, …)`). No parse tree,
é `parse->havingQual` (`columnar_agg.rs:689`, onde hoje declinamos). Após `set_plan_refs`, o `qual` do `Agg` está em
forma pós-planejamento: os `Aggref` permanecem `Aggref` (avaliados pela máquina de agregação do próprio `Agg`) e as
referências a colunas de grupo viram `Var(OUTER_VAR)` para o targetlist do subplano.

**Por que o "empurre o qual do Agg pro CustomScan" é ERRADO:** o swap M115 (`try_swap_agg`, `columnar_agg.rs:1125-1131`)
DROPA o filho (`plan_out.lefttree = null`) e hoje zera o qual (`plan_out.qual = null`, `columnar_agg.rs:1130`). Se
copiássemos `(*agg).plan.qual` para o CustomScan, seus `Var(OUTER_VAR)`/`Aggref` referenciariam um subplano e uma
máquina de agregação que NÃO existem mais no CustomScan self-scanning → garbage/crash. Reusar o qual do PG exigiria
re-mapear cada `Aggref`/`OUTER_VAR` do qual para a coluna de saída do CustomScan (um walk de expressão dirigido) — não
trivial, e o próprio walk é uma superfície de tradução.

**Alternativa: empurrar HAVING ao DataFusion (`.filter` pós-`.aggregate`).** No parse-tree (limpo, ANTES do
set_plan_refs), `parse->havingQual` está em forma `Aggref`, casável contra os mesmos `Aggref` que já parseamos no
target. Um HAVING de shape estreito `Aggref_i OP Const` (OP ∈ {`=`,`<>`,`<`,`<=`,`>`,`>=`}) traduziria para
`col("a{k}").gt(lit(const))` sobre o alias do agregado (`push_agg_exprs`, `df_executor.rs:206-227`), com `.filter`
depois do `.aggregate` (`df_executor.rs:476`). Byte-idêntico porque o valor do agregado que emitimos JÁ é byte-idêntico
ao PG (invariante M114) e a comparação ordenada (`int8gt`/`float8gt`) casa a do PG.

**MAS — a decisão medida (honest-negative, Corner 1):** as DUAS queries HAVING do ClickBench morrem em blockers
INDEPENDENTES (q27: `AVG(length(URL))` agg-sobre-expressão; q28: `REGEXP_REPLACE` group-key + `AVG(expr)`). HAVING-
sozinho tem **cobertura medida 0**. Portanto **Q3 conclui: HAVING é tecnicamente empurrável (via DataFusion post-agg
filter, shape estreito), mas NÃO deve ser implementado no M157** (roteia zero queries; o lever é date_trunc). Se
algum dia for feito, o desenho acima é o correto (parse-tree Aggref → post-agg `.filter`, NÃO o qual pós-planejamento).

---

## Cross-cutting Comparison

O eixo é a CORREÇÃO (byte-identidade), não a mecânica (já resolvida no M156). Compara PG vs DataFusion nas dimensões
que decidem admitir/declinar:

| Dimensão | PostgreSQL | DataFusion | Casa byte-wise? | Decisão M157 |
|---|---|---|---|---|
| group-key = expr (`date_trunc`) | `FuncExpr` no target; `grpColIdx`→child TE (`createplan.c:2330`) | `Expr` de função como chave em `.aggregate(group_exprs,…)` (`df_executor.rs:476`) | mecânica: sim (re-derivamos do parse-tree) | **admite** (novo `GroupExprSlot`) |
| `date_trunc` sobre `timestamp` (sem tz) | `timestamp2tm(…, NULL)` tzp=NULL → naive (`timestamp.c:4645`) | `parsed_tz=None` → naive UTC (`date_trunc.rs:314-315`) | **SIM** (ambos naive) | **admite** (whitelist de granularidade) |
| `date_trunc` sobre `timestamptz` | trunca sob GUC `TimeZone` (`timestamp.c:4830-4850`; docs PG) | trunca na tz da coluna Arrow (UTC/config), ignora viés histórico (`date_trunc.rs:310-312`) | **NÃO** (diverge sob TZ≠UTC, DST, histórico) | **DECLINA** (honest-negative) |
| granularidade | +`decade/century/millennium` (`timestamp.c:4881-4913`) | só até `year` (`date_trunc.rs:74-85`); resto = `exec_err!` | parcial | admite {sec..year}∩; declina o resto |
| group-key = `EXTRACT` | `date_part` → `numeric` | idem | tipo numeric não suportado como group-key | **DECLINA** (`arrow_supported_group_type`) |
| group-key = `CASE` | `select_common_type` cross-branch + condição arbitrária | coerção própria + `Expr::Case` | risco de unificação de tipo + predicado WHEN arbitrário | **DECLINA** (escopo) |
| group-key = `Const` (`GROUP BY 1`) | `T_Const` no target | — | n/a | **DECLINA** (não é Var/Aggref/FuncExpr-whitelisted) |
| HAVING | `qual` do `Agg` (`createplan.c:2326`); pós-refs = `Aggref`+`OUTER_VAR` | `.filter` pós-`.aggregate` (parse-tree Aggref) | shape estreito: sim | **DECLINA/defer** (cobertura medida 0) |

**Síntese:** o único casamento byte-wise LIMPO desta classe é `date_trunc(timestamp-sem-tz, granularidade∈{sec..year})`.
Tudo o mais declina — por CORREÇÃO (timestamptz/CASE/EXTRACT) ou por COBERTURA (HAVING). O guard de timezone é a
tradução byte-wise do que o M151 review pegou no cross-type temporal: a divergência mora onde o A/B TZ=UTC não olha.

## ADRs

### D1 — Chave-expr `date_trunc` via 3º canal de nós em `custom_private` (reuso do padrão M156, não reinventar)

**Contexto:** `custom_private` é hoje `List[ IntList_numérica, List_text_preds ]` (`columnar_agg.rs:1145-1152`); uma
chave-expr precisa carregar `{base_attno, func, unit-string, out_typoid}`, e a unit é varlena (não cabe na IntList).
**Decisão:** acrescentar um 3º elemento `List_group_exprs` ao outer List, cada entry =
`[makeInteger(base_attno), makeInteger(func_code), makeString(unit), makeInteger(out_typoid)]` — o MESMO padrão de
`encode_text_preds` (`columnar_agg.rs:1007-1019`), leaf `Value` nodes copyObject/out/read-safe. Admissão: novo
`TargetSlot::GroupExpr` + `layout` kind=2. Exec: `date_trunc(lit(unit), col(base))` como group-expr (`df_executor.rs:460`).
**Alternativas rejeitadas:** (a) `copyObject` do `FuncExpr` inteiro — carrega mais do que precisamos e acopla ao
formato interno do FuncExpr (mais frágil que 4 leaf nodes); (b) generalizar já para EXTRACT/CASE/aritmética — YAGNI,
M152 mede date_trunc como a única fatia limpa. **Consequência:** KISS, reusa o mecanismo M156 verbatim (Regra 9 /
`.claude/rules/parsimony-ladder.md`); a IntList numérica e o canal de texto não mudam. **Ref rule:**
`.claude/rules/architecture.md` (fronteira do CustomScan — o descritor viaja pelo private, o exec reconstrói).

### D2 — Guard de timezone do `date_trunc` (admite `timestamp`; declina `timestamptz` incondicionalmente)

**Contexto:** `date_trunc(timestamp)` é timezone-independente em ambos (PG `timestamp2tm(…,NULL)`, `timestamp.c:4645`;
DataFusion `parsed_tz=None`, `date_trunc.rs:314-315`); `date_trunc(timestamptz)` depende do GUC `TimeZone` no PG
(`timestamp.c:4830-4850` + docs oficiais PG) mas o DataFusion trunca na tz da coluna Arrow (UTC/config, ignora viés
histórico `date_trunc.rs:310-312`). **Decisão:** admitir `date_trunc(unit_const, Var)` sse `vartype==1114`
(timestamp sem tz) E `unit` ∈ {`second,minute,hour,day,month,quarter,year`} (o ∩ das granularidades, byte-provado por
A/B); declinar `1184` (timestamptz) SEMPRE, declinar granularidade fora do ∩, declinar `week` até o A/B provar ISO-week.
**Alternativas rejeitadas:** (a) anexar o GUC `TimeZone` como tz do Arrow p/ casar timestamptz — rejeitada: o DataFusion
ignora o histórico/DST não-minuto (`date_trunc.rs:310-312`) ≠ `pg_localtime`; divergiria em transições; (b) confiar no
A/B TZ=UTC — rejeitada: mascara a divergência (o A/B nunca roda TZ≠UTC nem usa timestamptz). **Consequência:** q42
(EventTime `timestamp`, `create.sql:8`) roteia byte-idêntico; timestamptz declina fail-closed. Espelha o M151
(classe provadamente-segura). **Ref rule:** `.claude/rules/discover-phd-rigor.md` (R3: perf/correção é claim, não
opinião — a divergência é provada por fonte primária, não assumida); `.claude/rules/error-handling.md` (fail-closed).

### D3 — HAVING, CASE e EXTRACT declinam no M157 (honest-negative measured; date_trunc é o único lever)

**Contexto:** HAVING é o `qual` do `Agg` (`createplan.c:2326`); empurrável ao DataFusion via `.filter` pós-`.aggregate`
para o shape estreito `Aggref OP Const` (parse-tree Aggref, byte-safe). MAS as 2 queries HAVING do ClickBench (q27,q28)
morrem em blockers INDEPENDENTES (`AVG(length(URL))` agg-sobre-expr; `REGEXP_REPLACE`) → cobertura medida = 0
(`queries.sql:28,29`; `docs/benchmarks/m152-routing-map.md:66`). `EXTRACT` (q18) sai `numeric` (group-key não
suportado); `CASE` (q39) tem condição `WHEN` arbitrária + risco de unificação de tipo cross-branch; `GROUP BY 1`
(q34) é `Const`; aritmética (q35,q36) é `OpExpr`. **Decisão:** M157 admite SÓ `date_trunc(timestamp)`-group; HAVING,
CASE, EXTRACT, const, aritmética declinam. **Alternativas rejeitadas:** (a) implementar HAVING "porque é elegante" —
rejeitada: roteia zero queries (lição M155 `m155-topn-honest-negative`: cobertura 0 → não rotear; esforço≠complexidade);
(b) implementar CASE-text — rejeitada: reabre a tradução de predicado (WHEN) + unificação de tipo (superfície grande,
YAGNI). **Consequência:** M157 é uma fatia mínima e honesta (1 query, byte-idêntica); HAVING/CASE ficam documentados
como possíveis milestones futuros SE bundleados com agg-over-expression/regex e SE o benchmark justificar (Regra 5).
**Ref rule:** `.claude/rules/discover-phd-rigor.md` (R3 UNBENCHMARKED); CLAUDE.md "Esforço≠Complexidade".

## Recommendations

Desenho concreto do M157 para o `/to-plan` (uma fatia, TDD, byte-idêntico):

1. **Tipo `GroupExprSpec`** (irmão de `group_cols`) em `columnar_agg.rs`: `{ base_attno: i32, func: GroupFunc, unit: String, out_typoid: u32 }`, `enum GroupFunc { DateTrunc }`. `Admitted` (`columnar_agg.rs:415-423`) ganha `group_exprs: Vec<GroupExprSpec>`; `layout` ganha kind=2.
2. **Admissão** — em `classify_target_node` (`columnar_agg.rs:485`), ramo `T_FuncExpr`: nome `date_trunc` (sem OID hardcoded), arg0 `Const` text ∈ whitelist {sec,min,hour,day,month,quarter,year}, arg1 `Var` base-rel tipo **1114**. `timestamptz` (1184) / granularidade fora / week → `admit_trace("group_expr_date_trunc_declined"); return None`. `admit` (`columnar_agg.rs:734-736`) passa a aceitar group_exprs não-vazio como chave válida.
3. **Serialização** — 3º canal em `encode`/`begin` (padrão `encode_text_preds`, `columnar_agg.rs:1007-1019,1322`): entry `[Integer(base_attno), Integer(func), String(unit), Integer(out_typoid)]`.
4. **Group-expr no DataFusion** — em `run_columnar_grouped_aggs` (`df_executor.rs:460`), estender `group_exprs` com `date_trunc(lit(Utf8(unit)), col(base_name))`; materializar de volta com `out_typoid=1114` via `arrow_value_to_datum`, atento à UNIT Arrow ns→µs (MUST-verify).
5. **Guards (D2)**: timestamptz declina; granularidade fora do ∩ declina; a coluna base entra na projeção (`decode_to_batch`) como qualquer group-col.
6. **Wiring triad**: (a) caller = o swap M115 já roteia — o delta é o group-expr no exec; (b) integration test = A/B in-PG sobre `theodb_columnar` com q42-like (date_trunc minute/day/month sobre `timestamp`) exigindo `diverged=0`, MAIS o caso negativo `timestamptz` sob `SET TimeZone='America/Sao_Paulo'` que DEVE declinar (Corner 1); (c) runtime metric = estender `admit_trace` (M152) com `group_expr_date_trunc_pushed` / `group_expr_date_trunc_timestamptz` / `group_expr_granularity_unsupported`.
7. **Honest-negatives documentados (D3)**: HAVING (cobertura 0 — q27/q28 têm blockers independentes), CASE/EXTRACT/const/aritmética declinam. M157 roteia **q42** (a única query limpa desta classe). Sem claim de performance sem `docs/benchmarks/` (Regra 5).
8. **Alvo medível**: `diverged=0` no A/B (`run_m128`) para q42 e para os casos negativos de granularidade/timestamptz; cobertura ClickBench +1 (a única disponível — honesto).

## References

**Q1 — representação da chave-expr no `Agg` (≥2 primárias):**
- `.claude/knowledge-base/references/postgres/src/backend/optimizer/plan/createplan.c:2308-2344` (`create_agg_plan` → `make_agg(tlist, quals, …)`, `grpColIdx` via `extract_grouping_cols`).
- `theodb_rs/src/am/columnar_agg.rs:485-518,712,734-736` (`classify_target_node` só `T_Var`; `admit` re-deriva do parse-tree target; decline de group-expr).

**Q2 — o delta no nosso caminho (≥2 primárias):**
- `.claude/knowledge-base/discoveries/blueprints/columnar-text-where-pushdown-blueprint.md` (o 2º canal M156 — padrão a reusar).
- `theodb_rs/src/am/columnar_agg.rs:415-423,441-444,1007-1019,1145-1152,1322-1350` (Admitted/TargetSlot/encode_text_preds/2º canal/decode); `theodb_rs/src/am/df_executor.rs:460,476,485-491` (group_exprs/aggregate/materialização).

**Q3 — HAVING (≥2 primárias):**
- `.claude/knowledge-base/references/postgres/src/backend/optimizer/plan/createplan.c:2324-2326` (HAVING = `qual` do `Agg`).
- `theodb_rs/src/am/columnar_agg.rs:687-695,1125-1131` (decline de `havingQual`; swap dropa lefttree/qual); `theodb_rs/src/am/df_executor.rs:206-227,473,476` (`push_agg_exprs`/`.filter`/`.aggregate`).

**Q4 — API DataFusion date_trunc/CASE/filter + semântica (≥2 primárias + web):**
- `.claude/knowledge-base/references/datafusion/datafusion/functions/src/datetime/date_trunc.rs:57-85` (granularidades), `:254-266` (return_type Timestamp Nanosecond None), `:301-329` (`process_array` `parse_tz`/`parsed_tz.is_none()`), `:310-315` (fast-path UTC + viés histórico ignorado), `:101-106` (`exec_err!` granularidade não suportada).
- [DataFusion — SQL Scalar Functions (date_trunc, session time zone via `datafusion.execution.time_zone`)](https://datafusion.apache.org/user-guide/sql/scalar_functions.html) (WEB/R0, fetch ao vivo 2026-07-25).

**Q5 — regra timezone do date_trunc do PG (≥2 primárias + web):**
- `.claude/knowledge-base/references/postgres/src/backend/utils/adt/timestamp.c:4621-4645` (`timestamp_trunc`, tzp=NULL → timezone-independente), `:4824-4850` (`timestamptz_trunc_internal`, tzp=session_timezone), `:4652-4939` (switch `DTK_*` de granularidade, `date2isoweek`, `decade/century/millennium`).
- [PostgreSQL — Date/Time Functions (`date_trunc(field, timestamptz)` "with respect to the current TimeZone setting")](https://www.postgresql.org/docs/current/functions-datetime.html) (WEB/R0, fetch ao vivo 2026-07-25).

**Q6 — queries-alvo + oráculo A/B (≥2 primárias):**
- `docs/benchmarks/m152-routing-map.md:22,25,66` (`target_grouping_expression_or_other` q18,34,35,39,42; HAVING q27,q28).
- `benchmarks/clickbench/theodb/queries.sql:19,28,29,35,36,40,43` (SQLs reais); `benchmarks/clickbench/theodb/create.sql:8-9` (EventTime `TIMESTAMP`, EventDate `Date`); `benchmarks/run_m128_clickbench.py:141-144,167,178-181,244,251-252` (oráculo A/B `_canonical`/EXPLAIN/`diverged`).

**Rules citadas:** `.claude/rules/discover-phd-rigor.md` (R0 web + acervo, R3 claim), `.claude/rules/architecture.md`
(fronteira do CustomScan), `.claude/rules/testing.md` §4.1 (edge vs. negative), `.claude/rules/error-handling.md`
(fail-closed), `.claude/rules/parsimony-ladder.md` (Regra 9 rung 4). CLAUDE.md ("Esforço≠Complexidade"). Memórias:
`m151-datafusion-coverage-released` (classe provadamente-segura), `m155-topn-honest-negative` (cobertura 0 → não rotear).
