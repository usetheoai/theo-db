# syntax=docker/dockerfile:1
# TheoDB image — PostgreSQL 18 + theodb_rs (own vector type + ANN AM + lakehouse Parquet own-code).
# M70: pgvector + pgvectorscale REMOVIDOS — o tipo `vector` e os índices ANN são 100% own-code (theodb_rs).
# M143: pg_duckdb REMOVIDO por completo — o lakehouse de arquivos externos (ler/escrever/agregar Parquet) é agora
#       own-code no theodb_rs (DataFusion/Arrow, sem DuckDB). Uma imagem só (a theodb-htap do M142 foi aposentada).
#       ADR-0057. Este é o ÚNICO artefato de imagem — sem componente C++/httpfs.
# Multi-stage: o stage builder compila a extensão Rust; o runtime copia SÓ os artefatos.

# Shared base pinned by digest — used by BOTH stages so the extension is compiled against the exact
# same PostgreSQL the runtime ships (reproducible build; no moving target between builder and runtime).
# M135: PG18. The digest pin from the 17 line is intentionally not carried over — a digest for the 18 image has
# to be taken from a real pull, and inventing one would be exactly the fabrication this project rejects. Pin it
# on the next release cut, once the image has actually been pulled and its digest recorded.
ARG BASE_IMAGE=postgres:18-bookworm

# ---- Stage 1: build theodb_rs (TheoDB's own Rust/pgrx extension — M17/M69/M70; own vector type + ANN AM) ----
# Compila a crate contra o MESMO PG pinado. M70: theodb_rs provê o tipo `vector` own-code (byte-idêntico ao
# pgvector) + os AMs theodb_hnsw/theodb_ivfflat + os schemas theodb/ai — sem depender do pgvector/pgvectorscale.
FROM ${BASE_IMAGE} AS theodb-toolchain
ARG PG_MAJOR=18
# M142: repin de 0.16.1 → 0.19.0. cargo-pgrx e o crate pgrx são lockstep; theodb_rs foi para pgrx =0.19.0 no M98
# (impl(m98)) mas o Dockerfile nunca acompanhou (o repin M135 não pegou) — a imagem default não buildava. Fix.
ARG PGRX_VERSION=0.19.0
# M142: repin de 1.91.0 → 1.97.1. cargo-pgrx 0.19.0 exige rustc ≥ 1.96; 1.97.1 é o toolchain provado no e2e-runner.
#
# B-026/B-025 (2026-08-12) — ALINHADO a 1.97.0, que é o que `theodb_rs/rust-toolchain.toml` declara, e o
# `rust-toolchain.toml` VENCE dentro do crate. O 1.97.1 daqui nunca compilou nada: medido no log do build,
# o rustup baixa 1.97.0 on-demand ao entrar em `theodb_rs/` ("syncing channel updates for 1.97.0"). Ou seja,
# o repin do M142 para .1 não teve efeito sobre o que de fato compila.
#
# Isso não era só cosmético — era o que **quebrava o `--component` abaixo**: os componentes iam para o
# 1.97.1 (o `--default-toolchain` deste RUN) enquanto todo `cargo` dentro do crate usava o 1.97.0, que não
# os tinha. Era por isso que rodar o gate na imagem exigia um `rustup component add --toolchain 1.97.0`
# manual. Uma versão só, num lugar só, e o drift deixa de existir.
ARG RUST_VERSION=1.97.0
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential postgresql-server-dev-$PG_MAJOR libssl-dev pkg-config clang curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
# B-025 — `--component clippy,rustfmt` no MESMO comando do rustup, não num `rustup component add` depois.
#
# O `--profile minimal` (correto, mantém a imagem enxuta) NÃO traz clippy nem rustfmt, e a ausência deles
# quebrava o gate de qualidade fora do CI de um jeito silencioso. Medido em 2026-08-11: `cargo clippy` na
# imagem devolve `error: 'cargo-clippy' is not installed for the toolchain '1.97.0'` com exit 1 — que é
# trivialmente lido como "o lint reprovou". Pior no fmt: `cargo fmt -- --check | grep -c "^Diff in"`
# imprimiu **0** — não porque estava limpo, mas porque o comando falhou e não produziu saída nenhuma. Um
# "tudo certo" falso, visualmente idêntico ao verdadeiro.
#
# O `lint-rust.yml` nunca sofreu com isso porque roda no runner self-hosted, que tem os componentes fora da
# imagem. Ou seja: a imagem que o projeto entrega não conseguia rodar o gate que o projeto exige — e o
# `.clippy_args` existe declaradamente para que "CI e local leiam o MESMO baseline, sem drift". O drift de
# ARGUMENTOS ele previne; o de FERRAMENTA passava.
#
# Componentes na mesma invocação (e não num `RUN` extra) por dois motivos: evita uma camada só para isso, e
# o rustup resolve tudo contra o toolchain pinado numa transação — sem chance de instalar componente de
# outra versão que a do `--default-toolchain`.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
      --component clippy,rustfmt --default-toolchain $RUST_VERSION
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install --locked cargo-pgrx --version $PGRX_VERSION
# Initialize the pgrx dev environment (compiles/links against the runtime's pg_config) BEFORE copying
# our source — this expensive layer is independent of the crate code, so editing lib.rs does not force
# a PostgreSQL recompile (only the COPY + install layers below rerun).
RUN cargo pgrx init --pg$PG_MAJOR "$(which pg_config)"

