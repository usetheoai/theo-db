# M143 — remoção total do pg_duckdb: lakehouse Parquet own-code (medido)

> Medido 2026-07-22 no e2e-runner (165.227.121.20), PostgreSQL **18.4**, Docker. A imagem default **M143**
> (`theodb:m143`) buildada do zero e validada com `scripts/m143-removal-validate.sh`. Código:
> `theodb_rs/src/parquet.rs`, `sql/85-theodb-htap.sql`. ADR: `docs/adr/0057`. Spike de viabilidade:
> `docs/benchmarks/parquet-reader-owncode-spike.md`.

## Headline

**O `pg_duckdb` foi removido por completo — o último componente C++/httpfs do projeto saiu.** O lakehouse
(ler/escrever/agregar Parquet externo) é agora **own-code** em Rust (DataFusion/Arrow, Apache-2.0), no build
**default** (uma imagem só; a `theodb-htap` do M142 foi aposentada). Provado na imagem shipada, sem DuckDB.

## Evidência — validação e2e na imagem default M143 (`scripts/m143-removal-validate.sh`)

```
== gate: pg_duckdb REMOVIDO ==            NO_PGDUCKDB
== gate: M62 own-code (htap_refresh→olap) M62_OWNCODE     (a|2|15, b|1|5)
== gate: read_parquet multi-tipo         READ_MULTI      ({"n":1,"flag":true,"amount":10.0,"category":"a"})
== gate: write_parquet fail-closed       WRITE_FAILCLOSED (timestamp → erro tipado, backend vivo)
== gate: REVOKE least-privilege          REVOKE_OK (lowpriv bloqueado em read/write_parquet)
== tamanho da imagem                     theodb:m143 = 724 MB
M143_REMOVAL_OK
```

| Gate | O que prova |
|---|---|
| **NO_PGDUCKDB** | `pg_extension` sem pg_duckdb; `shared_preload_libraries` sem ele — removido por completo |
| **M62_OWNCODE** | `theodb.htap_refresh(rel)` (escreve snapshot own-code) + `theodb.olap(rel)` (lê+agrega own-code) = `a\|2\|15`/`b\|1\|5`, **sem DuckDB** |
| **READ_MULTI** | `public.read_parquet(path)` → jsonb com todos os tipos (int/float/text/bool) via arrow-json |
| **WRITE_FAILCLOSED** | `public.write_parquet` de coluna timestamp (não-suportado v1) → erro tipado; backend vivo (`SELECT 1`=1) |
| **REVOKE_OK** | um role **sem privilégio** (`lowpriv`) é **bloqueado** (`permission denied`) em `public.write_parquet`/`read_parquet` — escrita/leitura de arquivo server-side é superuser-only (least-privilege, review HIGH-1) |

## Tamanho — o ganho medido

| Imagem | Tamanho | Lakehouse? | pg_duckdb (C++) |
|---|---|---|---|
| M142 default | 712 MB | ❌ (só via htap) | não |
| M142 `theodb-htap` (aposentada) | 887 MB | ✅ (via pg_duckdb) | **+118 MB** |
| **M143 default (esta)** | **724 MB** | ✅ **own-code** | **0** |

> Os três tamanhos vêm de `docker images` no mesmo e2e-runner (build-do-zero); 712/887 e o `pg_duckdb.so` = 118 MB
> (124.213.040 bytes, `stat`) foram medidos no harness M142 — ver `docs/benchmarks/m142-pgduckdb-tiering.md`.

**Número autoritativo (same-env):** o lakehouse own-code custa **+12 MB** no build default (M143 724 − M142
default 712, **ambos `docker images`, mesmo ambiente**). O `+9 MB` do `.so` medido no spike é uma corroboração
**cross-ambiente** (o `.so` da imagem docker vs o `.so` buildado no host pgrx — toolchains distintas; ver a nota de
ablação no spike doc), por isso o número de imagem `+12 MB` é o que citamos. Conclusão medida: **118 MB de
C++/httpfs removidos; +12 MB de Rust permissivo no build default no lugar.**

## O que a remoção envolveu (jornada M142→M143)

- **M142 (tier-out):** pg_duckdb saiu do default → imagem opcional `theodb-htap`.
- **Spike (Fase 4):** mediu que ler Parquet own-code é viável (paridade byte-a-byte, +9 MB).
- **M143 (remoção total):** `read_parquet` (jsonb) + `write_parquet` (ArrowWriter) + `olap` (agregado) own-code;
  `sql/85` reescrito (`htap_refresh`/`olap` diretos — o codegen do pg_duckdb colapsou, pois own-code roda dentro
  da função); `pg_duckdb` + `Dockerfile.htap` + job CI `htap-image` deletados; lakehouse dobrado no default.

## Escopo honesto

- **Leitura:** ampla (jsonb cobre todos os tipos, incl. nested via arrow-json).
- **Escrita v1:** escalares (int2/4/8, float4/8, bool, text); tipo não-suportado → erro tipado fail-closed
  (legível via read_parquet; escrita ampla de nested/timestamp/decimal é follow-on — measurement-first).
- **Paridade:** `olap` byte-a-byte vs o pg_duckdb (provada no spike + reproduzida aqui num arquivo pequeno).
  Perf em arquivos grandes é follow-on.

## Reprodução

```bash
# no e2e-runner:
bash scripts/m143-removal-validate.sh   # build theodb:m143 + NO_PGDUCKDB + M62_OWNCODE + READ_MULTI + WRITE_FAILCLOSED
# reusar a imagem: SKIP_BUILD=1 TAG=theodb:m143 bash scripts/m143-removal-validate.sh
```
