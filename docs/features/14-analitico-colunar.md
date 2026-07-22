# Consultas analíticas sobre armazenamento colunar

> **✅ Entregue (M99 + M100 + M114 + M115):** armazenamento colunar own-code via o **Table Access Method**
> `theodb_columnar` (`CREATE TABLE ... USING theodb_columnar`) — registrado em
> `theodb_rs/src/am/columnar.rs:218` (`CREATE ACCESS METHOD theodb_columnar TYPE TABLE HANDLER
> theodb_columnar_tam_handler`), formato column-major com dicionário de min/max por chunk-group + MVCC
> delegado a um catálogo heap. Agregados/`GROUP BY`/`WHERE` vetorizados via `CustomScan` sobre DataFusion
> (own-code glue, Regra 9) em `theodb_rs/src/am/columnar_agg.rs` + `theodb_rs/src/am/df_executor.rs`.
> Benchmarks medidos: [`docs/benchmarks/m99-columnar-tam.md`](../benchmarks/m99-columnar-tam.md) (substrato
> de storage), [`columnar-groupby-verdict.md`](../benchmarks/columnar-groupby-verdict.md) (GROUP BY
> 4,53–9,75×), [`columnar-minmax-zonemap-verdict.md`](../benchmarks/columnar-minmax-zonemap-verdict.md)
> (`min`/`max` fast-path ~1300–1400×), [`columnar-zonemap-verdict.md`](../benchmarks/columnar-zonemap-verdict.md)
> (zone-map skip 7,29×), [`m114-columnar-aggregate-verdict.md`](../benchmarks/m114-columnar-aggregate-verdict.md),
> [`m115-columnar-composability-verdict.md`](../benchmarks/m115-columnar-composability-verdict.md) e
> [`m128-clickbench-columnar.md`](../benchmarks/m128-clickbench-columnar.md) (43 queries ClickBench
> byte-idênticas vs heap).
>
> **Caveats honestos:** (1) o pushdown vetorizado de agregados é **opt-in** — a GUC
> `theodb.enable_columnar_agg` tem **default OFF** (`theodb_rs/src/am/columnar_agg.rs:22-23`); sem ela, a
> tabela colunar funciona como storage e o agregado roda pelo plano nativo do PostgreSQL. (2) Nem toda
> forma de agregado faz pushdown — o que não é admitido cai (fail-safe) para o plano nativo (ver seção 9).
> (3) É armazenamento colunar **em disco** own-code (não in-memory automático); a paridade *literal* com o
> AlloyDB columnar está fora de escopo (CLAUDE.md, D2). (4) A superfície DML é **append-only / INSERT-only**:
> `UPDATE`, `DELETE`, tuple-lock, parallel scan, sample scan, TID-range scan e `CREATE INDEX` falham com
> **erro tipado** em tabelas `theodb_columnar` (stubs `error!` — `theodb_rs/src/am/columnar.rs:15`, `:237`);
> bitmap scan não erra — os callbacks ficam `NULL` de propósito e o planner **desvia** da forma bitmap
> (`columnar.rs:304-310`, M135/ADR-2). Use tabelas heap para dados mutáveis; a colunar é para carga
> analítica append-only.

Esta página cobre como criar tabelas colunares no TheoDB com o Table Access Method own-code `theodb_columnar`,
ligar o pushdown vetorizado de agregados, e quais formas de `count`/`sum`/`avg`/`min`/`max`, `GROUP BY` e
`WHERE` são aceleradas (com fast-path de zone-map) versus quais declinam para o plano nativo.

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` (own-code), que registra o access method de tabela `theodb_columnar`
(`theodb_rs/src/am/columnar.rs:218`).

---

# 2. Criar uma tabela colunar (`USING theodb_columnar`)

```sql
CREATE TABLE eventos (
    id       int,
    ts       timestamptz,
    regiao   int,
    valor    float8
) USING theodb_columnar;
```

Cria uma tabela com armazenamento column-major append-only. Cada coluna é gravada em chunks comprimidos
(zstd) com um diretório de min/max por chunk-group (a base do zone-map). Verificado em
`theodb_rs/src/am/df_executor.rs:703` e `:736` (tabelas de teste `USING theodb_columnar`).

---

# 3. Inserir dados

```sql
INSERT INTO eventos
SELECT g, now() - (g || ' seconds')::interval, g % 100, g * 1.5
FROM generate_series(1, 1000000) AS g;
```

O caminho de inserção é append-only: as linhas viram stripes colunares; a visibilidade MVCC é delegada a um
catálogo heap (`theodb_rs/src/am/columnar.rs` — `columnar_tuple_insert` / `columnar_multi_insert`).

---

# 4. Ler a tabela (seqscan colunar)

```sql
SELECT id, valor
FROM eventos
WHERE regiao = 7
LIMIT 20;
```

Um `SELECT` normal (seqscan plano) decodifica **todas as colunas** de cada stripe — o TAM não recebe a lista
de projeção do planner (`theodb_rs/src/am/columnar.rs:1015-1021`), então o seqscan puro é medido
**paridade-ou-mais-lento que heap, por design** (~16–26× no agregado full-scan —
[`m99-columnar-tam.md`](../benchmarks/m99-columnar-tam.md)). O ganho de projeção/vetorização existe **apenas**
no caminho `CustomScan` do M100 (GUC da seção 5 ligada + forma admitida — seção 9); sem ele, a vitória do
storage colunar é o tamanho em disco (compressão), não a latência de leitura.

---

# 5. Ligar o pushdown vetorizado de agregados (opt-in — default OFF)

```sql
SET theodb.enable_columnar_agg = on;
```

Liga o `CustomScan` vetorizado para agregados sobre tabelas `theodb_columnar`. A GUC tem **default OFF**
(`theodb_rs/src/am/columnar_agg.rs:22-23` — `ENABLE_COLUMNAR_AGG = GucSetting::new(false)`; registro em
`:90`). Com ela desligada, os agregados abaixo continuam corretos, mas executam pelo plano nativo do
PostgreSQL, sem a aceleração colunar.

---

# 6. `count(*)` acelerado

```sql
SELECT count(*) FROM eventos;
```

`count(*)` é a forma mais simples admitida (kind 0 em `theodb_rs/src/am/columnar_agg.rs:328`). Responde do
metadado colunar sem materializar linhas.

---

# 7. `sum` — tipos aceitos e tipo de saída

```sql
SELECT
    sum(valor)   AS soma_float8,   -- sum(float8) -> float8
    sum(regiao)  AS soma_int       -- sum(int2/int4) -> int8
