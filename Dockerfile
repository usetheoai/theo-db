# syntax=docker/dockerfile:1
# TheoDB image (DEFAULT) — PostgreSQL 18 + theodb_rs (own vector type + ANN AM, M17/M69/M70).
# M70: pgvector + pgvectorscale REMOVIDOS — o tipo `vector` e os índices ANN são 100% own-code (theodb_rs).
# M142: pg_duckdb REMOVIDO do default (tier-out) — o lakehouse de arquivos externos (Parquet/Iceberg/CSV, aposta
#       D2) vive agora na imagem OPCIONAL `theodb-htap` (packaging/Dockerfile.htap, camada sobre esta). ADR-0056.
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
FROM ${BASE_IMAGE} AS theodb-rs-builder
ARG PG_MAJOR=18
# M142: repin de 0.16.1 → 0.19.0. cargo-pgrx e o crate pgrx são lockstep; theodb_rs foi para pgrx =0.19.0 no M98
# (impl(m98)) mas o Dockerfile nunca acompanhou (o repin M135 não pegou) — a imagem default não buildava. Fix.
ARG PGRX_VERSION=0.19.0
ARG RUST_VERSION=1.91.0
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential postgresql-server-dev-$PG_MAJOR libssl-dev pkg-config clang curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $RUST_VERSION
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install --locked cargo-pgrx --version $PGRX_VERSION
# Initialize the pgrx dev environment (compiles/links against the runtime's pg_config) BEFORE copying
# our source — this expensive layer is independent of the crate code, so editing lib.rs does not force
# a PostgreSQL recompile (only the COPY + install layers below rerun).
RUN cargo pgrx init --pg$PG_MAJOR "$(which pg_config)"
# Copy the crate (with its committed Cargo.lock for reproducibility — pgrx install does not re-resolve).
COPY theodb_rs/ /tmp/theodb_rs/
RUN cd /tmp/theodb_rs && cargo pgrx install --release --features pg$PG_MAJOR

# ---- Stage 2: runtime (postgres:18 + theodb_rs) — M70: SEM pgvector/pgvectorscale; M142: SEM pg_duckdb ----
# O pg_duckdb (columnar/HTAP lakehouse, aposta D2) foi tierado para a imagem opcional theodb-htap
# (packaging/Dockerfile.htap = FROM esta imagem + a camada pg_duckdb). ADR-0056.
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

# M15/M70 — TheoDB ships as an INSTALLABLE EXTENSION (CREATE EXTENSION theodb), not init-scripts.
# Build the install script (concat of the modular bodies in load order) + copy theodb.control + sql/theodb--*.sql
# into the PG extension dir. SQL-only install is a plain copy. M70: `theodb.control` requires `theodb_rs` (o flip —
# theodb_rs provê o tipo `vector` + os schemas theodb/ai); NÃO há mais dep de vector/vectorscale.
COPY theodb.control /tmp/theodb/theodb.control
COPY sql/ /tmp/theodb/sql/
RUN set -eux; \
    cd /tmp/theodb; \
    cat sql/30-theodb-embed.sql sql/40-theodb-hybrid.sql sql/50-theodb-ai.sql \
        sql/60-theodb-nl.sql sql/61-theodb-nl-config.sql sql/70-theodb-ml.sql \
        sql/80-theodb-migrate.sql sql/85-theodb-htap.sql > sql/theodb--1.0.sql; \
    install -m 0644 theodb.control sql/theodb--1.0.sql sql/theodb--1.0--1.1.sql sql/theodb--1.1--1.2.sql \
        sql/theodb--1.2--1.3.sql sql/theodb--1.3--1.4.sql sql/theodb--1.4--1.5.sql \
        "/usr/share/postgresql/$PG_MAJOR/extension/"; \
    rm -rf /tmp/theodb

# Create the extension on fresh DB init (greenfield — M15 ADR D3). M70: CASCADE puxa theodb_rs (o flip);
# NÃO puxa mais vector/vectorscale (removidos). theodb_rs provê o tipo `vector` own-code + os schemas.
COPY <<'EOF' /docker-entrypoint-initdb.d/00-create-theodb.sql
-- M15/M70: a superfície TheoDB (hybrid/ai/nl/ml/migrate) é o extension SQL `theodb`, que requer `theodb_rs`
-- (o flip — theodb_rs provê o tipo `vector` own-code + os schemas theodb/ai + os AMs ANN). CASCADE pulls theodb_rs.
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
-- theodb_rs (M17/M69/M70): o tipo `vector` own-code + a superfície embed/ai/nl em Rust + os AMs. Sem deps externas.
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
-- M142: pg_duckdb NÃO é mais criado no default (tier-out). A superfície HTAP codegen (theodb.htap_refresh_sql/
-- olap_sql) permanece na extensão theodb mas RAISE feature_not_supported (0A000) sem pg_duckdb (guard M142). Para
-- o lakehouse de arquivos externos, use a imagem theodb-htap (packaging/Dockerfile.htap). ADR-0056.
EOF

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
