---
slug: m156-text-where
milestone_id: M156
created_at: 2026-07-25
goal: Rotear predicados de texto no WHERE (=, <>, LIKE, NOT LIKE) ao CustomScan colunar via filtro DataFusion sobre Utf8, subindo a cobertura ClickBench acima de 21 com A/B byte-idêntico ao heap.
---

# Plano — M156: Rotear predicados de texto no WHERE ao CustomScan colunar

## Goal

Serializar um predicado de texto (`col = 'x'`, `col <> ''`, `col LIKE '%p%'`, `col NOT LIKE 'a%'`) no `custom_private`
e construir o filtro DataFusion equivalente sobre a coluna Utf8 decodificada — **medido:** `columnar_customscan_count`
sobe acima de 21 (as ~4-8 queries text-WHERE do ClickBench que o M152 mediu), com `result_ab.diverged == 0` byte-idêntico.

## Context

O M152 mediu `unpushable_where_qual` como o maior first-blocker (8 queries: q12,14,20,27,30,31,36,37). O blueprint
`columnar-text-where-pushdown` (discover, SHIPPABLE_WITH_CAVEATS) desenhou o M156 a partir do pg_clickhouse/postgres_fdw
(shippability + collation-safety) + DataFusion (`Expr::Like`/`LikeExpr`). Hoje `extract_zone_predicate`
(`columnar_agg.rs:160`) só serializa numérico (`u64` zone-bits); texto (varlena) não cabe.

## Prior Art & Related Work

- Blueprint `columnar-text-where-pushdown-blueprint.md` (discover, este ciclo) — o desenho + fontes primárias.
- pg_clickhouse `src/shipable.c`/`deparse.c` (Apache-2.0), postgres_fdw `deparse.c` (state-machine collation), DataFusion `physical-expr/.../like.rs`.
- M151 (extract_zone_predicate + o padrão de A/B), M153/M154 (o guard `get_collation_isdeterministic`), M149 (o CustomScan de projeção que já filtra).

## Baseline Context

### Files that will be touched

| Arquivo | LoC | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~1690 | `extract_zone_predicate`(:160), `extract_all_predicates`(:298), `encode_private`(:868), `decode_private`(:1040) — só numérico | + extração de predicado de texto (guardado por collation/tipo/operador) + 2º canal de serialização (Const nodes) |
| `theodb_rs/src/am/df_executor.rs` | ~770 | `build_filter_expr`(:269) — só literais numéricos/temporais | + braço de texto: `eq`/`not_eq`/`like`/`not_like` sobre `col` Utf8 com `ScalarValue::Utf8` |

### Current callers / dependents

- `extract_all_predicates` (columnar_agg.rs:298) — chamado por `build_admission`; hoje coleta `Vec<ZonePredicate>` (numérico). O texto entra como uma coleção paralela `Vec<TextPredicate>`.
- `encode_private`/`decode_private` — serializam a `IntList` `custom_private`. O texto precisa de um 2º canal (Const nodes via `copyObject`) — o custom_private vira `List[IntList_atual, List_de_text_preds]`.
- `build_filter_expr` (df_executor.rs:269) — recebe os predicados e monta o `Expr` do DataFusion; ganha o braço texto.
- `run_columnar_grouped_aggs`/`run_columnar_aggs` — passam os predicados ao filtro; ganham o parâmetro de text-preds.

### Domain glossary

- **TextPredicate:** `{ col: usize, op: TextOp, needle: String }` — `col OP 'needle'`, `TextOp ∈ {Eq, Ne, Like, NotLike}`. Só filtro (NÃO poda chunk-group, como o `Ne` numérico).
- **2º canal (custom_private):** hoje `custom_private` é uma `IntList`. O texto (varlena) não cabe em int. Solução (per o blueprint, seção ADRs): `custom_private = List[<IntList numérica atual>, <List de text-preds>]`, cada text-pred = `[Integer(col), Integer(op_code), copyObject(Const*)]` (o Const sobrevive via copyObject — idiom fdw_private).
- **Guards:** collation determinística (`get_collation_isdeterministic(inputcollid)`), tipo texto/varchar (25/1043 — bpchar 1042 EXCLUÍDO, M153), operador `=`/`<>`/`~~`(LIKE)/`!~~`(NOT LIKE); ILIKE (`~~*`) e regex (`~`/`!~`/`~*`/`!~*`) DECLINAM.

### Architecture boundaries affected

Nenhuma nova. Estende a serialização `custom_private` (mesma fronteira do M114/M115) + o `build_filter_expr` (M100). Sem novo formato de página, sem novo write.

## ADRs

