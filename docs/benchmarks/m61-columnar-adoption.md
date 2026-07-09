# M61 — adoção columnar/HTAP (pg_duckdb embarcado): benchmark de adoção

**Veredito:** pg_duckdb (MIT, GA v1.1.0) **embarcado com sucesso** na imagem TheoDB PG17. O ganho analítico
columnar **materializa sobre dados em formato colunar (Parquet): ~9× a 5M** — NÃO sobre o heap row-store (onde é
honest-negative). Medição-first honesta (Regra 5): o número é da superfície pg_duckdb medida aqui, **não herdado**
do ~14× do pg_mooncake (M30, mecanismo diferente — columnstore nativo vs read_parquet).

## Método

- **Harness:** `benchmarks/run_m61_columnar_adoption.py` (reusa `theodb_bench.columnar._AGG`/`_results_match` — Rule 9).
  Duas superfícies medidas na MESMA box, ≥3 runs mean±std (warm-up descartado), correctness-matched:
  1. **HEAP `force_execution`:** a mesma tabela heap agregada pelo executor DuckDB (`SET duckdb.force_execution=true`)
     vs o row-executor do Postgres. O caminho "analytics sobre dados transacionais sem ETL".
  2. **PARQUET colunar:** o heap exportado para Parquet (`COPY … TO … (FORMAT parquet)`), depois agregado pelo
     DuckDB (`read_parquet` via `duckdb.query`) vs o heap pelo Postgres. O caminho data-lake/lakehouse.
- **Ambiente:** droplet DigitalOcean c-8, imagem `theodb:m61` (pg_duckdb v1.1.0 + pgvector + pgvectorscale + theodb).
- **Correctness:** checksum full-scan (`sum(amount*2.0 + id)`) comparado numericamente (DuckDB retorna double,
  Postgres numeric — comparação por eps relativo, não string).

## Resultado 1 — HEAP `force_execution` (honest-negative)

Agregação `GROUP BY category` sobre a tabela heap; DuckDB (force_execution) vs Postgres row-executor:

| n | Postgres row (ms) | DuckDB heap (ms) | DuckDB/Postgres | match |
|---|---|---|---|---|
| 100k | 23.6 ± 5.2 | 26.4 ± 1.9 | **0.89×** | ✓ |
| 1M | 108.4 ± 15.0 | 164.2 ± 5.3 | **0.66×** | ✓ |
| 5M | 394.4 ± 12.2 | 627.8 ± 111.2 | **0.63×** | ✓ |

**DuckDB PERDE sobre o heap em todas as escalas.** Razão medida: o `force_execution` escaneia o heap **row-format**
via o access-method do Postgres e entrega ao DuckDB — sem a vantagem vetorizada (que exige dados **já colunares**);
a conversão row→vetor adiciona overhead. Resultados corretos (match=True), plano usa DuckDB (confirmado por EXPLAIN).

## Resultado 2 — PARQUET colunar (o ganho real)

O heap exportado para Parquet (colunar), agregado pelo DuckDB vs o heap pelo Postgres:

| n | DuckDB/Parquet (ms) | Postgres/heap (ms) | **Parquet/heap** | checksum |
|---|---|---|---|---|
| 100k | 5.9 | 9.2 | **1.56×** | ✓ |
| 1M | 9.3 | 73.7 | **7.91×** | ✓ |
| 5M | 24.6 | 216.1 | **8.78×** | ✓ |

**DuckDB sobre Parquet colunar VENCE e escala** (1.56× → 8.78×), com checksum correto. O ganho cresce com o tamanho
(a vantagem colunar+vetorizada do DuckDB escala melhor que o scan row do Postgres) — na faixa do ~14× do mooncake
(M30), com o caveat de que aqui os dados precisam estar em Parquet (o mooncake mantinha um columnstore sincronizado).

## O que isso significa (honesto)

- **pg_duckdb embarcado entrega valor como engine analítica para dados COLUNARES** (Parquet/Iceberg/CSV externos) —
  uma capacidade **data-lake/lakehouse** (~9× a 5M). É a aposta D2 (lakehouse), declarada, não cópia do AlloyDB.
- **NÃO acelera transparentemente analytics sobre o heap Postgres** — isso exigiria os dados em colunar. Sem
  MotherDuck, pg_duckdb não tem columnstore nativo persistente (medido: "Only TEMP tables… if MotherDuck not enabled").
- **Posicionamento vs AlloyDB (Regra 5):** AlloyDB tem columnar in-memory auto-mantido (até 100× reportado, sem
  benchmark reproduzível nosso); nós entregamos analytics colunar sobre arquivos + abertura/custo/portabilidade.
  Sem claim de paridade analítica — o número acima é o que medimos, sobre Parquet.

## Caveats honestos

1. **Dados sintéticos** (5 categorias, 5M linhas) — a direção (colunar vence, heap não) é mecânica; absolutos
   movem com o dataset. Follow-up: dataset analítico realista.
2. **Parquet exige export** — o ~9× pressupõe os dados em Parquet; a materialização row→Parquet tem custo (não
   medido aqui — é o trade-off do caminho lakehouse). O columnstore-sincronizado (mooncake/M30) evita isso mas é
   outra peça.
3. **1-cliente** (latência, não throughput multi-cliente).

## Reprodução

```
PGHOST=127.0.0.1 python3 benchmarks/run_m61_columnar_adoption.py --scales 100000,1000000,5000000 --runs 3 --out docs/benchmarks/m61-columnar-adoption.json
```

Dados brutos: `docs/benchmarks/m61-columnar-adoption.json`.
