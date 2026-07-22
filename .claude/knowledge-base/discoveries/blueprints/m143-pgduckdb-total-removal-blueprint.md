# Discovery blueprint — M143 remoção total do pg_duckdb (lakehouse Parquet own-code)

> Slug: `m143-pgduckdb-total-removal` · Date: 2026-07-22 · Milestone: M143
> Prior art medida: o **spike Fase 4** (`docs/benchmarks/parquet-reader-owncode-spike.md`, GO — read own-code =
> paridade byte-a-byte a +9MB vs 118MB). Este blueprint mapeia o *resto* (write, rewrite, drop, tipos).

## Problema

Remover o `pg_duckdb` inteiro, substituindo a superfície lakehouse por own-code (DataFusion+Arrow, já no
binário; Apache-2.0), dobrando a capacidade no build default e aposentando a imagem `theodb-htap`.

## Mapa da superfície pg_duckdb (evidência — grep exaustivo)

**TODA** a dependência de pg_duckdb no repo está em `sql/85-theodb-htap.sql` — 2 funções codegen:

| Função | O que produz hoje (texto p/ o cliente rodar) | Substituto own-code |
|---|---|---|
| `theodb.htap_refresh_sql(rel)` | `COPY (SELECT * FROM tbl) TO '<path>' (FORMAT parquet)` — o **writer** Parquet do DuckDB | `theodb.htap_refresh(rel)` — lê a tabela (SPI) → Arrow → `DataFrame::write_parquet` **dentro da função** |
| `theodb.olap_sql(rel)` | `SELECT * FROM duckdb.query($$ …read_parquet('<path>') GROUP BY… $$)` — **read+aggregate** | `theodb.olap(rel)` — lê o snapshot Parquet → agrega → retorna linhas **dentro da função** |

`theodb.htap_register`/`_htap_path`/`htap_freshness` são SQL puro (catálogo), **sem** DuckDB — permanecem.

## Insight de Staff engineer — o codegen COLAPSA

O `sql/85:6-16` documenta que o design codegen (funções RETORNAM texto que o cliente roda) existe **só porque
"pg_duckdb PROHIBITS DuckDB execution inside a function"**. Com own-code, **DataFusion roda dentro da função** (o
spike provou: `read_parquet_agg_spike` é um `#[pg_extern]` que lê+agrega in-function). Logo o M143 **elimina a
dança codegen**: `htap_refresh`/`olap` viram funções diretas (sem round-trip cliente, sem `_sql` no nome). Isso é
*simplificação*, não workaround — o motivo do workaround (a restrição do DuckDB) desaparece.

## Peças a construir (e a reusar — Regra 9)

| Peça | Estado | Fonte a reusar |
|---|---|---|
| Ler Parquet + agregar own-code | ✅ **provado** (spike) | `theodb_rs/src/parquet_spike.rs` (promover) |
| `read_parquet(path)` geral (schema arbitrário → SETOF) | a construir | pgrx SRF com column-def-list (`AS t(col type,…)`); bridge Arrow→PG datum de `df_executor::arrow_value_to_datum` |
| Bridge Arrow↔PG (tipos escalares) | ✅ existe | `df_executor.rs` — int2/4/8, float4/8, bool, text, timestamp/tz, date, decimal128 |
| Escrever Parquet (tabela→arquivo) | a construir | `DataFrame::write_parquet` (feature parquet) + ler linhas via `Spi::connect+select` → Arrow (`build_arrow`) |
| Runtime DataFusion in-extension | ✅ existe | `df_executor.rs` (tokio current-thread + `block_on` + `HeldInterrupts` + `GreedyMemoryPool`) |

## Tipos (decisão do owner: "amplo", com fail-closed)

O bridge cobre os escalares comuns (o que o M62 usa + a maioria dos casos lakehouse). "Amplo" = esses escalares
+ nested/list/struct **na medida do viável**; o **não-suportado RAISE erro tipado** (fail-closed, nunca
silencioso). Risco de escopo (grill R1): se nested/struct estourar um milestone, **dividir** (v1 escalares /
v2 nested) — measurement-first, não inchar.

## Estratégia de validação (medida)

1. Round-trip own-code: `htap_refresh(rel)` escreve parquet → `olap(rel)` lê+agrega → resultado correto, **sem
   pg_duckdb**. Reusa/estende `scripts/spike-parquet-validate.sh`.
2. Paridade: comparar o agregado own-code vs o baseline pg_duckdb (gerado uma vez pela imagem htap antiga).
3. `read_parquet(path)` geral: ler um Parquet multi-tipo (int/float/text/bool/timestamp) → linhas corretas;
   tipo não-suportado → erro tipado.
4. Imagem: build default com o lakehouse own-code (feature parquet permanente), `pg_extension` **sem** pg_duckdb;
   `Dockerfile.htap` removido; delta de tamanho (~118MB de C++ fora, ~9MB Rust dentro) em `docs/benchmarks/`.

## Referências

- Spike GO: `docs/benchmarks/parquet-reader-owncode-spike.md`, `theodb_rs/src/parquet_spike.rs`.
- Superfície: `sql/85-theodb-htap.sql`; bridge/runtime: `theodb_rs/src/am/df_executor.rs`.
- pg_duckdb: `Dockerfile`/`packaging/Dockerfile.htap` (M142, ADR-0056), ADR-0020/0021/0023.
- Regra 9 / parsimony rung 4: `.claude/rules/parsimony-ladder.md`.
