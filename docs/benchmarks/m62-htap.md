# M62 — superfície HTAP unificada (lakehouse-materializada): benchmark de 3 eixos

**Veredito:** o HTAP do TheoDB — row-store (OLTP) + snapshot Parquet colunar (OLAP) via pg_duckdb — **funciona e
entrega o ganho analítico (~31× a 5M)** sem degradar o OLTP, ao preço de um refresh explícito (~1.2s @5M) e
freshness datada. **Achado arquitetural honesto:** o pg_duckdb **proíbe execução DuckDB dentro de funções plpgsql**
→ a superfície NÃO é um single-call transparente; é um fluxo **codegen statement-level** (as funções geram o SQL,
o cliente o executa na conexão). É a aposta lakehouse/D2 (assistida), não o columnar in-memory auto-mantido do AlloyDB.

## O achado arquitetural (medido)

`SELECT … FROM duckdb.query(…)` **dentro** de uma função plpgsql dispara `ERROR: DuckDB execution is not supported
inside functions` (não há GUC para permitir). Logo `theodb.olap()`/`htap_refresh()` que chamavam duckdb.query
internamente FALHAM. No nível de **statement** (conexão) o `COPY … TO parquet` e `read_parquet` via `duckdb.query`
FUNCIONAM. Pivô honesto: as funções viram **codegen** —

| Função | Tipo | Faz |
|---|---|---|
| `theodb.htap_refresh_sql(rel)` | codegen → text | retorna o `COPY (…) TO '<path>' (FORMAT parquet)` (o cliente executa) |
| `theodb.htap_register(rel, path)` | SQL puro → timestamptz | upsert do catálogo `theodb._htap_snapshots` (funciona em função) |
| `theodb.olap_sql(rel)` | codegen → text | retorna o `SELECT * FROM duckdb.query($$ … read_parquet('<path>') … $$)` |
| `theodb.htap_freshness(rel)` | SQL puro → interval | `now() - refreshed_at` (o lag datado) |

Nenhuma função chama duckdb.query internamente. Provado por **16 pytest GREEN** (`benchmarks/tests/test_htap.py`):
fluxo COPY→Parquet→register→olap checksum-matched vs heap fresco, freshness/staleness explícita, race-aware
OLTP-sob-OLAP, negativos tipados.

## Método

- **Harness:** `benchmarks/run_m62_htap.py` (eixo-3, `threading.Barrier` p/ overlap real) + medição inline dos eixos
  1-2. Reusa o padrão `_AGG`/`_results_match` (Rule 9). Imagem `theodb:m62` (pg_duckdb + o snapshot dir), droplet c-8.

## Resultado — 3 eixos (n=5M)

| Eixo | Métrica | Valor | Veredito |
|---|---|---|---|
| **1 — OLAP colunar** | `theodb.olap_sql` (read_parquet) vs heap `GROUP BY` | **15.9 ms vs 492.9 ms → ~31×** | DuckDB/Parquet vence forte (columnar+vetorizado no group-by) |
| **2 — custo de refresh** | `COPY row→Parquet` @5M (statement-level) | **~1.2 s** | o preço da freshness — o snapshot precisa ser materializado |
| **3 — não-interferência OLTP** | p95 INSERT baseline vs sob OLAP concorrente | **2.31 ms → 1.09 ms (não degrada)** | o snapshot Parquet é read-only → OLAP não bloqueia o OLTP |

Correctness: o resultado do `olap_sql` == o `GROUP BY` no heap fresco (checksum-matched no test suite).

## O trade-off HTAP (honesto)

- **Ganho:** analytics ~31× sobre o snapshot colunar, sem impacto no OLTP.
- **Preço:** (a) **freshness datada** — o snapshot fica atrás do heap até o próximo refresh (nós EXPOMOS o lag via
  `htap_freshness`; AlloyDB/TiFlash escondem com sync automático); (b) **custo de refresh** ~1.2s @5M; (c) **storage
  2×** (heap + Parquet em disco — mais barato que a RAM do columnar in-memory do AlloyDB, mas real).
- **Posicionamento (Regra 5):** é HTAP **lakehouse-materializado** (D2), não o in-memory auto-mantido do AlloyDB.
  Sem claim de paridade — os números acima são o que medimos.

## Caveats honestos

1. **Dados sintéticos** (5 categorias, 5M) — a direção (colunar vence, OLTP não degrada) é mecânica; absolutos movem.
2. **~31× é o group-by** (o `_AGG` canônico) — o full-scan sum do M61 deu ~9× (o group-by lê menos colunas → ganho maior). Ambos honestos, queries diferentes.
3. **Freshness manual** — o refresh é explícito (o cliente decide quando); um scheduler/CDC seria um follow-up (M-futuro).
4. **1-cliente** nos eixos 1-2; o eixo-3 é concorrente (2 threads).

## Reprodução

```
# eixo-3: python3 benchmarks/run_m62_htap.py --seed-n 1000000 --n-inserts 5000 --out m62_htap_axis3.json
# eixos 1-2: o fluxo htap_refresh_sql → COPY → htap_register → olap_sql vs heap GROUP BY (ver docs/benchmarks/m62-raw/)
```

Dados brutos: `docs/benchmarks/m62-htap.json` + `docs/benchmarks/m62-raw/*.json`.
