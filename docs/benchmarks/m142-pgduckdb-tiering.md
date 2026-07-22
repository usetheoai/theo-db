# M142 — pg_duckdb tier-out: delta de tamanho das imagens + smokes das duas imagens (medido)

> Medido 2026-07-22 no e2e-runner (165.227.121.20, Docker 29.4.1, PostgreSQL **18.4**, pgrx 0.19.0). As duas
> imagens buildadas do zero (`Dockerfile` default + `packaging/Dockerfile.htap` FROM a default). Script:
> `scripts/m142-tiering-validate.sh` (build + smoke das 2 imagens + delta, todos os gates falham exit≠0).
> Reprodução no fim.

## Headline

**O tier-out do pg_duckdb enxuga a imagem default em 175 MB (887 → 712 MB), removendo o único componente C++/
httpfs; a capacidade lakehouse continua opt-in na imagem `theodb-htap` (= default + pg_duckdb), com a superfície
M62 funcionando e2e.** A imagem default fica sem pg_duckdb (nem extensão, nem `shared_preload`, nem `libcurl4`) e
a superfície HTAP codegen falha-claro (`0A000`, com dica para a imagem htap).

## Evidência 1 — delta de tamanho (o Goal medido)

| Imagem | Tamanho (`docker images`) |
|---|---|
| `theodb:m142-default` (sem pg_duckdb) | **712 MB** |
| `theodb:m142-htap` (default + pg_duckdb) | **887 MB** |
| **Delta** | **175 MB** (≥ 150 MB — gate do DoD) |

O delta é o `pg_duckdb.so` (bundle DuckDB estático, `DUCKDB_BUILD=ReleaseStatic`) — **~118 MB** medido
(`124.213.040 bytes`) — mais o `libcurl4` (httpfs) e o overhead de layer. Consistente com o "+170 MB" que o
ADR-0020 estimou para a adoção do pg_duckdb.

> Nota de honestidade (Rule 3): a primeira tentativa de medição usou `docker image inspect --format {{.Size}}`,
> que reportou valores divergentes (168/214 MB) neste Docker; a medição correta é `docker images` (712/887 MB),
> que o script passou a usar. Nenhum número foi estimado — todos vêm de `docker images` contra as tags reais.

## Evidência 2 — smoke da imagem DEFAULT (`DEFAULT_OK`)

```
== DEFAULT smoke ==
DEFAULT_OK
```

Provado contra a imagem default buildada:

| Gate | O que prova |
|---|---|
| `pg_extension` sem `pg_duckdb` (count=0) | o tier-out removeu a extensão do default |
| `shared_preload_libraries` sem `pg_duckdb` | não é mais carregado no boot |
| `theodb`+`theodb_rs` presentes; tipo `vector` own-code funciona | o núcleo do TheoDB ficou intacto |
| `theodb_columnar` TableAM: `CREATE ... USING theodb_columnar` + readback | o colunar in-DB own-code (M99) intacto |
| `theodb.olap_sql('t')` → **RAISE `0A000`** (feature_not_supported) | o **guard M142** fail-closed dispara sem pg_duckdb |
| `htap_guard_test.sql` exit 0 (caminho guard) | a superfície HTAP falha-claro, nunca statement quebrado |

## Evidência 3 — smoke da imagem HTAP (`HTAP_OK`)

```
== HTAP smoke ==
HTAP_OK
```

Provado contra a imagem `theodb-htap`:

| Gate | O que prova |
|---|---|
| `pg_extension` COM `pg_duckdb` (count=1) | a camada pg_duckdb foi re-adicionada |
| `theodb.htap_refresh_sql(t)` → COPY parquet **executa** (cliente) | o writer Parquet do pg_duckdb funciona |
| `theodb.olap_sql(t)` → `duckdb.query` **executa**; resultado `a\|2\|15`, `b\|1\|5` | a superfície M62 e2e (row→Parquet→OLAP) funciona |
| `htap_guard_test.sql` exit 0 (caminho positivo) | o guard passa quando pg_duckdb está presente |

> Limitação pré-existente da M62 (não do tier-out): o `COPY → Parquet` do pg_duckdb rejeita `NUMERIC` sem
> precisão ("DuckDB requires the precision of a NUMERIC to be set"). A tabela de teste usa `double precision`
> (a mesma escolha do harness M61/M62). Fora do escopo do M142.

## Consequência para o roadmap

- **Gate M142 PASSA** (`M142_TIERING_OK`) — default 175 MB menor sem pg_duckdb, guard fail-closed provado, htap
  opt-in com M62 e2e. Fecha o M142.
- **Fixes colaterais medidos** (a imagem default não buildava desde o M98): `PGRX_VERSION` `0.16.1`→`0.19.0` e
  `RUST_VERSION` `1.91.0`→`1.97.1` no Dockerfile (cargo-pgrx 0.19 exige rustc ≥ 1.96). Sem eles nenhuma imagem
  buildava.

## Reprodução

```bash
# no e2e-runner (Docker + a raiz do repo):
bash scripts/m142-tiering-validate.sh
# → build theodb:m142-default (sem pg_duckdb) + theodb:m142-htap (FROM default + pg_duckdb)
# → DEFAULT_OK + HTAP_OK + "== DELTA: default=712MB htap=887MB delta=175MB" + M142_TIERING_OK
# reusar imagens já buildadas: SKIP_BUILD=1 bash scripts/m142-tiering-validate.sh
```
