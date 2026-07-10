# syntax=docker/dockerfile:1
# TheoDB image — PostgreSQL 17 + theodb_rs (own vector type + ANN AM, M17/M69/M70) + pg_duckdb (M61).
# M70: pgvector + pgvectorscale REMOVIDOS — o tipo `vector` e os índices ANN são 100% own-code (theodb_rs).
# Multi-stage: os stages builder compilam as extensões Rust/C++; o runtime copia SÓ os artefatos.

# Shared base pinned by digest — used by BOTH stages so the extension is compiled against the exact
# same PostgreSQL the runtime ships (reproducible build; no moving target between builder and runtime).
ARG BASE_IMAGE=postgres:17-bookworm@sha256:17b6c778de50f4bb9a878c36e736110fbcd9b7020377d6fdfdf20f7c0347e40a

# ---- Stage 1: build theodb_rs (TheoDB's own Rust/pgrx extension — M17/M69/M70; own vector type + ANN AM) ----
# Compila a crate contra o MESMO PG pinado. M70: theodb_rs provê o tipo `vector` own-code (byte-idêntico ao
# pgvector) + os AMs theodb_hnsw/theodb_ivfflat + os schemas theodb/ai — sem depender do pgvector/pgvectorscale.
FROM ${BASE_IMAGE} AS theodb-rs-builder
ARG PG_MAJOR=17
ARG PGRX_VERSION=0.16.1
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

# ---- Stage 1c: build pg_duckdb (M61 — columnar/HTAP adoption; MIT, ADR 0013/0020) ----
# Adoption, NOT own-code (Regra 9): pg_duckdb (github.com/duckdb/pg_duckdb, MIT, GA v1.1.1, PG14-18 native)
# embeds the DuckDB analytical engine as a Postgres extension. Built statically (DUCKDB_BUILD=ReleaseStatic →
# one self-contained pg_duckdb.so, no separate libduckdb.so — ADR D2). C++/cmake/ninja, NO Rust.
FROM ${BASE_IMAGE} AS pgduckdb-builder
ARG PG_MAJOR=17
ARG PGDUCKDB_REF=v1.1.1
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential postgresql-server-dev-$PG_MAJOR libssl-dev pkg-config git cmake ninja-build \
      ca-certificates curl liblz4-dev libcurl4-openssl-dev zlib1g-dev && \
    rm -rf /var/lib/apt/lists/*
RUN git clone https://github.com/duckdb/pg_duckdb /tmp/pg_duckdb && \
    cd /tmp/pg_duckdb && git checkout $PGDUCKDB_REF && \
    git submodule update --init --recursive
RUN cd /tmp/pg_duckdb && DUCKDB_BUILD=ReleaseStatic make install -j"$(nproc)"

# ---- Stage 2: runtime (postgres:17 + theodb_rs + pg_duckdb) — M70: SEM pgvector/pgvectorscale ----
FROM ${BASE_IMAGE}
ARG PG_MAJOR=17

# theodb_rs artifacts (M17/M69/M70) — TheoDB's own Rust extension (.so + .control + .sql): provê o tipo
# `vector` own-code + os AMs ANN + os schemas theodb/ai. Artifact-only COPY (no Rust toolchain in runtime).
# minreq uses native-tls → the runtime needs libssl3 (present in postgres:17-bookworm base) + ca-certificates.
COPY --from=theodb-rs-builder /usr/lib/postgresql/$PG_MAJOR/lib/theodb_rs* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=theodb-rs-builder /usr/share/postgresql/$PG_MAJOR/extension/theodb_rs* /usr/share/postgresql/$PG_MAJOR/extension/

# M61 — pg_duckdb artifacts (columnar/HTAP; MIT; static DuckDB engine linked into pg_duckdb.so). Artifact-only
# COPY (no C++ toolchain in runtime). pg_duckdb REQUIRES shared_preload_libraries (loaded at boot) — appended below.
COPY --from=pgduckdb-builder /usr/lib/postgresql/$PG_MAJOR/lib/pg_duckdb* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=pgduckdb-builder /usr/share/postgresql/$PG_MAJOR/extension/pg_duckdb* /usr/share/postgresql/$PG_MAJOR/extension/
# Append pg_duckdb to shared_preload_libraries in the initdb template (ADR D3 — append-to-sample). Idempotent.
RUN grep -q "shared_preload_libraries.*pg_duckdb" /usr/share/postgresql/$PG_MAJOR/postgresql.conf.sample || \
    echo "shared_preload_libraries = 'pg_duckdb'" >> /usr/share/postgresql/$PG_MAJOR/postgresql.conf.sample

# ca-certificates IS required for TLS verification on HTTPS cloud endpoints — used by the Rust AI surface
# (minreq/native-tls/OpenSSL) in theodb_rs. libcurl4 is required by pg_duckdb.so (DuckDB httpfs).
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libcurl4 && \
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
        sql/theodb--1.2--1.3.sql sql/theodb--1.3--1.4.sql \
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
-- M61: pg_duckdb (columnar/HTAP, MIT — ADR 0013/0020) NÃO é dependência do theodb.control (adjunto analítico),
-- então criar explicitamente. Requires shared_preload_libraries='pg_duckdb' (set no postgresql.conf.sample).
CREATE EXTENSION IF NOT EXISTS pg_duckdb;
EOF

# M62 — the HTAP snapshot directory (theodb.htap_refresh_sql writes row→Parquet snapshots here, server-side as
# the postgres OS user). Created + chowned at build so the COPY does not fail with "No such file or directory".
RUN mkdir -p /var/lib/postgresql/htap && chown postgres:postgres /var/lib/postgresql/htap

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
