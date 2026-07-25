# Discovery Plan: Late materialization de colunas de saída no scan colunar (M158)

**Slug:** columnar-late-materialization
**Owner:** paulohenriquevn
**Created:** 2026-07-25
**Time budget:** 5h (breakdown in ADR D1)

## Context

O M148 (flamegraph, `docs/benchmarks/m148-flamegraph-scan.md`) mediu que o scan colunar é **100% CPU-bound** e **~80% do
tempo é materializar cada linha como heap-tuple** (`palloc`+`heap_form_tuple` por linha em `form_row`/`decode_stripe`,
`columnar.rs:671,698,790`). O M149 adicionou `want_mask` (projeção — só materializa as colunas do targetlist∪qual), mas
no regime `SELECT <cols> … ORDER BY key LIMIT k` TODAS as linhas são materializadas (o Sort puxa tudo do scan) e só `k`
sobrevivem. O M155 (spike Top-N, honest-negative) apontou este lever explicitamente: **decodificar só a chave de
ordenação p/ todas as linhas, materializar as demais colunas só p/ o top-k** (late materialization à C-Store/MonetDB).
Este é o ÚNICO caminho que muda o *tempo* do path já-colunar (não a cobertura). É measurement-first e aceita
honest-negative (o overhead do re-fetch/CustomScan pode comer o ganho). Cita `rules/discover-phd-rigor.md` (R0 web) +
`rules/architecture.md` (fronteiras do CustomScan) + o `theodb-evolution` (five-question gate + invariantes MVCC).

## Objective

Produzir um blueprint que responda **se e como fazer late materialization no regime `ORDER BY key LIMIT k` do scan
colunar — decodificar só a chave (+ um row-locator) para todas as linhas, top-k, e materializar as colunas restantes só
para o top-k — preservando MVCC/byte-identidade, com o DESENHO concreto (CustomScan scan+sort+limit vs alternativa) e um
VEREDITO de viabilidade (viável / viável-com-restrições / não-viável / honest-negative) ancorado no custo medido do
re-fetch vs o ganho de materialização evitada**.

## In-Scope / Out-of-Scope

### In-Scope

- `.claude/knowledge-base/references/papers/cstore-stonebraker-2005.pdf` (late vs early materialization — a fonte).
- `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf` (vectorized + materialization).
- `theodb_rs/src/am/columnar.rs` (`form_row`, `decode_stripe`, `load_next_batch`, o row-locator/stripe+offset), `am/scan.rs`.
- `docs/benchmarks/m148-flamegraph-scan.md` + `benchmarks/profile_columnar_scan.sh` (o baseline medido + o harness de flamegraph).
- DataFusion `physical-plan/src/topk/` (TopK — como o top-k é feito vetorizado, se reusável).

### Out-of-Scope (explicit)

- Reescrever o TableAM scan geral (late-mat é do regime ORDER-BY-LIMIT, não do scan-tudo).
- Índices / HNSW / vetorial.
- Agregação (o agg CustomScan M100 já não materializa row-by-row para o executor).

## ADRs

### D1 — Time budget + stop conditions

- papers C-Store/MonetDB: 2h (o padrão late-vs-early + o custo do re-fetch). scan path + M148 baseline: 2h. web (R0): 1h.
- Stop por questão: cita `arquivo:linha`/`PDF` OU `[BLOCKED]`. O veredito de viabilidade é OBRIGATÓRIO (não deixar "a definir").

### D2 — Measurement-first: o veredito pode ser honest-negative

- Como o M155, este discover PODE concluir não-viável/honest-negative se o custo do re-fetch (seek de volta ao stripe
  para materializar o top-k) + o overhead do CustomScan superar o ganho de não-materializar N−k linhas. O blueprint DEVE
  estimar/medir isso, não assumir o ganho. Anti-sunk-cost (CLAUDE.md).

## Research Questions

