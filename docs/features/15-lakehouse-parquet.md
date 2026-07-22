# Lakehouse Parquet — ler, escrever e agregar Parquet externo (own-code)

> **✅ Entregue (M62 + M130 + M143).** Ler/escrever/agregar arquivos Parquet externos é **100% own-code** em
> Rust (DataFusion + Arrow, Apache-2.0) — **sem DuckDB** (o `pg_duckdb`, último componente C++/httpfs do
> projeto, foi **removido por completo** no M143). Primitivas: `public.read_parquet(path)` → `SETOF jsonb`
> (`theodb_rs/src/parquet.rs:121`), `public.write_parquet(rel, path)` → `bigint`
> (`theodb_rs/src/parquet.rs:168`), `public.olap(path)` → agregado tipado (`theodb_rs/src/parquet.rs:75`).
> Superfície de usuário HTAP em `sql/85-theodb-htap.sql:39`. Benchmarks:
> [`docs/benchmarks/m143-pgduckdb-removal.md`](../benchmarks/m143-pgduckdb-removal.md),
> [`docs/benchmarks/m130-htap.md`](../benchmarks/m130-htap.md),
> [`docs/benchmarks/parquet-reader-owncode-spike.md`](../benchmarks/parquet-reader-owncode-spike.md).

> **Status:** ✅ **Entregue (own-code, imagem default).** Trade-off honesto (`.claude/rules/public-copy.md § 3`):
> o lakehouse do TheoDB é **disk/Parquet own-code** (DataFusion/Arrow), **não** in-memory-auto como o AlloyDB
> (D2 — `theo-db/CLAUDE.md`). Paridade byte-a-byte do agregado M62 vs o antigo pg_duckdb foi **medida**
> ([`parquet-reader-owncode-spike.md`](../benchmarks/parquet-reader-owncode-spike.md)); o pilar HTAP misto
> (CH-benCHmark) foi medido com 0% de erro em [`m130-htap.md`](../benchmarks/m130-htap.md).

