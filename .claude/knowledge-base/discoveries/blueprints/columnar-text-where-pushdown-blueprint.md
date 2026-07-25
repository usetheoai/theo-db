# Blueprint: Rotear predicados de texto no WHERE ao CustomScan colunar (M156)

**Slug:** columnar-text-where-pushdown
**Cycle:** discover (execute) — 2026-07-25
**Plan:** `.claude/knowledge-base/discoveries/plans/columnar-text-where-pushdown-plan.md`
**Owner:** paulohenriquevn

## Objective

Dar o desenho concreto do M156: **como serializar um predicado de texto (`=`/`<>`/LIKE) no `custom_private` e construir
o filtro DataFusion equivalente, byte-idêntico ao PG, com os guards de correção (collation determinística; operadores
seguros; regex/ILIKE declinam)** — suficiente para rotear as ~4-8 queries text-WHERE do ClickBench que o M152 mediu.

## Context

O M152 (routing-map, `docs/benchmarks/m152-routing-map.md`) mediu `unpushable_where_qual` como o MAIOR
first-blocker das queries ClickBench não-roteadas ao CustomScan colunar (`SearchPhrase <> ''`, `URL LIKE '%…%'`,
regex). Hoje o `extract_zone_predicate` (`theodb_rs/src/am/columnar_agg.rs:160`) só admite predicados cuja coluna
tem um `MinMaxKind` numérico e serializa o const como `u64` zone-bits (`ZonePredicate.const_bits`,
`theodb_rs/src/am/zonemap.rs:36`); texto (varlena, comprimento variável) não cabe no `u64` nem na `IntList` do
`custom_private` (`encode_private`, `columnar_agg.rs:868`). O `build_filter_expr`
(`theodb_rs/src/am/df_executor.rs:269`) só constrói literais numéricos/temporais.

Este blueprint responde **como serializar um predicado de texto (`=`/`<>`/LIKE) no `custom_private` e construir o
filtro DataFusion equivalente, byte-idêntico ao PG, com os guards de correção (collation determinística; operadores
seguros; regex declina)** — o desenho concreto do M156. A técnica-âncora de shippability + collation-safety vem do
`pg_clickhouse` (Apache-2.0) e do `postgres_fdw` (a fonte canônica local do state-machine de collation).

**Escopo honesto (o que empurra vs. declina):**

| Operador PG | Node | M156 | Razão |
|---|---|---|---|
| `text = const` | `OpExpr` (btree `=`, strat. 3) | **empurra** | byte-igualdade == PG sob collation determinística |
| `text <> const` | `OpExpr` (negador de `=`) | **empurra** | mesmo caminho `Ne` já existente (filter-only) |
| `text LIKE '%p%'` (`~~`) | `OpExpr` | **empurra** | arrow `like` kernel = semântica SQL LIKE |
| `text NOT LIKE` (`!~~`) | `OpExpr` | **empurra** | arrow `nlike` |
| `text ILIKE`/`NOT ILIKE` (`~~*`/`!~~*`) | `OpExpr` | **DECLINA** | ILIKE do PG é locale-aware; arrow ILIKE ≠ garantidamente igual |
| regex `~`,`!~`,`~*`,`!~*` | `OpExpr` | **DECLINA (honest-negative)** | DataFusion usa Rust `regex` (PCRE-like) ≠ POSIX ERE do PG — confirma M152 |

---

## Coverage Corner 1 — Integration Tests

**Q5 — Como o `pg_clickhouse/test/` verifica a correção do pushdown de texto/LIKE/collation (o oráculo)?**

O oráculo do pg_clickhouse é o **par EXPLAIN + execução dupla**: para cada predicado de texto, o teste emite
`EXPLAIN (VERBOSE, COSTS OFF)` (prova que o predicado foi empurrado — aparece no "Remote SQL") seguido da MESMA
query executada, e o `.out` esperado fixa o resultado. Ver `.claude/knowledge-base/references/pg_clickhouse/test/sql/ilike_regex.sql:47-49`:

```sql
-- ILIKE should match regardless of case
EXPLAIN (VERBOSE, COSTS OFF)
SELECT id, name FROM ilike_regex_bin.events WHERE name ILIKE '%view%' ORDER BY id;
SELECT id, name FROM ilike_regex_bin.events WHERE name ILIKE '%view%' ORDER BY id;
```

