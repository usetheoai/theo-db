---
slug: cc-hotspots-refactor
milestone_id: M145
date: 2026-07-23
plan: .claude/knowledge-base/plans/cc-hotspots-refactor-plan.md
status: IMPLEMENTATION_COMPLETE
---

# Implementation — M145 Refactor dos hotspots de CC

Decompõe os 4 hotspots refactor-worthy de `theodb_rs` para CC ≤ 25 por Extract Function,
comportamento **byte-idêntico preservado**. Sequência risco-ascendente (ADR-3). Validação:
`lizard` (métrica de CC, o aceite mecânico), `cargo check --features pg18` (compila) + build
release completo (`cargo pgrx install --release`, exit 0) + smokes A/B no droplet PG18.4.

## Tarefas + evidência medida (droplet, 2026-07-23)

| Task | Fn | CC (lizard) | Helpers extraídos | Prova comportamental |
|---|---|---|---|---|
| **T1.1** | `write_parquet_impl` (`parquet.rs`) | **35 → 19** | `col_builder_for`/`append_row`/`finish_arrays`/`atomic_write_parquet` + enum `Col` içado | `write_parquet('pq')` → 2 rows + `read_parquet` → 2 rows (roundtrip) |
| **T1.2** | `theodb_embed_worker_main` (`vectorizer.rs`) | **41 → 14** | `reap_and_purge`/`claim_batch`/`renew_lease`/`process_one`/`process_group` | release build exit 0; limites de txn M122/H-1/H1 movidos intactos; fns SQL inalteradas (worker async live precisa de endpoint de embed — não smokado; extração é pura) |
| **T1.3** | `main_index_pages` (`am/page/mod.rs`) | **34 → 11** | 4 helpers verbatim `pending_start_v4`/`_v5_v7`/`_v6_v8`/`_v2_v3` (ADR-2, não unificados) | Index Scan usa `theodb_ivfflat` → top-20 correto; após INSERT na pending region → 20/20 (parser de offset correto — misparse perderia linhas) |
| **T1.4** | `admit` (`am/columnar_agg.rs`) | **59 → 17** | `parse_agg_kind`/`classify_target_node`/`build_admission` + enum `TargetSlot` | **CRÍTICO M115:** com `enable_columnar_agg=on`, EXPLAIN mostra `Custom Scan (theodb_columnar_agg)` (admit admitiu) E o A/B `col EXCEPT heap` = **0/0 byte-idêntico** para count / sum(int4→int8) / sum(int8→numeric) / avg(float8) / avg(int→numeric) / min(int8) / max(ts) + WHERE+GROUP BY combinado |

## Integração (T2.1)

- **CC gate:** os 4 alvos ≤ 25 medido por `lizard <arquivo> -l rust` (o mesmo comando do audit). **Nenhuma NOVA fn CC>25 introduzida** — os helpers são 2–24; o único CC>25 nos arquivos tocados é `begin_custom_scan` (26), **pré-existente** (era 26 em HEAD~4/v0.134.0, um dos 15 hotspots que o audit julgou complexidade essencial, NÃO refactor-worthy).
- **Compila:** `cargo check --features pg18,pg_test --tests` exit 0 por task; build release `cargo pgrx install --release` exit 0.
- **Zero mudança de superfície SQL:** mesmas assinaturas `#[pg_extern]` (`write_parquet`, etc.); a extensão continua 1.2.0, o A/B usa o mesmo `.so`.
- **Válvula honest-negative (ADR-3):** NÃO acionada — os 4 alvos renderam ganho real de legibilidade E redução de CC medida. `main_index_pages` foi o caso mais próximo (verbatim-move, não simplificação de lógica) mas o ganho de naming + CC é real (ADR-2).

## Byte-idêntico — a prova central (T1.4 admit / M115)

O risco #1 do plano era regredir o Agg-swap byte-idêntico do M115 ao refatorar o `admit`. A
extração preservou a ORDEM de decisão (preamble → walk → grouped-empty → mode), TODO ponto de
`None`/`?`, os `layout.push((_, len()))` na ordem do target, e columnar-antes-de-heap. Prova
medida no droplet: com o CustomScan colunar ATIVO (`enable_columnar_agg=on`, confirmado no
EXPLAIN), a saída é byte-a-byte igual ao heap native (`EXCEPT` bidirecional = 0 linhas). Sem o
GUC, ambos rodam native (A/B trivial) — por isso a prova exige o GUC ON + o EXPLAIN mostrando
`Custom Scan (theodb_columnar_agg)`.

## Nota honesta (Regra 3)

T1.2 (worker) não tem smoke async live (precisaria de endpoint de embed real + timing do poll
loop). É uma extração PURA (as 2 closures viram fns, o 3-phase embed vira `process_group`, os
limites de transação são move-block intactos) que compila e cujas fns SQL (claim/mark/process/
renew) são inalteradas — testadas em M122/M144. O risco comportamental é baixo; a prova é
compilação + release build + preservação estrutural verificável no diff.