# Pré-requisitos do `cargo pgrx test`, aqui e não no runtime de cada execução.
#
# `initdb` RECUSA rodar como root — e a recusa não aparece como uma falha, aparece como uma AVALANCHE:
# medido em 2026-08-12, o primeiro `#[pg_test]` aborta duro segurando o mutex de teste do pgrx e os 373
# seguintes reprovam com "Could not obtain test mutex", sem executar. De 374 blocos de falha, UM continha
# a causa. O `CARGO_PGRX_TEST_RUNAS` resolve, mas exige `sudo` e um usuário com PGDATA próprio; deixar
# isso para um `apt-get install` dentro de cada execução paga rede toda vez e faz a suíte depender de
# repositório externo estar no ar para poder rodar.
RUN apt-get update && apt-get install -y --no-install-recommends sudo && \
    rm -rf /var/lib/apt/lists/* && \
    (id -u postgres >/dev/null 2>&1 || useradd -m postgres) && \
    mkdir -p /pgdata && chown postgres /pgdata
ENV CARGO_PGRX_TEST_RUNAS=postgres \
    CARGO_PGRX_TEST_PGDATA=/pgdata

# ---- Stage 1b: builder do produto — o ÚNICO estágio que contém código-fonte ----
#
# A separação é o ponto, e ela nasceu de um defeito medido em 2026-08-12: a imagem de teste usada
# localmente tinha o crate COPIADO dentro dela, em `/tmp/theodb_rs`. Um `docker run` que montasse o repo e
# copiasse por cima aninhava o diretório em vez de substituí-lo, e o `cargo` compilava a cópia velha —
# devolvendo **440 testes verdes que não testaram o código sob mudança**. Um falso verde visualmente
# idêntico ao verdadeiro, que é a pior forma de falha de verificação.
#
# Com o corte, `--target theodb-toolchain` produz uma imagem SEM fonte alguma. O modo de falha vira
# estrutural: montagem errada não compila outra coisa em silêncio, ela para com "could not find Cargo.toml".
# É a mesma disciplina do resto do projeto — preferir a falha alta à leitura plausível e errada.
#
# Para a suíte: `docker build --target theodb-toolchain -t theodb-toolchain .`, depois montar o repo e
# apontar CARGO_TARGET_DIR para um volume nomeado, de modo que a compilação seja incremental entre
# execuções. O job de teste também deixa de pagar o `cargo pgrx install --release` abaixo, que ele nunca
# reaproveita: perfil e features diferentes não compartilham artefato de compilação.
FROM theodb-toolchain AS theodb-rs-builder
# `ARG` não atravessa fronteira de estágio: sem esta redeclaração, `$PG_MAJOR` no RUN abaixo expandiria
# para vazio e o `--features pg` instalaria a extensão sem feature de versão do PostgreSQL.
ARG PG_MAJOR=18
# Copy the crate (with its committed Cargo.lock for reproducibility — pgrx install does not re-resolve).
COPY theodb_rs/ /tmp/theodb_rs/
RUN cd /tmp/theodb_rs && cargo pgrx install --release --features pg$PG_MAJOR

# ---- Stage 2: runtime (postgres:18 + theodb_rs) — SEM pgvector/pgvectorscale (M70); SEM pg_duckdb (M143) ----
# O lakehouse é own-code no theodb_rs (DataFusion/Arrow) — nenhum componente C++/httpfs. ADR-0057.
#
# Sobre docker:S6471 ("the postgres image runs with root as the default user") — aviso ACEITO com
# justificativa; o marcador NOSONAR não é suportado em Dockerfile, então o hotspot precisa ser marcado
# como *safe* no dashboard do SonarCloud. O entrypoint oficial da imagem `postgres` PRECISA iniciar como root
# para ajustar as permissões do PGDATA (chown do volume no primeiro boot) e só então rebaixar o privilégio
# via `gosu postgres` — o servidor NUNCA roda como root. Declarar `USER postgres` aqui quebraria o initdb
# em volumes novos, trocando um falso-positivo de análise estática por uma falha real de produto. Quem
# quiser fixar o usuário no deploy pode passar `--user` no `docker run` sobre um volume já provisionado.
FROM ${BASE_IMAGE}
ARG PG_MAJOR=18

# theodb_rs artifacts (M17/M69/M70) — TheoDB's own Rust extension (.so + .control + .sql): provê o tipo
# `vector` own-code + os AMs ANN + os schemas theodb/ai. Artifact-only COPY (no Rust toolchain in runtime).
# minreq uses native-tls → the runtime needs libssl3 (present in postgres:18-bookworm base) + ca-certificates.
COPY --from=theodb-rs-builder /usr/lib/postgresql/$PG_MAJOR/lib/theodb_rs* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=theodb-rs-builder /usr/share/postgresql/$PG_MAJOR/extension/theodb_rs* /usr/share/postgresql/$PG_MAJOR/extension/

# ca-certificates IS required for TLS verification on HTTPS cloud endpoints — used by the Rust AI surface
# (minreq/native-tls/OpenSSL) in theodb_rs. (M142: libcurl4 removido — era dep do pg_duckdb httpfs, agora só na htap.)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# B-030 — UMA extensão de produto: `theodb_rs`. O umbrella SQL-only `theodb` foi absorvido por ela: a
# superfície ai/nl/ml/migrate/htap virou `extension_sql_file!` em `theodb_rs/src/surface.rs`, lendo os
# mesmos arquivos SQL, agora sob `theodb_rs/sql/surface/`. Com o umbrella saíram `theodb.control`, o
# `Makefile` PGXS, a cadeia `theodb--X--Y.sql` e a concatenação de corpos que este bloco fazia.
#
# M148 (#181) — o shim `vector` PERMANECE separado, e de propósito: toda app pgvector roda
# `CREATE EXTENSION IF NOT EXISTS vector` no bootstrap (drizzle/alembic/prisma/scripts). Sem um objeto de
# extensão com esse nome a app NÃO sobe — nem chega a emitir uma query. O shim não implementa nada (o
# tipo/operadores/opclasses são own-code do theodb_rs); ele completa o drop-in da ADR-0029 § D2 no nível
# tooling. Aqui o NOME é o contrato, então colapsá-lo reintroduziria o bloqueio que o #181 mediu.
COPY vector.control /tmp/theodb/vector.control
COPY sql/ /tmp/theodb/sql/
RUN set -eux; \
    cd /tmp/theodb; \
    install -m 0644 vector.control sql/vector--*.sql \
        "/usr/share/postgresql/$PG_MAJOR/extension/"; \
    rm -rf /tmp/theodb

# Create the extension on fresh DB init (greenfield — M15 ADR D3). M70: CASCADE puxa theodb_rs (o flip);
# NÃO puxa mais vector/vectorscale (removidos). theodb_rs provê o tipo `vector` own-code + os schemas.
COPY <<'EOF' /docker-entrypoint-initdb.d/00-create-theodb.sql
-- B-030: UMA extensão entrega a superfície inteira — o tipo `vector` own-code, os AMs ANN, os schemas
-- `theodb`/`ai`/`theodb_ml` e toda a superfície embed/ai/nl/ml/migrate/htap. O umbrella `theodb` deixou
-- de existir; não há mais um segundo `CREATE EXTENSION` a emitir aqui.
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
-- M143: o lakehouse é own-code no theodb_rs (theodb.htap_refresh/olap + read_parquet/write_parquet own-code, sem
-- DuckDB). Nada de pg_duckdb. ADR-0057.

-- M148 (#181, ADR-0058) — o shim `vector` declara `requires = theodb_rs`, e o tooling real das apps
-- (drizzle, alembic, prisma, pg_restore) emite `CREATE EXTENSION IF NOT EXISTS vector` **SEM** CASCADE.
-- Num banco que não tenha o theodb_rs isso falha com `required extension "theodb_rs" is not installed`,
-- e a app não sobe — o bloqueio que o #181 existe para remover. Instalar em `template1` faz TODO banco
-- criado depois (`CREATE DATABASE app`, multi-tenant, os DBs de teste do CI) herdar a dependência já
-- satisfeita, então o comando sem CASCADE funciona. `\c template1` é a única forma de alcançar bancos
-- que ainda não existem no momento do initdb.
\c template1
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
CREATE EXTENSION IF NOT EXISTS vector;
EOF

# M62/M143 — o diretório de snapshots HTAP: theodb.htap_refresh escreve os Parquet own-code (public.write_parquet)
# aqui, server-side como o usuário postgres. Criado + chowned no build (o writer own-code não cria o dir base).
RUN mkdir -p /var/lib/postgresql/htap && chown postgres:postgres /var/lib/postgresql/htap

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