O ficheiro cobre `LIKE`/`NOT LIKE`, `ILIKE`/`NOT ILIKE`, LIKE em contexto de agregado
(`WHERE name ILIKE 'page%'` sob `count(*)`), e dados propositalmente de casing misto
(`ilike_regex.sql:24-34`: `'page_view'`, `'Page_View'`, `'PAGE_VIEW'`) — o dataset que expõe uma divergência de
case-folding. O oráculo de correção é: **o resultado com pushdown == o resultado sem pushdown == o resultado do
PostgreSQL nativo**. O `<>` de texto é coberto no mesmo estilo em `in_null_semantics.sql:107-158` (execução dupla
de `<> ANY/ALL`).

**Mapeamento ao NOSSO oráculo (A/B byte-idêntico):** o pg_clickhouse compara *pushed vs. remote*; nós comparamos
*CustomScan colunar vs. plano nativo do PostgreSQL sobre a MESMA tabela `theodb_columnar`* — o padrão `run_m128`
(limit-strip + canonicalize) já usado em M114/M149/M151. Para o M156:

1. Uma tabela `theodb_columnar` com uma coluna `text` clusterizada, dados de casing/acento misto e um valor `''`.
2. Para cada shape (`= 'x'`, `<> ''`, `LIKE '%abc%'`, `NOT LIKE 'a%'`), rodar com `enable_columnar_agg=on` e `off`
   e exigir `diverged=0` (mesmo multiset de linhas/agregados).
3. **Casos negativos obrigatórios** (`.claude/rules/testing.md` §4.1): coluna com collation NÃO-determinística
   (ICU `und-x-icu` case-insensitive) → o CustomScan DEVE declinar (cai no nativo) e o resultado casa; `ILIKE` e
   regex `~` → declina; LIKE com escape (`LIKE 'a\%b'`) → resultado byte-idêntico ao PG (verifica o `escape_char`).
4. Edge: pattern com `%`/`_` literais escapados; string vazia; `NULL` const (não empurra).

---

## Coverage Corner 2 — Dependencies

**Q4 — Qual módulo/função do DataFusion 54 dá o filtro `LIKE`/`=`/`<>`/regex sobre Utf8, e a semântica casa SQL
LIKE + operadores PG (RE2 vs POSIX)?**

- **`=` / `<>` sobre Utf8:** os métodos já usados no `build_filter_expr` (`df_executor.rs:305,308`) — `col(name).eq(lit(...))`
  / `.not_eq(lit(...))` — funcionam sobre `Utf8` desde que o literal seja `ScalarValue::Utf8(Some(String))`. O
  `LikeExpr`/comparadores físicos vivem em
  `.claude/knowledge-base/references/datafusion/datafusion/physical-expr/src/expressions/like.rs:27-60`
  (`pub struct LikeExpr { negated, case_insensitive, expr, pattern }`). No nível lógico, o node é
  `Expr::Like(Like { negated, case_insensitive, expr, pattern, escape_char })` (variante confirmada em
  `.claude/knowledge-base/references/datafusion/datafusion/expr/src/expr_schema.rs:208,376` — `Expr::Like { .. } => DataType::Boolean`).
  A construção idiomática é `col(name).like(lit(pattern))` / `.not_like(...)`.