### ADR-1 — Const de texto via 2º canal de `Const` nodes (não bytes crus na IntList)
- **Decisão:** `custom_private` vira `List[IntList_atual, text_preds_List]`; cada text-pred = `[Integer(col), Integer(op), copyObject(Const*)]`.
- **Rationale:** o Const de texto é varlena (comprimento variável) — não cabe na `IntList`. `copyObject` o torna sobrevivível ao ciclo de plano (idiom `fdw_private` do PG, validado no pg_clickhouse). KISS/Regra 9 (reusar o mecanismo de nodes do PG, não inventar um blob).
- **Alternativa rejeitada:** serializar os bytes do texto como ints na IntList — frágil (encoding manual de comprimento/UTF-8, propenso a bug); rejeitada. Rejeitada também: um GUC/tabela lateral — YAGNI.

### ADR-2 — Guard de collation determinística + whitelist de operador (reuso M153/M154 + blueprint)
- **Decisão:** empurrar texto sse `get_collation_isdeterministic(inputcollid)` E operador ∈ {`=`,`<>`,`~~`,`!~~`} E tipo ∈ {25,1043}. ILIKE + regex declinam.
- **Rationale:** DataFusion agrupa/compara byte-wise; casa o PG só sob collation determinística (fonte primária `varlena.c` + postgres_fdw). ILIKE é locale-aware; regex do DataFusion é RE2/PCRE ≠ POSIX ERE do PG (confirma M152) → fail-safe declina.
- **Alternativa rejeitada:** empurrar ILIKE/regex com "melhor esforço" — divergência silenciosa; rejeitada (o mesmo erro que os reviews M151/M154 pegaram).

## Dependency Graph

```
Fase 1 (extract_text_predicate + guards, columnar_agg) ─→ Fase 2 (2º canal encode/decode, columnar_agg) ─→ Fase 3 (braço texto no build_filter_expr, df_executor) ─→ Fase 4 (build no droplet + A/B + cobertura)
```

## Phase 1 — Extração + guards do predicado de texto

### T1.1 — `extract_text_predicate` (op/tipo/collation guards)

#### Why this step
É o gate de correção: só predicados de texto PROVADAMENTE seguros entram. Raciocínio: ADR-2 + blueprint Corner 3/4.

#### Concurrency tests
(none — single-threaded) roda no planner.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `extract_text_predicate(clause, relid) -> Option<TextPredicate>`: reconhece `OpExpr(Var_texto, Const_texto)`; guard tipo (25/1043, exclui 1042), operador (mapear aggfnoid/opno → Eq/Ne/Like/NotLike; ILIKE/regex → None), collation determinística (`inputcollid`); const não-NULL. `extract_all_predicates` retorna também `Vec<TextPredicate>`.

#### TDD
- **RED:** harness A/B — `WHERE phrase = 'p1'` / `<> ''` / `LIKE '%p%'` / `NOT LIKE 'a%'` roteia (`theodb_columnar` no EXPLAIN) e A/B == heap. Falha antes (declina `unpushable_where_qual`).
- **GREEN:** extração + guards.
- **REFACTOR:** provar declínios: ILIKE, regex `~`, collation ICU não-det, bpchar, const-NULL → todos declinam ao nativo, A/B correto.

#### Acceptance criteria
- [ ] `WHERE text_col = 'x'`/`<> ''`/`LIKE '%p%'`/`NOT LIKE 'a%'` (collation determinística) roteia (verificado por: EXPLAIN mostra o CustomScan colunar).
- [ ] A/B byte-idêntico vs heap em cada operador (verificado por: A/B no pg_test + run_m128).
- [ ] ILIKE, regex, collation não-determinística, bpchar, const-NULL DECLINAM (verificado por: EXPLAIN nativo + A/B correto).

#### DoD
- Testes passam (RED→GREEN no droplet).

## Phase 2 — 2º canal de serialização (Const nodes)

### T2.1 — encode/decode do canal de text-preds

#### Why this step
O texto não cabe na IntList; precisa do 2º canal copyObject-safe. Raciocínio: ADR-1.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `encode_private` embrulha `List[IntList_atual, text_preds_List]`; cada text-pred = `[Integer(col), Integer(op), copyObject(Const*)]`. `decode_private` desembrulha, reconstrói `Vec<TextPredicate>` (extrai a `String` do `Const` via `TextDatumGetCString`/varlena). Backward-compat: ausência do 2º canal → 0 text-preds.