| # | Question | Corner | Reference | Fase A (structural) | Fase B (read) | Answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o C-Store define late vs early materialization e QUANDO late vence (o critério de seletividade/largura)? | techniques | `.claude/knowledge-base/references/papers/cstore-stonebraker-2005.pdf` | leitura do paper (seção materialization strategies) | Read + WEB (R0): "late materialization column store" | O critério exato (quando late vence early) → aplica ao nosso ORDER-BY-LIMIT? |
| Q2 | Qual o DESENHO no nosso path — um CustomScan que substitui Scan+Sort+Limit, decodifica só a chave + row-locator (stripe,offset) p/ todas as linhas, top-k, e re-materializa o top-k? Qual o row-locator disponível hoje? | techniques | `theodb_rs/src/am/columnar.rs` | `grep -nE "stripe|offset|ctid|ItemPointer|form_row|decode_stripe|want_mask" columnar.rs` | Read `form_row`/`decode_stripe`/o row-locator | Desenho concreto do CustomScan + como re-materializar k linhas por (stripe,offset) |
| Q3 | Qual o CUSTO do re-fetch (materializar k linhas por row-locator) vs o ganho (não materializar N−k)? É viável, viável-com-restrições, ou honest-negative? | techniques | `docs/benchmarks/m148-flamegraph-scan.md` | Read o baseline M148 (~104ms/13005 linhas materializar) + `benchmarks/profile_columnar_scan.sh` | leitura do harness + WEB (R0): custo de random-access em column store | Estimativa: ganho ≈ (N−k)/N × 80% do scan; custo re-fetch ≈ k × (decode+materialize) — veredito |
| Q4 | O DataFusion TopK (`physical-plan/src/topk`) é reusável para o top-k da chave, ou o PG top-N heapsort já basta (M155 mediu que o PG já usa)? | deps | `.claude/knowledge-base/references/datafusion` | `grep -rn "TopK|heap" physical-plan/src/topk/` | Read topk + WEB (R0) | Decisão: reusar TopK vs PG heapsort vs Rust BinaryHeap próprio |
| Q5 | Como PROVAR a byte-identidade + MVCC (o top-k re-materializado é idêntico ao eager) e como flamegraph-medir o antes/depois? | tests | `benchmarks/profile_columnar_scan.sh` | Read o harness M148 (folded/selftime) + `run_m128` A/B | leitura + o oráculo A/B | Método: flamegraph antes/depois (materialização cai) + A/B diverged=0 + MVCC (mesma linha que o eager) |
| Q6 | Como o MonetDB/X100 lida com o trade-off vetorizado (materializar tarde num modelo colunar) — há armadilha (cache, TLB) que invalida o ganho? | tools | `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf` | leitura do paper | Read + WEB (R0): "vectorized late materialization pitfalls" | Armadilhas conhecidas → o que medir p/ não dar falso-ganho |

## Coverage Matrix

| Coverage Corner | Questions | Status |
|---|---|---|
| Integration tests | Q5 | Covered |
| Dependencies | Q4 | Covered |
| Tools | Q6 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

Z = 100%; techniques ≥ 2 (perfil frontier, `discover-phd-rigor.md`).

## Halt-loop Checkpoints

- Antes de DONE: resposta cita `arquivo:linha`/PDF real OU `[BLOCKED]`.
- Q3: o VEREDITO de viabilidade DEVE ser explícito (viável/viável-com-restrições/não-viável/honest-negative) com números estimados — o ponto do milestone.

## Acceptance Criteria

- [ ] Toda questão respondida com ≥1 citação que resolve (Q1/Q3/Q6 papers; Q2 columnar.rs; Q4 datafusion; Q5 harness).
- [ ] R0 honrado: ≥1 fonte web nos métodos de Q1/Q3/Q4.
- [ ] O blueprint dá o desenho do M158 (CustomScan late-mat + row-locator + re-fetch) E o veredito de viabilidade com custo estimado.
- [ ] 4 coverage corners populados; ≥1 ADR sintetiza a decisão (incl. o critério honest-negative).

## Global Definition of Done

- [ ] `/discover-plan-confidence` ≥ SHIPPABLE_WITH_CAVEATS.
- [ ] `/discover-execute` produz `knowledge-base/discoveries/blueprints/columnar-late-materialization-blueprint.md`.
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS.
- [ ] ADRs referenciam ≥1 rule (`architecture.md`, `discover-phd-rigor.md`) + o five-question gate do `theodb-evolution`.
