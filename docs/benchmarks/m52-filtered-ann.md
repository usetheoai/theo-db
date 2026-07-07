# M52 — Filtered ANN: recall sob `WHERE` seletivo (theodb iterative scan vs pgvector 0.8)

**Date:** 2026-07-07 · **Milestone:** M52 · **Metric:** cosine · **GT:** seqscan exato + filtro · **Image:** `theodb:m52` (vs pgvector 0.8.3)
**Harness:** `benchmarks/run_m52_filtered_ann.py` (reusa `theodb_bench.metrics`, espelha M50/M51) · **n=25 000, dim=128, k=10, 3 runs**
**Verdict:** **No filtro SELETIVO (1%) — onde o iterative scan é o ponto — theodb ATINGE PARIDADE com o pgvector 0.8 (recall 0.971 ≥ 0.964).** O gate do DoD (recall sob filtro seletivo ≥ paridade pgvector) é cumprido.

---

## ⚠️ Caveats (Rule 3)

Escala tratável (25k×128 gaussiano) numa box **contendida** (`load_pre=9.47`, por-run `[9.3, 11.9, 13.9]`). Os números de **QPS carregam ruído** de contenção; o **recall é determinístico** (seed fixo, GT exato) e independente de carga — é o eixo confiável e o gate do M52. O ganho ≥2× de QPS não é um objetivo do M52 (é do M51).

## 1. Recall@10 por seletividade (theodb iterative vs pgvector 0.8 relaxed_order, mean±std, 3 runs)

| seletividade | filtro | theodb recall | pgvector recall | paridade (±0.01) | theodb p50 | pgvector p50 |
|---|---|---|---|---|---|---|
| **~1%** | `cat = 7` | **0.9713 ± 0.002** | 0.9640 ± 0.003 | ✅ **SIM** | 42.8 ms | 14.6 ms |
| ~10% | `cat < 10` | 0.5973 ± 0.009 | 0.5873 ± 0.006 | ✅ SIM | 3.9 ms | 3.0 ms |
| ~50% | `cat < 50` | 0.5873 ± **0.032** | 0.5773 ± 0.008 | ✅ SIM | 3.5 ms | 2.3 ms |

## 2. Leitura

- **O gate É o caso SELETIVO (1%), e ele PASSA.** Sob `cat = 7` (~250 de 25 000 linhas), o HNSW ingênuo (≤ ef_search=100 tuplas) quase nunca acha 10 linhas que passam o filtro nos 100 candidatos mais próximos → recall colapsaria. O **iterative scan** (re-busca com ef crescente até `max_scan_tuples`) recupera: **theodb 0.971 ≥ pgvector 0.964** (±0.002 sobre 3 runs, baixa variância). Este é o gap CRÍTICO que o DoD ataca, e theodb iguala o SOTA permissivo (pgvector 0.8 `relaxed_order`). Que o iterative scan é o que recupera aqui está provado pelo pg_test `iterative_scan_off_when_max_scan_tuples_zero` (com o knob em 0 o path degrada) + `filtered_scan_preserves_recall_via_iterative` (com o knob on, index top-k == exato).
- **A 10%/50% também passam paridade nesta run (theodb marginalmente À FRENTE: 0.597 vs 0.587; 0.587 vs 0.577) — mas NÃO são o gate e o delta NÃO é conclusivo.** Estes NÃO são filtros altamente seletivos: a ef=100 já sobram candidatos que passam, então o recall medido é próximo do **recall base do HNSW a ef=100** (~0.59 para ambos, não-seletivo). O delta de ±0.01 aqui está dentro da **variância de run committada** — o `recall_std` do theodb a 50% é **0.032** (grande; ver a tabela), então o sinal do delta theodb-vs-pgvector oscila com a run/query set e NÃO é um déficit sistemático. A **base confiável (M50, 50q×3runs)** é theodb **0.6227** vs pgvector **0.590** — theodb marginalmente à FRENTE unfiltered, consistente. Honestidade (Rule 3): uma versão anterior deste artefato (a) citou os números do M50 invertidos e (b) afirmou controles ON/OFF/multi-seed como "medidos" sem código/raw committado — ambos retirados; o que fica é o que o harness committado produz. Um controle multi-seed formal (ON/OFF + seeds) é follow-up rastreado no backlog.
- **QPS: theodb ~3× mais lento que pgvector no caso seletivo** (42.8 vs 14.6 ms) porque, por ADR-1 (KISS), o iterative scan do theodb **re-busca o grafo inteiro com ef dobrado** a cada esgotamento, enquanto o pgvector 0.8 **resume do `discarded` set** (não re-percorre). Trade-off documentado: paridade de RECALL agora; a otimização resume-from-discarded fica como follow-up rastreado (backlog).

## 3. VEREDITO (DoD)

**Cumprido:** recall sob filtro seletivo (1%) **≥ paridade pgvector 0.8** (0.971 ≥ 0.964, baixa variância) — o iterative scan do theodb funciona e iguala o SOTA permissivo no caso que importa. Nesta run as 3 seletividades passam paridade (±0.01). `EXPLAIN` prova `Index Scan` sob `WHERE` (pg_test `filtered_scan_preserves_recall_via_iterative`). Zero regressão no path unfiltered (a suíte M45/M50 usa `LIMIT k` com ef≫k → o grow nunca dispara; pg_test `traverse_presize` + `iterative_scan_off` verdes).

**Honesto sobre o que NÃO é conclusivo:** (a) a 10%/50% o delta theodb-vs-pgvector (~+0.01 nesta run) é pequeno, não-seletivo e dentro da variância de run committada (`recall_std` até 0.032) — nem vitória nem déficit sistemático; um controle multi-seed formal é follow-up; (b) o QPS do theodb no caso seletivo é ~3× o do pgvector (re-busca vs resume) → follow-up ADR-1; (c) o `parity_gate` do harness usa tolerância de 0.01 (`theodb >= pgvector − 0.01`), explicitada aqui.

## 4. Metodologia / reprodução

```bash
PGPORT=<port> PGOPTIONS='-c statement_timeout=300000' \
  python3 benchmarks/run_m52_filtered_ann.py --n 25000 --dim 128 --nq 50 --runs 3 --out m52.json
```
`cat in 0..100` → `cat = X` ≈1%, `cat < 10` ≈10%, `cat < 50` ≈50%. theodb: `max_scan_tuples=20000`, `ef_search=100`. pgvector: `hnsw.iterative_scan=relaxed_order`, `max_scan_tuples=20000`, `ef_search=100`. GT = seqscan+filtro exato. Raw em `m52-filtered-ann.json`.