#### TDD
- **RED:** `WHERE phrase LIKE '%p%'` executa sem `bad private` e A/B == heap (o round-trip do Const funciona). Falha antes.
- **GREEN:** encode/decode do canal.
- **REFACTOR:** round-trip de string com `%`/`_`/`\`/vazia/UTF-8 multibyte preservado byte-a-byte.

#### Acceptance criteria
- [ ] Round-trip do Const de texto correto (verificado por: a query executa + A/B == heap; strings com `%_\`, vazia, UTF-8).
- [ ] Backward-compat: query sem text-pred continua roteando (verificado por: as 21 anteriores diverged=0).

#### DoD
- Round-trip provado; regressão zero nas 21.

## Phase 3 — Braço de texto no filtro DataFusion

### T3.1 — `build_filter_expr` texto (eq/not_eq/like/not_like)

#### Why this step
O executor precisa avaliar o predicado de texto sobre a coluna Utf8. Raciocínio: blueprint Corner 2.

#### Concurrency tests
(none — single-threaded) DataFusion single-thread neste caminho.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — `build_filter_expr`: para cada TextPredicate, `col(name).eq(lit(ScalarValue::Utf8(Some(needle))))` / `.not_eq` / `.like(lit(...))` / `.not_like(lit(...))`; combinar com os predicados numéricos via `and`. `escape_char` do LIKE setado para casar o default do PG (`\`).

#### TDD
- **RED:** o A/B de `LIKE '%p%'` (T1.1) passa a byte-idêntico (o filtro avalia). Falha antes (braço inexistente).
- **GREEN:** o braço texto.
- **REFACTOR:** `LIKE 'a\%b'` (escape) byte-idêntico ao PG; `= ''` e `<> ''` corretos; combinação texto+numérico no mesmo WHERE.

#### Acceptance criteria
- [ ] `LIKE`/`=`/`<>`/`NOT LIKE` byte-idêntico vs heap (verificado por: A/B).
- [ ] Escape do LIKE casa o PG (`LIKE 'a\%b'`) (verificado por: A/B).
- [ ] `cargo build` exit 0, clippy limpo (verificado por: exit codes).

#### DoD
- A/B byte-idêntico; build verde.

## Phase 4 — A/B + cobertura medida

### T4.1 — Gate A/B ClickBench + cobertura + CHANGELOG

#### Why this step
DoD measurement-first: cobertura sobe acima de 21, diverged=0 nos dois regimes.

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0`.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `docs/benchmarks/m156-text-where.md` (NEW) + `docs/benchmarks/m156-artifacts/`; `benchmarks/m156_ec_harness.sql` (NEW); `CHANGELOG.md`.

#### TDD
- **RED:** `run_m128 --agg` mostra `columnar_customscan_count > 21` + `diverged == 0`. Antes, 21.
- **GREEN:** rodar no droplet (head + systematic).
- **REFACTOR:** listar honestamente quais queries text-WHERE rotearam + quais permanecem honest-negative (regex/LIKE composto).

#### Acceptance criteria
- [ ] `columnar_customscan_count > 21` (verificado por: o JSON do run).
- [ ] `result_ab.diverged == 0` nos dois regimes head+systematic (verificado por: os 2 JSON).
- [ ] CHANGELOG `[Unreleased]` cita M156 (verificado por: `grep -c M156 CHANGELOG.md` >= 1).
- [ ] Zero droplets efêmeros (verificado por: `doctl compute droplet list`).

#### DoD
- `docs/benchmarks/m156-text-where.md` com a cobertura medida (>21) + diverged=0.

## Coverage Matrix

| Requisito (DoD do ROADMAP M156) | Task |
|---|---|
| Cobertura columnar_customscan sobe (>21) | T4.1 |
| A/B byte-idêntico vs heap (`=`/`<>`/LIKE/NOT LIKE) | T1.1, T3.1, T4.1 |
| Guard collation determinística + operador (ILIKE/regex/bpchar declinam) | T1.1, ADR-2 |
| Serialização de const-texto (2º canal) | T2.1, ADR-1 |
| Escape do LIKE casa o PG | T3.1 |
| CHANGELOG | T4.1 |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Escape do LIKE do DataFusion ≠ default do PG (`\`) → divergência | ALTA | Setar `escape_char='\'`; A/B com `LIKE 'a\%b'` (T3.1) | implementer |
| copyObject do Const não sobrevive / decode extrai string errada (UTF-8/varlena) | ALTA | Round-trip provado (T2.1) com `%_\`/vazia/multibyte; `TextDatumGetCString` correto | implementer |
| ILIKE/regex acidentalmente empurrado → divergência silenciosa | ALTA | Whitelist de operador fail-closed (ADR-2); A/B provando o decline (T1.1) | reviewer |
| Combinação texto+numérico no mesmo WHERE mal-combinada (AND) | MÉDIA | `and` explícito no build_filter_expr; A/B com WHERE misto (T3.1) | implementer |

## Unresolved Questions

- (none — every decision is resolved at plan time). A forma exata do decode do Const de texto (`TextDatumGetCString` vs `text_to_cstring`) será confirmada por leitura no implement; o A/B de round-trip (T2.1) é o gate.

## Global DoD

- [ ] Testes verdes (RED→GREEN no droplet); clippy limpo.
- [ ] `run_m128 --agg` diverged=0 + count > 21 (head + systematic).
- [ ] Benchmark em `docs/benchmarks/m156-*`.
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG.
- [ ] Droplet destruído.

## Final Phase — Integration Validation

Build no droplet + A/B das 43 (diverged=0, count>21) + prova dos declínios (ILIKE/regex/bpchar/collation-não-det) +
round-trip do Const. A cadeia só está completa quando o A/B é byte-idêntico E a cobertura sobe E os guards declinam o
que devem. Falha → volta ao implement.
