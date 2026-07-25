# Blueprint M149 — Projection pushdown no scan colunar (via CustomScan)

Fonte: discover M149 (council-index-storage sobre source primário local — Citus + PG18 + TheoDB). 2026-07-24.

## Descoberta arquitetural (LOCKED)

**Um TableAM puro do PG18 NÃO consegue saber as colunas projetadas.** `scan_begin`
(`references/postgres/src/include/access/tableam.h:334`) recebe só `Relation, Snapshot, nkeys, ScanKeyData*,
ParallelTableScanDesc, flags` — nenhum bitmap de colunas. Os `flags` (`ScanOptions`) não carregam projeção. O
slot de destino (`ss_ScanTupleSlot`) é sempre de largura total; a projeção do targetlist acontece ACIMA do nó
de scan (via `ExecProject`). **Logo, projection pushdown exige um CustomScan** que veja `ps.plan` — igual ao
Citus. O TableAM puro fica como fallback all-columns.

## Prior art (Citus, source primário local)

- Citus `columnar_beginscan` puro = `bms_add_range(NULL, 0, natts-1)` (todas) — `references/citus/.../columnar_tableam.c:193`.
- Projeção vem via `ColumnarScan` CustomScan (`set_rel_pathlist_hook`); no exec computa `ColumnarAttrNeeded`.
- **`ColumnarAttrNeeded` (`columnar_customscan.c:1814`)** = `pull_var_clause` sobre **`plan->targetlist` E `plan->qual`** concatenados → 1 bitmap. `varattno==0` → todas (fail-safe); `varattno<0` (system col) → erro/fallback.
- `columnar_getnextslot` preenche só `attr_needed`; colunas não-pedidas ficam intocadas no slot (nunca lidas pelo nó superior → resultado idêntico).

## Estado atual do TheoDB (file:line) — peças reusáveis

- Alvo (caminho A, SeqScan): `decode_stripe` (`columnar.rs:684`, loop `0..natts`), `form_row` (`:650`, heap_form_tuple de 105 col), `load_next_batch` (`:1026`, monta `cols` p/ todas), `getnextslot` (`:1105`).
- **Já projeta (caminho B, M100/agg):** `decode_columns` (`columnar.rs:756`, aceita `projection: Option<&[usize]>`, pula zstd das não-projetadas) + `deform_rows_into_columns` (`:564`, materializa só `wanted`).
- **Hook vivo:** `set_rel_pathlist_hook` + `PREV_HOOK` encadeável (`customscan.rs:264`).
- **Introspecção de Var/targetlist/qual:** `columnar_agg.rs` (o `ColumnarAttrNeeded` do TheoDB já parcialmente existe).

## Design M149 (mínimo, KISS)

1. **CustomScan de projeção** via o `set_rel_pathlist_hook` existente, para RTEs sobre `theodb_columnar`, quando a query NÃO é roteada pelo columnar_agg (projeção pura/filtro, sem agg).
2. **`wanted` = `pull_var_clause(targetlist) ∪ pull_var_clause(qual)`** (0-based). `varattno==0` → todas (fail-safe); `varattno<0` (system col) → fallback decode-tudo.
3. **Empurrar `wanted`** até `decode_stripe`/`load_next_batch`/`form_row` — materializar heap-tuples só com as colunas de `wanted` preenchidas (resto `isnull=true`). Reusar `deform_rows_into_columns`. Ataca o frame dominante do M148 (`form_row` de N em vez de 105) **e** o decode (~7%).
4. **Fallback**: sem CustomScan (path perdeu / `varattno<0` / lista indisponível) → `columnar_scan_begin` puro decodifica tudo (intacto). O CustomScan é aditivo, nunca o único caminho.

## Invariantes (evolution-gate)

- **A/B byte-idêntico vs heap nas 43 queries do ClickBench** — vale porque `wanted = targetlist ∪ qual`: colunas não-preenchidas nunca são lidas pelo nó superior. Gate obrigatório (Rule 5), in-PG por query.
- **Fallback decode-tudo** é o piso de correção.
- **MVCC/crash intactos:** M149 não toca o conjunto de stripes visíveis nem o tail pending; só reduz QUAIS colunas cada linha materializa. Ordem de linhas byte-idêntica.

## Risco principal (sinalizado pelo usuário)

**Unir projeção + filtros.** Derivar `wanted` só do targetlist descartaria a coluna do filtro
(`SELECT count(*) WHERE advengineid<>0` → `wanted={}` → resultado errado). Mitigação = Citus: `targetlist ∪
qual ∪ colunas de predicados empurrados (zone-map)`, fail-safe para todas se qualquer `Var` não resolver.

## ADR implícito

M149 é um **CustomScan**, não uma mudança no TableAM — porque o TableAM não vê as colunas (§ Descoberta).
Alternativa rejeitada: hackear o `scan_begin` — impossível pelo contrato do PG18.
