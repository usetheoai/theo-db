# Spike — leitor Parquet own-code (DataFusion, sem DuckDB) — VERÊDITO: VIÁVEL (GO)

> Medido 2026-07-22 no e2e-runner (165.227.121.20), PostgreSQL 18.4 (pgrx 0.19), `theodb_rs` compilado com
> `--features "pg18 spike-parquet"`. Fase 4 (spike falsificável) da `theodb-evolution`. Script:
> `scripts/spike-parquet-validate.sh`. Código: `theodb_rs/src/parquet_spike.rs`.

## Pergunta do spike

O `theodb_rs` consegue ler um Parquet externo **own-code** (via DataFusion + Arrow — Apache-2.0, já no binário;
**sem DuckDB**) e produzir o mesmo agregado que a superfície M62 (`olap_sql`) gera hoje via
`pg_duckdb.read_parquet`? A que **custo de tamanho** e com que **paridade**?

## Resultado medido

| Gate | Resultado |
|---|---|
| **Correção (ground truth)** | ✅ own-code retorna `a\|2\|15`, `b\|1\|5` (dados conhecidos: `(a,10),(a,20),(b,5)`) |
| **Paridade vs pg_duckdb** | ✅ **byte-a-byte idêntico** ao `SELECT * FROM duckdb.query($$…read_parquet…GROUP BY…$$)` |
| **Custo de tamanho** | ✅ **+9 MB** (`theodb_rs.so` 62 → 71 MB) — o leitor Parquet Rust puro (parquet + arrow-json/csv/ipc). ⚠️ **cross-ambiente** (o 62 é o `.so` da imagem docker; o 71 é o `.so` buildado no host pgrx — toolchains distintas). A medição same-env autoritativa é o **delta de imagem +12 MB** (M143 724 vs M142 default 712, ambos `docker images`) — ver `m143-pgduckdb-removal.md`. |
| **vs bundle DuckDB** | **118 MB** (`pg_duckdb.so` = 124.213.040 bytes, `stat` — medido no harness M142, `m142-pgduckdb-tiering.md`) → o own-code é ordem-de-grandeza menor |
| **Sem C++/httpfs** | ✅ Rust puro (DataFusion/Arrow), Apache-2.0 (D1-clean); nenhuma superfície SSRF |

```
pg_duckdb baseline:   a|2|15   b|1|5
own-code output:      a|2|15   b|1|5
GROUND_TRUTH_OK · PARITY_OK · SPIKE_PARQUET_OK
.so: baseline 62 MB → spike 71 MB  (leitor own-code = +9 MB, vs DuckDB 118 MB)
```

## Por que é viável (e NÃO é reescrever o DuckDB)

O trabalho pesado — o motor DataFusion + Arrow — **já está no binário** (M98/M99/M100) e roda dentro da extensão
com o runtime tokio já provado (`am/df_executor.rs`). O spike **reusa** (Regra 9) a mesma DataFrame API que o
`df_executor` usa para agregar, trocando a fonte `read_batch` (memória) por `read_parquet` (arquivo). É **ligar
um feature (`datafusion/parquet`) + uma função** — parsimony rung 4 (reusar o instalado), não reinvenção de motor.

## Escopo honesto (o que o spike NÃO provou ainda)

- **Escala/tipos complexos:** testado num arquivo pequeno (3 linhas, 2 colunas). Perf em arquivos grandes / muitos
  tipos é não-medida — é follow-on. A **correção e a paridade** estão provadas no caso representativo.
- **Só o READ+aggregate.** A remoção TOTAL do pg_duckdb também exige: **escrever Parquet** own-code (o
  `COPY TO parquet` do `htap_refresh_sql` → `DataFrame::write_parquet`, coberto pelo mesmo feature) e reescrever o
  `sql/85` (`olap_sql`/`htap_refresh_sql`) para o caminho próprio, então dropar o pg_duckdb do `Dockerfile.htap`.
- **Shape fixo** no spike (category/c/a). Um `read_parquet` de produção com schema arbitrário (SETOF record
  dinâmico) é mais trabalho no pgrx — mas a superfície M62 só precisa do agregado de shape fixo.

## Veredito → GO

**VIÁVEL.** O leitor Parquet own-code lê Parquet externo **byte-idêntico ao pg_duckdb**, a **+9 MB vs 118 MB**,
sem C++/httpfs, reusando o DataFusion já no binário. Isto **destrava** o milestone de remoção total do pg_duckdb
(read + write own-code + reescrever o M62 + dropar o bundle DuckDB), trocando 118 MB de C++ por ~9 MB de Rust
permissivo — mantendo a capacidade lakehouse.

## Reprodução

```bash
# no e2e-runner:
cd theodb_rs && cargo pgrx install --release --features "pg18 spike-parquet" \
  --pg-config ~/.pgrx/18.4/pgrx-install/bin/pg_config
bash scripts/spike-parquet-validate.sh   # GROUND_TRUTH_OK + PARITY_OK + SPIKE_PARQUET_OK + delta do .so
```