Esta página cobre as primitivas de I/O Parquet (`public.*`), a superfície HTAP de usuário (`theodb.*`), a
**restrição de segurança superuser-only** (I/O de arquivo server-side), e os tipos suportados na escrita v1.

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` e sua base `theodb_rs` via `CASCADE` — o que provê as funções `public.read_parquet`,
`public.write_parquet`, `public.olap` (own-code, DataFusion/Arrow) e a superfície HTAP `theodb.*`.

---

# 2. Segurança — as funções de I/O de arquivo são superuser-only

```sql
-- theodb_rs/src/parquet.rs:321  (extension_sql!)
REVOKE ALL ON FUNCTION public.write_parquet(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.read_parquet(text)        FROM PUBLIC;
REVOKE ALL ON FUNCTION public.olap(text)                FROM PUBLIC;
```

Ler/escrever arquivo no lado do servidor é **privilégio de superuser** (como o `COPY … TO file`, que exige
superuser / `pg_write_server_files`). O default do pgrx seria `GRANT EXECUTE TO PUBLIC`; por isso há um `REVOKE`
explícito no `extension_sql!` (`theodb_rs/src/parquet.rs:321`). A superfície de usuário `theodb.*` também é
revogada de PUBLIC (`sql/85-theodb-htap.sql:144`), então não é contornável chamando as primitivas `public.*`
direto. Um role de baixo privilégio é bloqueado (gate `REVOKE_OK` em
[`m143-pgduckdb-removal.md`](../benchmarks/m143-pgduckdb-removal.md)).

---

# 3. Ler um Parquet — `public.read_parquet(path)`

```sql
SELECT read_parquet
FROM public.read_parquet('/var/lib/postgresql/htap/sales.parquet')
LIMIT 5;
```

`public.read_parquet(path text) RETURNS SETOF jsonb` (`theodb_rs/src/parquet.rs:121`): cada linha do Parquet vira
um `jsonb` (via arrow-json), cobrindo **todos os tipos** (escalares e nested) sem precisar declarar um
`SETOF record` dinâmico. A coluna de saída chama-se `read_parquet`.

---

# 4. Extrair campos do `jsonb` lido

```sql
SELECT
    read_parquet ->> 'category'        AS category,
    (read_parquet ->> 'amount')::numeric AS amount
FROM public.read_parquet('/var/lib/postgresql/htap/sales.parquet');
```

Como cada linha é um `jsonb`, os operadores nativos `->`/`->>` extraem campos. O gate `READ_MULTI` mediu
`{"n":1,"flag":true,"amount":10.0,"category":"a"}` — int/float/text/bool num só jsonb
([`m143-pgduckdb-removal.md`](../benchmarks/m143-pgduckdb-removal.md)).

---

# 5. Escrever uma tabela em Parquet — `public.write_parquet(rel, path)`

```sql
SELECT public.write_parquet('sales', '/var/lib/postgresql/htap/sales.parquet');
```

`public.write_parquet(rel text, path text) RETURNS bigint` (`theodb_rs/src/parquet.rs:168`): materializa a tabela
`rel` num arquivo Parquet único (SPI → arrays Arrow → `ArrowWriter`), retornando o número de linhas escritas. `rel`
é resolvido/validado via `$1::regclass` e o `FROM` usa o nome **canônico** (injection-safe — não interpolação
crua, `theodb_rs/src/parquet.rs:185`). A escrita é atômica (temp único por-backend + `rename`).

---

# 6. Tipos suportados na escrita (v1)

```sql
-- theodb_rs/src/parquet.rs:227 — OIDs aceitos:
--   int2 (21), int4 (23), int8 (20), float4 (700), float8 (701), bool (16), text (25/1042/1043)
```

A escrita v1 cobre os **escalares** acima. Um tipo não-suportado gera **erro tipado (fail-closed)**, com o
backend permanecendo vivo — não um panic (`theodb_rs/src/parquet.rs:235`).

---

# 7. Tipo não-suportado na escrita → erro tipado

```sql
-- uma coluna timestamptz (OID 1184) na tabela alvo:
SELECT public.write_parquet('events', '/var/lib/postgresql/htap/events.parquet');
-- ERROR:  theodb.write_parquet: coluna 'created_at': tipo OID 1184 não suportado na
--         escrita Parquet own-code (v1: int2/4/8, float4/8, bool, text). ...
```

O tipo é legível via `read_parquet` (que produz jsonb de qualquer schema), mas a **escrita** ampla é follow-on. O
comportamento fail-closed foi medido no gate `WRITE_FAILCLOSED` de
[`m143-pgduckdb-removal.md`](../benchmarks/m143-pgduckdb-removal.md).

---

# 8. Agregado canônico direto do Parquet — `public.olap(path)`

```sql
SELECT category, c, a
FROM public.olap('/var/lib/postgresql/htap/sales.parquet');
```

`public.olap(path text) RETURNS TABLE(category text, c bigint, a float8)`
(`theodb_rs/src/parquet.rs:75`) lê+agrega o Parquet own-code (DataFusion). É o agregado **fixo** do M62:
`GROUP BY category` com `count(*)` (coluna `c`) e `round(avg(amount), 4)` (coluna `a`).

> **Honestidade:** `public.olap` **não** é um SQL genérico sobre Parquet — é o agregado de demonstração M62,
> que espera colunas chamadas `category` e `amount` no arquivo. Para consulta arbitrária, use `read_parquet`
> (§3–4). A paridade byte-a-byte deste agregado vs o antigo pg_duckdb foi medida
> ([`parquet-reader-owncode-spike.md`](../benchmarks/parquet-reader-owncode-spike.md)).

---

# 9. Superfície de usuário HTAP — materializar um snapshot

```sql
SELECT theodb.htap_refresh('sales');
```

`theodb.htap_refresh(p_rel regclass) RETURNS timestamptz` (`sql/85-theodb-htap.sql:39`) materializa a relação num
snapshot Parquet own-code (chama `public.write_parquet` internamente) **e** registra o snapshot no catálogo, numa
chamada só. Retorna o `refreshed_at`. É a superfície recomendada — não é preciso mexer nos paths à mão.

---

# 10. Consultar o snapshot registrado — `theodb.olap(rel)`

```sql
SELECT * FROM theodb.olap('sales');
```

`theodb.olap(p_rel regclass) RETURNS TABLE(category text, c bigint, a float8)`
(`sql/85-theodb-htap.sql:93`) resolve o path do snapshot no catálogo e devolve o agregado canônico lido own-code
(`public.olap`). Se não há snapshot, levanta `no_data_found` (P0002, fail-closed) pedindo um `htap_refresh` antes —
nunca um `NULL` silencioso.

---

# 11. Registrar um snapshot materializado fora do fluxo — `theodb.htap_register`

```sql
SELECT theodb.htap_register('sales', '/var/lib/postgresql/htap/sales.parquet');
```

`theodb.htap_register(p_rel regclass, p_parquet_path text) RETURNS timestamptz`
(`sql/85-theodb-htap.sql:62`) faz o upsert do catálogo `(rel, parquet_path, now())`. Útil para quem materializa
por fora do `htap_refresh`. Um path vazio levanta `invalid_parameter_value` (22023, erro tipado).

---

# 12. Medir a defasagem do snapshot — `theodb.htap_freshness`

```sql
SELECT theodb.htap_freshness('sales');
```

`theodb.htap_freshness(p_rel regclass) RETURNS interval` (`sql/85-theodb-htap.sql:120`) devolve o lag
`now() - refreshed_at` do snapshot. A freshness é um contrato datado: cresce entre refreshes e zera a cada
`htap_refresh`/`htap_register`. Sem snapshot → `no_data_found` (P0002).

---

# 13. Fluxo HTAP completo (row → snapshot → OLAP)

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

-- (como superuser) materializa o snapshot own-code + registra
SELECT theodb.htap_refresh('sales');

-- consulta analítica sobre o snapshot Parquet
SELECT * FROM theodb.olap('sales');

-- quão fresco está o snapshot?
SELECT theodb.htap_freshness('sales');
```

Fluxo completo:

1. instala a extensão `theodb`;
2. `theodb.htap_refresh` escreve o Parquet own-code (DataFusion/Arrow, sem DuckDB) e registra o snapshot;
3. `theodb.olap` lê+agrega o snapshot registrado;
4. `theodb.htap_freshness` reporta a defasagem.

---

# 14. Consulta arbitrária sobre o Parquet materializado

```sql
SELECT
    read_parquet ->> 'category'          AS category,
    sum((read_parquet ->> 'amount')::numeric) AS total
FROM public.read_parquet('/var/lib/postgresql/htap/sales.parquet')
GROUP BY read_parquet ->> 'category'
ORDER BY total DESC;
```

Quando o agregado fixo de `public.olap` não basta, `read_parquet` devolve cada linha como `jsonb` e você aplica
qualquer SQL nativo (`GROUP BY`, `sum`, `ORDER BY`) sobre os campos extraídos. Isso roda **inteiramente
own-code** — o DataFusion lê o arquivo dentro da função, sem DuckDB (M143).

---

# 15. Notas de arquitetura (own-code, sem DuckDB)

```sql
-- gate NO_PGDUCKDB — m143-pgduckdb-removal.md
SELECT * FROM pg_extension WHERE extname = 'pg_duckdb';  -- (vazio: removido por completo)
```

Desde o M143, o lakehouse é 100% Rust (DataFusion/Arrow, Apache-2.0), no build **default** — a imagem opcional
`theodb-htap` do M142 foi aposentada. O DataFusion roda **dentro da função** (não há mais o design "codegen" do
M62 que retornava texto para o cliente rodar), limitado por `work_mem` (`GreedyMemoryPool` — um Parquet maior que
`work_mem` vira erro tipado, não OOM — `theodb_rs/src/parquet.rs:50`), sob `HeldInterrupts` para que um longjmp do
PG não salte por cima do runtime tokio (`theodb_rs/src/parquet.rs:30`). ADR: `docs/adr/0057`.