FROM eventos;
```

O pushdown admite `sum(float8)→float8`, `sum(int2/int4)→int8` e `sum(int8)→numeric` (kinds 1/2/4 em
`theodb_rs/src/am/columnar_agg.rs:352-360`). **Declina** para o plano nativo em `sum(float4)` e `sum(numeric)`
— mantendo o resultado byte-idêntico ao PostgreSQL. Medido: `sum(int4)` **11,74×**
([`m114-columnar-aggregate-verdict.md`](../benchmarks/m114-columnar-aggregate-verdict.md)).

---

# 8. `avg` — tipos aceitos e tipo de saída

```sql
SELECT
    avg(valor)   AS media_float8,  -- avg(float8) -> float8
    avg(regiao)  AS media_int      -- avg(int2/4/8) -> numeric
FROM eventos;
```

O pushdown admite `avg(float8)→float8` (kind 3) e `avg(int2/4/8)→numeric` (kind 5, divisão exata via
`AnyNumeric` = `numeric_div` do PostgreSQL — `theodb_rs/src/am/columnar_agg.rs:362-371`). **Declina**
`avg(float4)` e `avg(numeric)`. Medido: `avg(float8)` **9,52×**
([`m114-columnar-aggregate-verdict.md`](../benchmarks/m114-columnar-aggregate-verdict.md)).

---

# 9. Formas que declinam para o plano nativo (fail-safe)

```sql
-- ambas rodam corretas, mas SEM aceleração colunar (plano nativo):
SELECT sum(valor::float4) FROM eventos;     -- sum(float4): declina
SELECT max(nome_texto)    FROM eventos;     -- min/max em tipo nao-ordenado: declina
```

O guard de admissão nunca dá erro: qualquer forma não suportada (`sum(float4)`, `avg(float4)`,
`min`/`max` em tipo não-ordenado como `text`/`numeric`, ou uma expressão `min(col+1)` em vez de coluna
pura) faz o `CustomScan` recuar e o PostgreSQL executa o plano nativo
(`theodb_rs/src/am/columnar_agg.rs:333-378`). Correção sempre vence performance.

---

# 10. `GROUP BY` acelerado

```sql
SELECT regiao, count(*), sum(valor)
FROM eventos
GROUP BY regiao;
```

O `GROUP BY` faz pushdown para o hash-aggregate vetorizado do DataFusion
(`run_columnar_grouped_aggs` em `theodb_rs/src/am/df_executor.rs`; admissão em
`theodb_rs/src/am/columnar_agg.rs`). Medido byte-idêntico com **6,00×** (chave int), **4,53×** (multi-chave)
e **9,75×** (chave temporal) em [`columnar-groupby-verdict.md`](../benchmarks/columnar-groupby-verdict.md).

> Chaves de `GROUP BY` de tipo `numeric` declinam para o plano nativo
> (`theodb_rs/src/am/columnar_agg.rs:306`).

---

# 11. `GROUP BY` com `WHERE` (zone-map skip-pruning)

```sql
SELECT regiao, avg(valor)
FROM eventos
WHERE ts >= '2026-07-01' AND ts < '2026-07-08'
GROUP BY regiao;
```

Um `WHERE` seletivo sobre coluna clusterizada aciona o **zone-map skip-pruning**: o scan consulta o min/max
por chunk-group e pula os que não podem casar, decodificando só a fração relevante. Medido **7,29×** de
redução de latência (89% dos chunk-groups pulados) em
[`columnar-zonemap-verdict.md`](../benchmarks/columnar-zonemap-verdict.md). O ganho depende de
seletividade × clusterização (numa coluna não-ordenada o skip é pequeno).

---

# 12. Kill-switch do zone-map (A/B — default ON)

```sql
SET theodb.columnar_zonemap_skip = off;   -- desliga o skip (baseline A/B); default = on
```

Controla o skip-pruning por min/max num agregado filtrado. Default **ON**
(`theodb_rs/src/am/guc.rs:130` — `COLUMNAR_ZONEMAP_SKIP = GucSetting::new(true)`; registro em `:344`). Sem
efeito em tabelas não-colunares.

---

# 13. `min` / `max` respondidos só do diretório (fast-path)

```sql
SELECT min(id), max(id), min(ts), max(ts)
FROM eventos;
```

`min`/`max` de coluna de tipo ordenado (`int2/4/8`, `float4/8`, `bool`, `timestamp`/`date`) são respondidos
**só do diretório de zone-map**, sem decodificar nenhum chunk (kinds 6/7 em
`theodb_rs/src/am/columnar_agg.rs:374-378`). Medido **~1300–1400×** para int/temporal e ~13–16× para float
(gate de `NaN` força scan) em
[`columnar-minmax-zonemap-verdict.md`](../benchmarks/columnar-minmax-zonemap-verdict.md).

---

# 14. Composição do resultado do agregado colunar (subquery / join / ORDER BY)

```sql
SELECT regiao, total
FROM (
    SELECT regiao, sum(valor) AS total
    FROM eventos
    GROUP BY regiao
) t
WHERE total > 1000
ORDER BY total DESC;
```

A saída do `CustomScan` colunar é consumível por subquery, join e `ORDER BY` — a rearquitetura Agg-swap
(M115) troca um `Agg` normal pelo `CustomScan` após o `set_plan_refs`, preservando a composabilidade.
Verdict medido em [`m115-columnar-composability-verdict.md`](../benchmarks/m115-columnar-composability-verdict.md).

---

# 15. Materializar cache Arrow em memória a partir de uma tabela heap (pragma opcional)

> ⚠️ **Caminho opcional / experimental (M101).** É um cache Arrow **in-memory** por-backend construído de uma
> tabela **heap** — separado do storage colunar em disco do `theodb_columnar`.

```sql
SELECT theodb_columnarize('minha_tabela_heap'::regclass, ARRAY['valor']);
```

`theodb_columnarize(table, cols[])` monta um `RecordBatch` Arrow em memória a partir das colunas projetadas de
uma tabela heap (`theodb_rs/src/am/arrow_cache.rs:206`). O tamanho do cache é limitado pela GUC
`theodb.arrow_cache_max_entries` (default 16 — `theodb_rs/src/am/guc.rs:217`). Companheiras:
`theodb_cache_refresh(table)` (`arrow_cache.rs:247`) e `theodb_cache_agg(table, num_col)` (`arrow_cache.rs:258`).

---

# 16. Verificar o EXPLAIN (o CustomScan foi engajado?)

```sql
SET theodb.enable_columnar_agg = on;
EXPLAIN (VERBOSE)
SELECT regiao, count(*) FROM eventos GROUP BY regiao;
```

Com a GUC ligada e uma forma admitida, o plano mostra o `CustomScan` colunar no lugar do agregado nativo. Se
o EXPLAIN ainda mostrar o `Aggregate` nativo, a forma declinou (seção 9) — o resultado continua correto, sem
a aceleração. Sweep de EXPLAIN medido em
[`m131-columnar-agg-accelerated.md`](../benchmarks/m131-columnar-agg-accelerated.md).

---

# 17. Fluxo completo recomendado

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE TABLE eventos (
    id     int,
    ts     timestamptz,
    regiao int,
    valor  float8
) USING theodb_columnar;

INSERT INTO eventos
SELECT g, now() - (g || ' seconds')::interval, g % 100, g * 1.5
FROM generate_series(1, 1000000) AS g;

SET theodb.enable_columnar_agg = on;   -- opt-in (default OFF)

SELECT regiao, count(*), avg(valor)
FROM eventos
WHERE ts >= '2026-07-01'
GROUP BY regiao
ORDER BY 1;
```

Fluxo completo:

1. instala a extensão `theodb` (registra o AM `theodb_columnar`);
2. cria a tabela colunar `USING theodb_columnar`;
3. carrega os dados (append-only, column-major);
4. liga o pushdown vetorizado (`theodb.enable_columnar_agg`, default OFF);
5. executa `GROUP BY` + `WHERE` com skip-pruning por zone-map — byte-idêntico ao plano nativo, mais rápido nas formas admitidas.
