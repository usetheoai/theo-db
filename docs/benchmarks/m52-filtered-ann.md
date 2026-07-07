# M52 — Filtered ANN: recall sob `WHERE` seletivo (theodb iterative scan vs pgvector 0.8)

**Date:** 2026-07-07 · **Milestone:** M52 · **Metric:** cosine · **GT:** seqscan exato + filtro · **Image:** `theodb:m52` (vs pgvector 0.8.3)
**Harness:** `benchmarks/run_m52_filtered_ann.py` (reusa `theodb_bench.metrics`, espelha M50/M51) · **n=25 000, dim=128, k=10, 3 runs**
**Verdict:** **No filtro SELETIVO (1%) — onde o iterative scan é o ponto — theodb ATINGE PARIDADE com o pgvector 0.8 (recall 0.973 ≥ 0.967).** O gate do DoD (recall sob filtro seletivo ≥ paridade pgvector) é cumprido.

---

## ⚠️ Caveats (Rule 3)

Escala tratável (25k×128 gaussiano) numa box **muito contendida** (`load_pre=15.63`, por-run `[14.5, 20.5, 20.8]`). Os números de **QPS carregam ruído pesado** de contenção; o **recall é determinístico** (seed fixo, GT exato) e independente de carga — é o eixo confiável e o gate do M52. O ganho ≥2× de QPS não é um objetivo do M52 (é do M51).

## 1. Recall@10 por seletividade (theodb iterative vs pgvector 0.8 relaxed_order, mean±std, 3 runs)

| seletividade | filtro | theodb recall | pgvector recall | paridade | theodb p50 | pgvector p50 |
|---|---|---|---|---|---|---|
| **~1%** | `cat = 7` | **0.9733 ± 0.002** | 0.9673 ± 0.005 | ✅ **SIM** (≥) | 58.4 ms | 17.0 ms |
| ~10% | `cat < 10` | 0.6040 ± 0.008 | 0.6173 ± 0.008 | ✗ (−0.013) | 5.2 ms | 3.2 ms |
| ~50% | `cat < 50` | 0.5840 ± 0.016 | 0.6080 ± 0.013 | ✗ (−0.024) | 4.6 ms | 2.8 ms |

## 2. Leitura

- **O gate É o caso SELETIVO (1%), e ele PASSA.** Sob `cat = 7` (~250 de 25 000 linhas), o HNSW ingênuo (≤ ef_search=100 tuplas) quase nunca acha 10 linhas que passam o filtro nos 100 candidatos mais próximos → recall colapsaria. O **iterative scan** (re-busca com ef crescente até `max_scan_tuples`) recupera: **theodb 0.973 ≥ pgvector 0.967**. Este é o gap CRÍTICO que o DoD ataca, e theodb iguala o SOTA permissivo (pgvector 0.8 `relaxed_order`).
- **10% e 50% NÃO são filtros seletivos** — a ef=100 já sobram candidatos que passam, então o iterative scan mal dispara e o recall medido é simplesmente o **recall BASE do HNSW a ef=100** (~0.60 para AMBOS — bate com a régua M50/M51: theodb f32 ef=100 → 0.59, pgvector → 0.63). A diferença theodb −0.013/−0.024 é o **gap de navegação base conhecido** (theodb ligeiramente atrás do pgvector no HNSW puro, estabelecido no M50 § régua), **NÃO** um déficit do mecanismo de filtragem do M52. Subir `ef_search` eleva ambos (é o knob base, ortogonal ao iterative scan). Honestidade: o M52 não reduziu o recall base; ele adicionou a recuperação sob filtro seletivo.
- **QPS: theodb ~3× mais lento que pgvector no caso seletivo** (58 vs 17 ms) porque, por ADR-1 (KISS), o iterative scan do theodb **re-busca o grafo inteiro com ef dobrado** a cada esgotamento, enquanto o pgvector 0.8 **resume do `discarded` set** (não re-percorre). É o trade-off documentado: paridade de RECALL agora; a otimização resume-from-discarded fica como follow-up rastreado (backlog) SE o custo importar.

## 3. VEREDITO (DoD)

**Cumprido:** recall sob filtro seletivo (1%) **≥ paridade pgvector 0.8** (0.973 ≥ 0.967) — o iterative scan do theodb funciona e iguala o SOTA permissivo no caso que importa. `EXPLAIN` prova `Index Scan` sob `WHERE` (pg_test `filtered_scan_preserves_recall_via_iterative`). Zero regressão no path unfiltered (a suíte M45/M50 usa `LIMIT k` com ef≫k → o grow nunca dispara; pg_test `traverse_presize` + `iterative_scan_off` verdes).

**Honesto sobre o que NÃO é vitória:** (a) a 10%/50% o recall é o base do HNSW (theodb ~0.01-0.02 atrás do pgvector = gap M50, não M52); (b) o QPS do theodb no caso seletivo é ~3× o do pgvector (re-busca vs resume) — trade-off ADR-1, follow-up de otimização.

## 4. Metodologia / reprodução

```bash
PGPORT=<port> PGOPTIONS='-c statement_timeout=300000' \
  python3 benchmarks/run_m52_filtered_ann.py --n 25000 --dim 128 --nq 50 --runs 3 --out m52.json
```
`cat in 0..100` → `cat = X` ≈1%, `cat < 10` ≈10%, `cat < 50` ≈50%. theodb: `max_scan_tuples=20000`, `ef_search=100`. pgvector: `hnsw.iterative_scan=relaxed_order`, `max_scan_tuples=20000`, `ef_search=100`. GT = seqscan+filtro exato. Raw em `m52-filtered-ann.json`.