- **Semântica LIKE:** o `LikeExpr` delega ao kernel arrow `like`/`nlike`, que implementa a semântica SQL LIKE
  (`%` = qualquer sequência, `_` = 1 char) sobre bytes UTF-8 — **casa a semântica do PG LIKE sob collation
  determinística**. Ponto de atenção de correção: o **escape char**. O PG LIKE usa `\` como escape default; o
  `Like.escape_char` do DataFusion é `Option<char>` — o M156 DEVE setar o escape para casar o default do PG
  (validado pelo A/B; é um MUST-verify do Corner 1).
- **Regex → DECLINA (confirma M152):** o módulo de regex do DataFusion usa o crate Rust `regex`
  (`.claude/knowledge-base/references/datafusion/datafusion/functions/src/regex/mod.rs:21` `use regex::Regex;` e
  `.../functions/src/regex/regexplike.rs:37` `use regex::Regex;`). A doc oficial confirma: *"Apache DataFusion uses
  a PCRE-like regular expression syntax (minus … look-around and backreferences)"*
  ([DataFusion scalar functions](https://datafusion.apache.org/user-guide/sql/scalar_functions.html)). O crate
  `regex` é da família RE2, **não POSIX ERE** — que é o motor do PG `~`/`~*` (`src/backend/regex`). Casos de borda
  divergem (classes POSIX, back-refs, greediness), então **regex de texto NÃO é empurrável byte-idêntico → declina
  para o plano nativo** (fail-safe, nunca fail-wrong).

**Nenhuma dependência nova** — DataFusion/Arrow já estão no binário (M98/M100/M143); é a Regra 9 / parsimony-ladder
rung 4 (reusar o que já está instalado). O único delta é usar `ScalarValue::Utf8` + `Expr::Like`, APIs já
disponíveis.

---

## Coverage Corner 3 — Tools

**Q6 — Como o `shipable.c` MANTÉM a whitelist de operadores + o override dos regex (`~`,`!~`,`~*`,`!~*`)?**

O `chfdw_is_shippable` (`.claude/knowledge-base/references/pg_clickhouse/src/shipable.c:181`) decide a
empurrabilidade de um operador/função em camadas, na ORDEM (a ordem importa):

1. **Override de operadores custom ANTES do atalho builtin** (`shipable.c:197-212`): para `OperatorRelationId`,
   consulta `chfdw_check_for_custom_operator`. Se o operador é um regex-match (`CF_REGEX_MATCH`,
   `CF_REGEX_NO_MATCH`, `CF_REGEX_ICASE_MATCH`, `CF_REGEX_ICASE_NO_MATCH`), o pushdown é **condicionado a um GUC**
   (`return chfdw_pushdown_regex_ok();`, `shipable.c:207`) — regex é opt-in porque a semântica do motor remoto
   diverge. Para os demais custom-ops, `return true`.
2. **Builtin ships by default** (`shipable.c:364-377`): operadores e tipos builtin (`objectId < FirstUnpinnedObjectId`,
   via `chfdw_is_builtin`, `shipable.c:142-145`) empurram; funções builtin só empurram se registradas
   explicitamente (senão `return false` — *unrecognised builtins fail shippability rather than deparse to a name
   that may behave differently on the remote*, `shipable.c:356-363`). É uma **whitelist fail-closed**.
3. **Flags de regex** (`regex_flags_ok`, `shipable.c:147-175`): mesmo quando o regex é permitido, flags não
   suportadas (qualquer coisa fora de `i m n p s t w`, e `g` só em contexto global) → `return false`.

**Padrão extraído → nosso gate (M156):** replicar a whitelist fail-closed no `extract_zone_predicate`, mas mapeada
ao nosso conjunto seguro. Aceitar por **classe de operador detectada por opfamily/negador** (o precedente D5
"no hardcoded OIDs" que já usamos, `columnar_agg.rs:204-234`):

- btree `=` (strategy 3) e seu negador `<>` → aceita (já implementado para numérico; estender a texto).
- LIKE `~~` / NOT LIKE `!~~` → aceita (detectar pelo nome do operador `~~`/`!~~` como o pg_clickhouse faz em
  `deparse.c:5127-5133`, ou pelo OID do operador texticlike builtin).
- ILIKE `~~*`/`!~~*` e regex `~`,`!~`,`~*`,`!~*` → **declina** (fora da whitelist; regex é o análogo do gate-por-GUC
  do pg_clickhouse, só que nós declinamos incondicionalmente porque não temos motor POSIX no DataFusion).

Diferente do pg_clickhouse (que tem GUC para ligar regex ao motor remoto), nós **não** oferecemos GUC de regex: sem
motor POSIX equivalente, ligar seria fail-wrong. Fail-closed por design (`.claude/rules/error-handling.md`).

---

## Coverage Corner 4 — Techniques

### Q1 — Regra de collation-safety (`foreign_expr_walker`/shippability) e como casa nosso guard M153/M154

**Fonte canônica local: `postgres_fdw`** (o `pg_clickhouse` herda e simplifica dela). O
`.claude/knowledge-base/references/postgres/contrib/postgres_fdw/deparse.c:82-86` define o state-machine de três
estados:

```
FDW_COLLATE_NONE    /* tipo não-collatable, ou collation default (irrelevante) */
FDW_COLLATE_SAFE    /* collation deriva de um Var da tabela foreign */
FDW_COLLATE_UNSAFE  /* collation não-default derivada de outro lugar (Const/Param/join) */
```

- **Var** (`deparse.c:363-385`): se vem da tabela foreign, `state = OidIsValid(collation) ? SAFE : NONE`. Var de
  outra tabela com collation não-default → `UNSAFE`.
- **OpExpr** (`deparse.c:619-639`) — a regra que fecha a shippability:
  ```c
  if (oe->inputcollid == InvalidOid)         /* inputs noncollatable → OK */ ;
  else if (inner_cxt.state != FDW_COLLATE_SAFE ||
           oe->inputcollid != inner_cxt.collation)
      return false;                          /* collation não deriva de um Var foreign → NÃO empurra */
  ```

Ou seja: **um operador de texto só é empurrável se a collation de entrada é a collation default OU deriva de um Var
da própria tabela** (não uma collation não-default "solta"). O `pg_clickhouse` mantém a mesma intenção — o comentário
do `foreign_expr_walker` (`.claude/knowledge-base/references/pg_clickhouse/src/deparse.c:900-906`) diz *"all
collations used in the expression derive from Vars of the foreign table… the logic is pretty close to
assign_collations_walker()"* — e rejeita colunas de sistema no ramo `T_Var` (`deparse.c:946-948`).

**Como casa o NOSSO guard (M153/M154):** o postgres_fdw guarda *"a collation deriva de um Var foreign"* porque o
servidor remoto reavalia a comparação sob a collation nomeada. O nosso caso é DIFERENTE mas leva ao MESMO teste
prático: nós reavaliamos o filtro no **DataFusion**, que compara **byte-a-byte**. Byte-igualdade == igualdade-PG
**se e somente se a collation for determinística** — a doc do PG confirma: *"A deterministic collation … considers
strings to be equal only if they consist of the same byte sequence"*, e *"certain operations are not possible with
nondeterministic collations, such as some pattern matching operations"*
([PG collation docs](https://www.postgresql.org/docs/current/collation.html)). É exatamente o guard que já
validamos: `get_collation_isdeterministic((*var).varcollid)` no group-by M153 (`columnar_agg.rs:407-414`) e no
COUNT DISTINCT M154 (`columnar_agg.rs:479`). **Portanto o M156 reusa o guard M153/M154 verbatim** para o predicado
de texto: empurra sse `varcollid == InvalidOid` (não-collatable, impossível para texto) OU
`get_collation_isdeterministic(inputcollid) == true`; senão declina. Usar `inputcollid` do `OpExpr` (a collation
exata com que o PG dirige a comparação), como já fazemos no COUNT DISTINCT (`columnar_agg.rs:476-479`) — precisão +
defense-in-depth.

### Q2 — Serialização de `Const` de texto + LIKE no `deparse.c` (quoting/escaping/mapeamento de operador)

- **`deparseConst`** (`.claude/knowledge-base/references/pg_clickhouse/src/deparse.c:3370-3484`): usa
  `getTypeOutputInfo` + `OidOutputFunctionCall` para obter a representação textual, e para tipos string cai em
  `deparseStringLiteral` (o `default:` em `deparse.c:3481-3483`). O `deparseStringLiteral`
  (`deparse.c:2882-2887`) delega a `ch_quote_literal` (quoting + escaping de aspas/backslash), evitando injeção.
  Numéricos são emitidos sem aspas (`deparse.c:3448-3469`); bytea tem escaping byte-a-byte próprio
  (`deparse.c:3417-3443`).
- **Mapeamento LIKE** (`deparseOperatorName`, `deparse.c:5121-5137`): o operador é traduzido pelo **nome**:
  `~~ → LIKE`, `~~* → ILIKE`, `!~~ → NOT LIKE`, `!~~* → NOT ILIKE`; qualquer outro passa verbatim.

**Delta para nós (D2 do plano — extrair o PADRÃO, não o código):** o pg_clickhouse **emite uma string SQL remota**;
nós **NÃO emitimos SQL** — construímos um `Expr` do DataFusion e serializamos o const como bytes no `custom_private`.
Logo o `ch_quote_literal`/`deparseStringLiteral` são IRRELEVANTES para nós (não há string SQL para escapar; o valor
vai como `ScalarValue::Utf8(Some(bytes))`, sem risco de injeção — é dado, não código). O que reusamos é: (a) a
**decisão** "detectar LIKE pelo operador `~~`/`!~~`" (Q6), e (b) a lição de que o const de texto deve trafegar como
**bytes crus**, não como número. Isso informa o formato de `custom_private` do Q3.

### Q3 — O DELTA no NOSSO caminho: carregar const de texto no `custom_private` + `Expr` de filtro sobre Utf8

Este é o coração do M156. Hoje (`encode_private`, `columnar_agg.rs:868-894`), `custom_private` é uma **`IntList`**
(`lappend_int`), e cada predicado ocupa **4 ints**: `col, op, const_bits_hi, const_bits_lo`
(`columnar_agg.rs:881-884`), lido de volta em `decode`/begin (`columnar_agg.rs:1161-1179`). Uma `IntList` NÃO pode
carregar bytes variáveis → o texto não cabe. O paper MonetDB/X100 (`monetdb-x100-boncz-2005.pdf`, Abstract + §1)
fundamenta a estratégia: processar colunas de forma vetorizada e empurrar a **selection** para dentro do scan
vetorizado, materializando tarde (*"combining the column-wise execution of MonetDB with the incremental
materialization offered by Volcano-style pipelining"*) — que é exatamente o que fazemos ao empurrar o `Filter` de
texto ao DataFusion em vez de materializar linha-a-linha no PG. Um predicado de texto não poda chunk-group (não há
zone-map de texto — `chunk_can_match` só entende `MinMaxKind` numérico, `zonemap.rs:46`), então ele **só reduz linhas
no `Filter`** (exatamente como o `Ne` numérico hoje, `zonemap.rs:88`).

**(a) Formato de serialização — segundo canal de nodes (KISS, copyObject-safe).** A `IntList` atual é ótima para
numérico; NÃO a quebrar. Adotar o padrão canônico de `fdw_private` do PG (uma `List` de `Node`s heterogênea que
`copyObject`/`_readCustomScan` sabem copiar/ler). Transformar `custom_private` numa `List` de 2 elementos:

```
custom_private = List[
   elem[0] = <a IntList numérica de hoje>,          // metadata + preds numéricos, INALTERADA
   elem[1] = List[ text_pred_0, text_pred_1, … ]    // segundo canal, novo
]
```
onde cada `text_pred_i` é um sub-`List` de 3 nodes: `[ makeInteger(col), makeInteger(op_code), Const* ]` — e o
`Const*` é o **próprio `Const` de texto copiado** (`copyObject`). Justificativa: `Const` é um Node totalmente
copiável/serializável pelos read/out funcs do PG (é assim que `postgres_fdw` trafega literais em `fdw_private`),
carrega os bytes varlena, o `consttype`, o `constcollid` e o `constlen` de graça — zero parsing manual de bytes/
offset. `op_code` novo: reusar `ZoneOp` estendido OU um enum de texto (`TextEq=0, TextNe=1, TextLike=2, TextNotLike=3`).
Alternativa rejeitada (mais complexa, viola KISS): serializar os bytes do texto como uma sequência de ints
comprimento+conteúdo na própria `IntList` — reinventa `nodeToString` para `Const` (Regra 9). **Decisão: segundo
canal de `Const` nodes.**

**(b) O braço no `build_filter_expr`** (`df_executor.rs:269-316`). Hoje o loop só trata `ZonePredicate` numérico. O
M156 adiciona um segundo loop sobre os text-preds decodificados. Para cada `(col, op_code, text: String, collid)`:

```rust
let c = col(name.as_str());                     // coluna Utf8 já decodificada (df_executor build_arrow)
let e = match op_code {
    TextEq      => c.clone().eq(lit(ScalarValue::Utf8(Some(text)))),
    TextNe      => c.clone().not_eq(lit(ScalarValue::Utf8(Some(text)))),
    TextLike    => c.clone().like(lit(ScalarValue::Utf8(Some(text)))),      // escape_char = PG default '\\'
    TextNotLike => c.clone().not_like(lit(ScalarValue::Utf8(Some(text)))),
};
acc = Some(match acc { Some(prev) => prev.and(e), None => e });
```

O `col` já é `Utf8` porque `build_arrow` decodifica a coluna de texto (o `custom_scan_tlist` já expõe a coluna).
`TextLike`/`TextNotLike` usam `Expr::like`/`Expr::not_like` (Corner 2) com `escape_char` = `\` para casar o LIKE
default do PG (MUST-verify pelo A/B). Como o predicado de texto nunca poda chunk-group, o `skip`-mask do zone-map é
inalterado; o texto entra só na composição `acc.and(...)` — o `Filter` é a autoridade final (ADR D3, o mesmo
princípio de `Ne`).

**(c) O gate de admissão** (`extract_zone_predicate`, `columnar_agg.rs:160`): hoje retorna `None` quando
`minmax_kind_of(vartype) == None` (`columnar_agg.rs:182-185`) — é ONDE o texto morre. O M156 adiciona, ANTES desse
`None`, um ramo de texto: se `vartype ∈ {text, varchar, bpchar}` (25/1043/1042) E o operador ∈ whitelist de texto
(Q6) E o guard de collation determinística passa (Q1) E o const não é NULL → produzir um `TextPredicate` (novo tipo
irmão de `ZonePredicate`). Qualquer falha → `None` → declina (o `extract_all_predicates`, `columnar_agg.rs:298-309`,
já força decline se QUALQUER qual não for empurrável — invariante preservado).

---

## Patterns

1. **Whitelist fail-closed por classe de operador, override-antes-de-builtin** (pg_clickhouse `shipable.c:197-212`,
   `364-377`): decidir empurrabilidade por classe detectada (opfamily/negador/nome), com o caso perigoso (regex)
   barrado por default. Espelha o nosso D5 "no hardcoded OIDs" (`columnar_agg.rs:204-234`).
2. **Collation-safety como pré-condição de pushdown de texto** (postgres_fdw `deparse.c:82-86,619-639`): um operador
   collatable só empurra se a collation é default/deriva de Var foreign. Nós traduzimos para "collation
   determinística" porque o motor de reavaliação (DataFusion) é byte-wise — guard já validado M153/M154.
3. **Const de texto trafega como Node (`Const`), não como número** (postgres_fdw `fdw_private` idiom): usar o node
   copiável do PG em vez de reinventar serialização de bytes (Regra 9).
4. **Predicado-só-filtro vs. predicado-que-poda**: texto (sem zone-map) rida no `Filter` do DataFusion sem podar
   chunk-group — o mesmo papel do `Ne` numérico hoje (`zonemap.rs:88`, `df_executor.rs:308`). Poda é uma
   otimização ortogonal (zone-map de texto = fora de escopo).
5. **Push-selection-into-vectorized-scan + late materialization** (MonetDB/X100, Abstract+§1): reduzir linhas no
   engine colunar vetorizado em vez de materializar linha-a-linha no PG é a origem do ganho (o mesmo eixo do M148/
   M149).
6. **Oráculo A/B com execução dupla + EXPLAIN** (pg_clickhouse `ilike_regex.sql:47-49`): provar pushdown (EXPLAIN) +
   igualdade de resultado (pushed vs. nativo). Casamos com `run_m128` (limit-strip+canonicalize).

## Recommendations

Desenho concreto do M156 (uma fatia, TDD, byte-idêntico):

1. **Tipo `TextPredicate`** em `zonemap.rs` (irmão de `ZonePredicate`): `{ col: usize, op: TextOp, needle: String, collid: Oid }`,
   `enum TextOp { Eq, Ne, Like, NotLike }`. Não carrega zone-bits (texto não poda).
2. **Admissão** — estender `extract_zone_predicate` (`columnar_agg.rs:160`) com um ramo de texto ANTES do `None`
   por `MinMaxKind`: whitelist de operador (`=`, `<>` via negador, `~~`, `!~~`), guard
   `get_collation_isdeterministic(inputcollid)` (reuso M153/M154), const não-NULL, tipo ∈ {text,varchar,bpchar}.
   Declinar ILIKE (`~~*`/`!~~*`) e regex (`~`/`!~`/`~*`/`!~*`). `extract_all_predicates` retorna ambos os vetores;
   decline se QUALQUER qual não empurrável (invariante preservado).
3. **Serialização** — em `encode_private` (`columnar_agg.rs:868`), migrar `custom_private` para
   `List[ IntList_numérica, List_de_text_preds ]`; cada text-pred = `List[ makeInteger(col), makeInteger(op), copyObject(Const*) ]`.
   Em `decode`/begin (`columnar_agg.rs:1120-1186`), ler o 2º canal, extrair a `String` do `Const` (via
   `TextDatumGetCString`/`text_to_cstring`), reconstruir `TextPredicate`.
4. **Filtro** — em `build_filter_expr` (`df_executor.rs:269`), 2º loop sobre text-preds → `eq`/`not_eq`/`like`/`not_like`
   com `lit(ScalarValue::Utf8(...))`; compor com `acc.and(...)`. `escape_char = '\\'` para LIKE (MUST-verify A/B).
5. **Wiring triad**: (a) caller = o CustomScan já roteia (`begin`/`exec`), o delta é os preds de texto no filtro;
   (b) integration test = A/B in-PG sobre `theodb_columnar` com coluna text (Corner 1); (c) runtime metric = estender
   `admit_trace` (M152) com razões `text_pushed`/`text_declined_collation`/`text_declined_regex`/`text_declined_ilike`.
6. **Honest-negative documentado**: regex (`~`/`!~`/`~*`/`!~*`) e ILIKE ficam de FORA — declinam para o nativo. É
   correção, não incompletude: DataFusion (Rust `regex`, PCRE-like) ≠ POSIX ERE do PG; ILIKE do PG é locale-aware.
7. **Alvo medível**: rotear as queries text-WHERE do ClickBench que o M152 marcou (`SearchPhrase <> ''`,
   `URL LIKE '%…%'`), com `diverged=0` no A/B. Sem claim de performance sem `docs/benchmarks/` (Regra 5).

## Cross-cutting Comparison

O padrão vem de um **FDW** (pg_clickhouse/postgres_fdw), mas o nosso alvo é um **CustomScan local sobre DataFusion**.
A comparação isola o que se reusa (a LÓGICA de segurança) do que se reimplementa (o mecanismo):

| Dimensão | pg_clickhouse / postgres_fdw (FDW) | TheoDB M156 (CustomScan) | O que reusamos |
|---|---|---|---|
| Alvo do predicado | deparse → **string SQL** enviada ao servidor remoto | **`Expr` do DataFusion** avaliado localmente sobre batch Arrow | — (mecanismo diferente; ADR D1) |
| Serialização do const | `deparseStringLiteral`/`ch_quote_literal` (quoting/escaping para SQL) | `copyObject(Const*)` num 2º canal de `custom_private` → `ScalarValue::Utf8` (dado, não código) | o **conceito** de carregar o Const, não o escaping (irrelevante sem SQL) |
| Decisão de shippability | `foreign_expr_walker` + whitelist `chfdw_is_shippable` (`shipable.c:181`) | gate por OID de operador (`=`/`<>`/`~~`/`!~~`) + tipo texto (25/1043) | a **checklist de operadores seguros** + a estrutura fail-closed |
| Collation-safety | state-machine `FDW_COLLATE_NONE/SAFE/UNSAFE` (`postgres_fdw/deparse.c:82-86,619-639`) | `get_collation_isdeterministic(inputcollid)` — já validado no M153/M154 | a **regra** (empurra só se collation segura), traduzida a byte-equality determinística |
| Regex | override por-GUC (pode habilitar via custom operator) | **declina sempre** (não temos motor POSIX; DataFusion é RE2/PCRE) | a decisão de tratar regex à parte — mas nós fechamos, não abrimos |
| Oráculo de correção | EXPLAIN + execução dupla pushed-vs-remote (`test/sql/ilike_regex.sql`) | A/B `run_m128` colunar-vs-nativo na MESMA tabela (limit-strip + canonicalize) | o **padrão de oráculo** (execução dupla) |

**Síntese:** reusamos a *lógica de segurança* (quais operadores, collation determinística, regex à parte, oráculo de
execução-dupla) — não o *mecanismo* (SQL deparse). É exatamente a fronteira do ADR D2 do plano (extrair o padrão, não o
código; Regra 9 aplicada ao padrão). O nosso guard de collation (M153/M154) já é a tradução byte-wise correta do
state-machine do FDW — o pg_clickhouse **valida** a decisão que já tínhamos tomado, não a substitui.

## ADRs

### D1 — Const de texto via 2º canal de `Const` nodes em `custom_private` (não bytes crus na IntList)

**Contexto:** `custom_private` é hoje uma `IntList` de 4 ints/pred (`columnar_agg.rs:881-884`); texto varlena não cabe.
**Decisão:** transformar `custom_private` em `List[ IntList_numérica_inalterada, List_de_text_preds ]`, cada text-pred
`= List[ makeInteger(col), makeInteger(op), copyObject(Const*) ]`. O `Const` carrega bytes/tipo/collation e é
copiável/serializável pelos read/out funcs do PG.
**Alternativas:** (a) codificar os bytes do texto como ints comprimento+conteúdo na `IntList` — rejeitada: reinventa
`nodeToString` (Regra 9), frágil a UTF-8/offset. (b) um `offset+bytea` num único `Const bytea` — rejeitada: perde o
`constcollid` e mistura N preds num blob (mais difícil de ler). O node `Const` é o idioma canônico de `fdw_private`
do PG (postgres_fdw). **Consequência:** KISS, copyObject-safe, zero parsing manual; a `IntList` numérica não muda.

### D2 — Guard de collation determinística como pré-condição do pushdown de texto (reuso M153/M154)

**Contexto:** DataFusion filtra byte-a-byte; PG compara sob a collation. Coincidem só sob collation determinística
(PG docs; postgres_fdw `deparse.c:619-639`). **Decisão:** admitir texto `=`/`<>`/LIKE só se
`varcollid == InvalidOid` OU `get_collation_isdeterministic(inputcollid) == true`; senão declinar. Reusa o guard já
validado no group-by (`columnar_agg.rs:407-414`) e COUNT DISTINCT (`columnar_agg.rs:479`), usando `inputcollid` do
`OpExpr` (precisão). **Alternativa:** empurrar sempre e "torcer" — rejeitada (fail-wrong sob ICU não-determinístico).
**Consequência:** correção byte-idêntica garantida; colunas ICU case/accent-insensitive caem no nativo (raras no
ClickBench, que usa `C`/`default`).

### D3 — Regex e ILIKE declinam (honest-negative); LIKE/=/<> empurram

**Contexto:** o motor de regex do DataFusion é o crate Rust `regex` (PCRE-like, `regex/mod.rs:21`,
`regexplike.rs:37`; doc oficial), ≠ POSIX ERE do PG (`~`). ILIKE do PG é locale-aware. **Decisão:** whitelist
fail-closed = { `=`, `<>`, `LIKE`(`~~`), `NOT LIKE`(`!~~`) }; regex (`~`/`!~`/`~*`/`!~*`) e ILIKE (`~~*`/`!~~*`)
declinam para o plano nativo. Espelha o override-de-regex do pg_clickhouse (`shipable.c:197-212`), mas sem GUC (não
temos motor POSIX equivalente — ligar seria fail-wrong). **Alternativa:** implementar POSIX-ERE sobre Utf8 no
DataFusion — rejeitada (YAGNI/esforço desproporcional; M152 já mede regex como fatia menor). **Consequência:** M156
rota `<>`/`=`/LIKE (a maior fatia do M152); regex fica como possível milestone futuro se o benchmark justificar
(Regra 5 / `.claude/rules/discover-phd-rigor.md` R3 UNBENCHMARKED).

## References

**Q1 — collation-safety (≥2 primárias):**
- `.claude/knowledge-base/references/postgres/contrib/postgres_fdw/deparse.c:82-86,363-385,600-639` — state-machine
  `FDW_COLLATE_NONE/SAFE/UNSAFE` + a regra do `OpExpr.inputcollid` (fonte canônica).
- `.claude/knowledge-base/references/pg_clickhouse/src/deparse.c:900-906,927-948` — `foreign_expr_walker`, "all
  collations derive from Vars of the foreign table".
- [PostgreSQL — Collation Support](https://www.postgresql.org/docs/current/collation.html) (WEB/R0): deterministic
  ⟺ byte-sequence equality; pattern matching restrito sob non-deterministic.
- Nosso guard: `theodb_rs/src/am/columnar_agg.rs:407-414` (M153), `:476-479` (M154).

**Q2 — serialização de const-texto + LIKE (≥2 primárias):**
- `.claude/knowledge-base/references/pg_clickhouse/src/deparse.c:3370-3484` (`deparseConst`), `:2882-2887`
  (`deparseStringLiteral`), `:5121-5137` (`deparseOperatorName` — `~~→LIKE`).
- `.claude/knowledge-base/references/postgres/contrib/postgres_fdw/deparse.c` — idiom de `fdw_private` como List de
  Nodes (a base do ADR-M156-1).

**Q3 — o delta (≥2 primárias):**
- `theodb_rs/src/am/columnar_agg.rs:160,182-185,262,298-309,868-894,1120-1186` (admissão + serialização atuais);
  `theodb_rs/src/am/df_executor.rs:269-316` (`build_filter_expr`); `theodb_rs/src/am/zonemap.rs:21-46,88`
  (`ZonePredicate`/`ZoneOp`/`chunk_can_match`).
- `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf` (Abstract + §1) — vectorized execution +
  late materialization / push selection into the scan.
- [String pushdown / column-store execution — DataFusion expressions](https://datafusion.apache.org/user-guide/expressions.html) (WEB/R0).

**Q4 — DataFusion API + semântica (≥2 primárias):**
- `.claude/knowledge-base/references/datafusion/datafusion/physical-expr/src/expressions/like.rs:27-60` (`LikeExpr`);
  `.../datafusion/expr/src/expr_schema.rs:208,376` (`Expr::Like`); `.../functions/src/regex/mod.rs:21`,
  `.../functions/src/regex/regexplike.rs:37` (`use regex::Regex;`).
- [DataFusion — Scalar/Regex functions: "PCRE-like … syntax"](https://datafusion.apache.org/user-guide/sql/scalar_functions.html) (WEB/R0).

**Q5 — oráculo de teste (≥1 primária):**
- `.claude/knowledge-base/references/pg_clickhouse/test/sql/ilike_regex.sql:24-34,47-49` (dataset casing-misto +
  par EXPLAIN/execução-dupla); `.claude/knowledge-base/references/pg_clickhouse/test/sql/in_null_semantics.sql:107-158`
  (`<>` de texto).

**Q6 — whitelist/override de operadores (≥1 primária):**
- `.claude/knowledge-base/references/pg_clickhouse/src/shipable.c:142-145,147-175,181,197-212,356-377`
  (`chfdw_is_builtin`, `regex_flags_ok`, `chfdw_is_shippable`, override-regex-por-GUC, whitelist fail-closed).

**Rules citadas:** `.claude/rules/discover-phd-rigor.md` (R0/R3), `.claude/rules/architecture.md` (fronteiras do
CustomScan), `.claude/rules/testing.md` §4.1 (edge vs. negative), `.claude/rules/error-handling.md` (fail-closed),
`.claude/rules/parsimony-ladder.md` (Regra 9 rung 4). Benchmark de origem: `docs/benchmarks/m152-routing-map.md`.
