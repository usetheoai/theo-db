# M48 — VACUUM fold maintenance benchmark

Caracterização (não comparação competitiva) do fold crash-safe do índice `theodb_hnsw` (issue #47) numa única dev box. Números com caveat de carga; mean±std de 3 runs. N=50000, dim=8, seed=42.

**Box load1 no pré-flight:** 0.41 (nproc=12; load-guard aborta se load1 > nproc/2 — lição M46).

## (a) Degradação por pending + recuperação pelo fold

O custo do pending é uma varredura LINEAR (O(pending)) somada à travessia do grafo. Ela aparece no **scan p50** e no metric **`pending_pages`** — NÃO no `pages_read` do grafo (que é ~constante ~355, pois a travessia do grafo independe do pending). O fold só dispara quando `pending_pages > threshold` (default 16); então elimina a região pending (`pending_pages → 0`) e o p50 cai.

| pending alvo (páginas) | pending antes | pending depois | p50 antes (ms) | p50 depois (ms) | foldou? |
|---|---|---|---|---|---|
| 0 | 0±0.0 | 0±0.0 | 0.1646±0.0389 | 0.202±0.0601 | não (≤ threshold) |
| 8 | 10±0.0 | 10±0.0 | 0.367±0.0742 | 0.3431±0.0721 | não (≤ threshold) |
| 16 | 16±0.0 | 16±0.0 | 0.4634±0.0786 | 0.523±0.0743 | não (≤ threshold) |
| 64 | 64±0.0 | 0±0.0 | 1.1954±0.0752 | 0.1605±0.047 | sim |

## (b) WAL volume do fold (insumo M55)

Bytes de WAL emitidos por um VACUUM (delta de `pg_current_wal_lsn()`). O fold escreve a nova geração em páginas frescas + a página-meta full-image. Abaixo do threshold (alvos 0/8/16) o fold NÃO dispara → o VACUUM insert-only é barato (≈0 de WAL de índice). Acima (64) o fold reescreve o índice inteiro — é esse custo de WAL do shadow-rewrite que o M55 (fold incremental vs in-place) vai buscar reduzir.

| pending alvo (páginas) | WAL bytes / VACUUM (mean±std) |
|---|---|
| 0 | 0±0.0 (n=3) |
| 8 | 0±0.0 (n=3) |
| 16 | 0±0.0 (n=3) |
| 64 | 12284696±1601.1 (n=3) |

## (c) Custo honesto do planner (T5.1)

- N=100 usa índice? **False** (esperado: False — seqscan+sort vence).
- N=50000 usa índice? **True** (esperado: True — o pushdown sobrevive ao custo honesto).

## Metodologia (reprodução)

```bash
# Container com o scan-profiler ligado (lê pages_read/pending_pages do log):
docker run -d --name theodb-bench -e POSTGRES_PASSWORD=postgres -e THEODB_SCAN_PROFILE=1 \
  -p 55452:5432 theodb:m48-t51   # imagem = código do M48 (T1.1..T6.1)
PGHOST=localhost PGPORT=55452 PGUSER=postgres PGPASSWORD=postgres \
  THEODB_BENCH_CONTAINER=theodb-bench \
  python3 benchmarks/run_m48_maintenance.py --n 50000 --runs 3
```

Parâmetros: N=50000, dim=8, seed=42, threshold de fold=16 páginas (default `theodb.vacuum_pending_threshold`). Cada célula p50 = mediana de 200 scans `ORDER BY <-> LIMIT 5`; WAL = delta de `pg_current_wal_lsn()` em torno de um `VACUUM (INDEX_CLEANUP ON)`. Load-guard aborta se `load1 > nproc/2`.

## Caveats honestos

- **Escopo — caracterização, não competição:** números de uma dev box; sem claim comparativo vs outro produto.
- **dim=8 é escolha de custo-de-teste, NÃO representativa de embeddings reais** (que são 384–1536d). O custo do fold é sobre páginas/WAL da região pending (independe da dimensão), então dim baixa é válida para caracterizar MANUTENÇÃO — mas NÃO extrapole as latências absolutas de scan (p50) para um workload real de embeddings.
- Variância reportada (std); o efeito do fold (pending→0, p50 menor) deve exceder a variância entre runs para ser um sinal, não ruído. No alvo 64: p50 cai ~7× (1.2→0.16 ms), muito acima do std (~0.08) — sinal, não ruído.
- `pages_read`/`pending_pages` vêm do log com `THEODB_SCAN_PROFILE=1` (pilar-c do wiring).
