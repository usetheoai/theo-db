# ADR 0057 — Remoção total do pg_duckdb: lakehouse Parquet own-code (M143)

- **Status:** Accepted · **Date:** 2026-07-22 · **Milestone:** M143 · **Deciders:** CTO (paulohenriquevn)
- **Emenda:** ADR `0056-m142-pgduckdb-htap-tiering` (tier-out — um passo intermediário; agora o pg_duckdb é
  removido por completo) e ADR `0020-m61-embed-pgduckdb` (a adoção original).
- **Relacionado:** ADR `0021`/`0023` (superfície M62), ADR `0042`/M100 (DataFusion executor own-code reusado),
  `docs/benchmarks/parquet-reader-owncode-spike.md` (o spike Fase 4, GO), `docs/benchmarks/m143-pgduckdb-removal.md`
  (a evidência da remoção), `.claude/rules/parsimony-ladder.md` (rung 4).

## Contexto e problema

O M61 (ADR-0020) adotou o `pg_duckdb` para o lakehouse (ler/agregar Parquet externo); o M142 (ADR-0056) o tierou
para uma imagem opcional `theodb-htap`. O `pg_duckdb` era o **único componente C++/httpfs** do projeto (bundle
DuckDB estático de 118 MB). O spike da Fase 4 (`docs/benchmarks/parquet-reader-owncode-spike.md`) **mediu** que
ler Parquet own-code via DataFusion (Apache-2.0, já no binário) é **viável** — paridade byte-a-byte a **+9 MB** vs
os 118 MB do DuckDB. Este ADR conclui a jornada: **remove o pg_duckdb por completo**, substituindo a superfície
por own-code, e dobra o lakehouse no build default (a imagem `theodb-htap` é aposentada).

## Decisões

**D1 — Superfície de leitura own-code: `read_parquet`→SETOF jsonb + `olap`→tipado.** `public.read_parquet(path)`
retorna `SETOF jsonb` (cada linha Parquet → um jsonb via arrow-json — cobre **todos** os tipos, incl. nested,
sem a complexidade de `SETOF record` dinâmico no pgrx). `public.olap(path)` retorna o agregado M62 tipado
(category/count/avg — paridade byte-a-byte vs pg_duckdb, provada). Reusa a DataFrame API + o runtime tokio
in-extension do `df_executor` (Regra 9).

**D2 — Escrita own-code + colapso do codegen.** `public.write_parquet(rel, path)` lê a tabela (SPI) → Arrow →
`parquet::arrow::ArrowWriter` (arquivo único, atômico temp+rename). E o design "codegen" do M62 (funções que
RETORNAVAM texto para o cliente rodar) **colapsa**: existia só porque "pg_duckdb proíbe DuckDB dentro de função" —
restrição que some com own-code. `theodb.htap_refresh(rel)` escreve+registra e `theodb.olap(rel)` lê+agrega,
ambos **dentro da função**. Sem `duckdb.query`, sem `COPY … (FORMAT parquet)`, sem o guard M142.

**D3 — Uma imagem só (lakehouse no default; `theodb-htap` aposentada).** A feature `spike-parquet` foi promovida
a permanente (`datafusion/parquet` + `arrow/json`) — o lakehouse vem no `theodb_rs` do build default.
`packaging/Dockerfile.htap` e o job CI `htap-image` foram deletados. Custo medido: **+9 MB** no `theodb_rs.so`
vs os **118 MB** do bundle DuckDB removido — ganho líquido enorme, decisão do owner.

**D4 — Tipos: amplos na leitura, escalares na escrita v1 (fail-closed).** A leitura (jsonb) cobre todos os tipos
(escalares e nested, via arrow-json). A escrita v1 cobre os escalares comuns (int2/4/8, float4/8, bool, text);
tipo não-suportado na escrita → **erro tipado fail-closed** (legível via read_parquet; a escrita ampla de
nested/timestamp/decimal é follow-on — measurement-first, grill R1).

> **Least-privilege (review M143):** as primitivas `public.read_parquet`/`write_parquet`/`olap` (escrita/leitura de arquivo server-side) têm `REVOKE ALL FROM PUBLIC` (superuser-only, como o `COPY … TO file`) via `extension_sql!` no `parquet.rs`; a superfície de usuário `theodb.htap_refresh`/`olap` também é REVOKEd (sql/85). Um role sem privilégio não contorna chamando as primitivas direto.

## Alternativas rejeitadas

1. **Manter o pg_duckdb (tier-out do M142)** — REJEITADO: o spike mediu que o own-code custa 1/13 do tamanho com
   paridade; manter 118 MB de C++/httpfs por uma capacidade que own-code entrega é complexidade acidental.
2. **`SETOF record` dinâmico no leitor geral** — REJEITADO (D1): complexo no pgrx; nested não mapeia para colunas
   escalares → re-work. `SETOF jsonb` cobre tudo e é simples.
3. **Manter o codegen chamando own-code** — REJEITADO (D2): mantém a dança cliente-executa sem a restrição que a
   justificava.
4. **Reescrever o motor DuckDB inteiro own-code** — REJEITADO: anos/PhD (o motor DataFusion já resolve; só faltava
   ligar o `parquet` — parsimony rung 4).

## Consequências

- **Habilita:** lakehouse Parquet own-code (ler/escrever/agregar) no build default, **sem DuckDB**; uma imagem só,
  sem componente C++/httpfs (o último saiu). Extensão `theodb` bumpada 1.6 (`theodb--1.5--1.6.sql`).
- **Restringe:** a escrita v1 cobre escalares (tipo exótico → erro tipado); a escrita ampla é follow-on.
- **Backward compat:** as antigas `theodb.htap_refresh_sql`/`olap_sql` (codegen) foram **removidas** (DROP no
  upgrade); quem chamava o fluxo codegen migra para `theodb.htap_refresh(rel)` + `theodb.olap(rel)` (mais simples).
  Blast radius baixo (pré-1.0, sem dogfood em produção).

## Validação (medida)

Provado no droplet (PG18.4): round-trip own-code (`write_parquet`→`read_parquet`→`olap`) sem pg_duckdb; leitor
jsonb multi-tipo; escrita fail-closed em tipo não-suportado; `pg_extension` sem pg_duckdb; delta de tamanho
(118 MB de C++ fora, +9 MB de Rust dentro). Evidência: `docs/benchmarks/m143-pgduckdb-removal.md`,
`docs/benchmarks/parquet-reader-owncode-spike.md`.

## Referências

- ADR-0020/0056 (adoção + tier-out — emendados), ADR-0021/0023/0042.
- `theodb_rs/src/parquet.rs` (o own-code), `sql/85-theodb-htap.sql` (a superfície reescrita),
  `sql/theodb--1.5--1.6.sql` (o upgrade), `scripts/m143-removal-validate.sh` (a suíte de validação).
