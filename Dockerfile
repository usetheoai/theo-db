# syntax=docker/dockerfile:1
# TheoDB image — PostgreSQL 17 + pgvector (M0) + pgvectorscale StreamingDiskANN (M2 DoD-2).
# Multi-stage: the `scale-builder` stage compiles the Rust/pgrx extension; the runtime stage copies
# ONLY the artifacts (no Rust toolchain shipped — runtime stays ~445 MB).

# Shared base pinned by digest — used by BOTH stages so the extension is compiled against the exact
# same PostgreSQL the runtime ships (reproducible build; no moving target between builder and runtime).
ARG BASE_IMAGE=postgres:17-bookworm@sha256:17b6c778de50f4bb9a878c36e736110fbcd9b7020377d6fdfdf20f7c0347e40a

# ---- Stage 1: build pgvectorscale (Rust + cargo-pgrx) ----
FROM ${BASE_IMAGE} AS scale-builder
ARG PG_MAJOR=17
ARG PGVECTORSCALE_REF=57c88b7b4fe40a2afa20b195f60047a983279c19
ARG PGRX_VERSION=0.16.1
ARG RUST_VERSION=1.91.0
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential postgresql-server-dev-$PG_MAJOR libssl-dev pkg-config clang git curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $RUST_VERSION
ENV PATH="/root/.cargo/bin:${PATH}"
RUN git clone https://github.com/timescale/pgvectorscale /tmp/pgvectorscale && \
    cd /tmp/pgvectorscale && git checkout $PGVECTORSCALE_REF
RUN cargo install --locked cargo-pgrx --version $PGRX_VERSION
# `cargo pgrx install` does not accept --locked; crate-tree reproducibility comes from the committed
# Cargo.lock at the pinned PGVECTORSCALE_REF (pgrx install does not re-resolve dependencies by default).
RUN cd /tmp/pgvectorscale/pgvectorscale && \
    cargo pgrx init --pg$PG_MAJOR "$(which pg_config)" && \
    cargo pgrx install --release --features pg$PG_MAJOR

# ---- Stage 1b: build theodb_rs (TheoDB's own Rust/pgrx extension — M17, ROADMAP-v2 / ADR 0006) ----
# Mirrors the scale-builder pattern (plan ADR D3). Compiles our crate against the SAME pinned PG.
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

# ---- Stage 2: runtime (postgres:17 + pgvector + pgvectorscale + theodb_rs + pg_duckdb) ----
FROM ${BASE_IMAGE}
ARG PG_MAJOR=17

ADD https://github.com/pgvector/pgvector.git#586e7515bafe6912c425164d186d56550657c349 /tmp/pgvector

RUN apt-get update && \
    apt-mark hold locales && \
    apt-get install -y --no-install-recommends build-essential postgresql-server-dev-$PG_MAJOR && \
    cd /tmp/pgvector && \
    make clean && \
    make OPTFLAGS="" && \
    make install && \
    mkdir /usr/share/doc/pgvector && \
    cp LICENSE README.md /usr/share/doc/pgvector && \
    rm -r /tmp/pgvector && \
    apt-get remove -y build-essential postgresql-server-dev-$PG_MAJOR && \
    apt-get autoremove -y && \
    apt-mark unhold locales && \
    rm -rf /var/lib/apt/lists/*

# pgvectorscale artifacts (the .so + .control + .sql) — no Rust toolchain in the runtime image
COPY --from=scale-builder /usr/lib/postgresql/$PG_MAJOR/lib/vectorscale* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=scale-builder /usr/share/postgresql/$PG_MAJOR/extension/vectorscale* /usr/share/postgresql/$PG_MAJOR/extension/

# theodb_rs artifacts (M17) — TheoDB's own Rust extension (.so + .control + .sql). Same artifact-only
# COPY pattern as pgvectorscale (no Rust toolchain in runtime). minreq uses native-tls → the runtime
# needs libssl3 (present in postgres:17-bookworm base) + ca-certificates (installed below for HTTPS).
COPY --from=theodb-rs-builder /usr/lib/postgresql/$PG_MAJOR/lib/theodb_rs* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=theodb-rs-builder /usr/share/postgresql/$PG_MAJOR/extension/theodb_rs* /usr/share/postgresql/$PG_MAJOR/extension/

# M61 — pg_duckdb artifacts (columnar/HTAP; MIT; static DuckDB engine linked into pg_duckdb.so). Artifact-only
# COPY (no C++ toolchain in runtime), same pattern as pgvectorscale/theodb_rs. pg_duckdb REQUIRES
# shared_preload_libraries (loaded at boot, unlike pgvector's lazy LOAD) — appended idempotently below.
COPY --from=pgduckdb-builder /usr/lib/postgresql/$PG_MAJOR/lib/pg_duckdb* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=pgduckdb-builder /usr/share/postgresql/$PG_MAJOR/extension/pg_duckdb* /usr/share/postgresql/$PG_MAJOR/extension/
# Append pg_duckdb to shared_preload_libraries in the initdb template (ADR D3 — append-to-sample, not
# ALTER SYSTEM which only takes effect post-restart, too late for the init-time CREATE EXTENSION). Idempotent:
# only appends if not already present. Fail-closed: the smoke test asserts the extension loads.
RUN grep -q "shared_preload_libraries.*pg_duckdb" /usr/share/postgresql/$PG_MAJOR/postgresql.conf.sample || \
    echo "shared_preload_libraries = 'pg_duckdb'" >> /usr/share/postgresql/$PG_MAJOR/postgresql.conf.sample

# NO plpython3u — the whole `theodb` surface (embed M17, ai.* M18, nl_to_sql M19) is served by the Rust
# `theodb_rs` extension; `theodb.control` requires only vector+vectorscale (no plpython3u). The image no
# longer ships `postgresql-plpython3` (it was dead weight since M19).
# ca-certificates IS still required for TLS verification on HTTPS cloud endpoints — used by the Rust AI
# surface (minreq/native-tls/OpenSSL) in theodb_rs; without it cert verification fails.
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libcurl4 && \
    rm -rf /var/lib/apt/lists/*
# M61: pg_duckdb.so dynamically links libcurl (DuckDB's httpfs) → the runtime needs libcurl4 (the DuckDB
# engine itself is static via ReleaseStatic, but system libcurl is not bundled). Without it PG fails to boot
# (shared_preload_libraries load error). Added to the ca-certificates layer above.

# M15 — TheoDB ships as an INSTALLABLE EXTENSION (CREATE EXTENSION theodb), not init-scripts.
# Build the install script (concat of the modular bodies in load order) and install the SQL-only extension
# by copying theodb.control + sql/theodb--*.sql into the PG extension dir. SQL-only install is a plain copy —
# no PGXS/make at runtime (the dev package was removed above; M15 ADR D2 / EC-2). Schemas ai/theodb/theodb_ml
# are created in-script by the extension. Deps (vector/vectorscale) come from theodb.control requires.
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

# Create the extension on fresh DB init (greenfield — M15 ADR D3). CASCADE pulls vector+vectorscale.
COPY <<'EOF' /docker-entrypoint-initdb.d/00-create-theodb.sql
-- M15: the TheoDB surface (hybrid/ai/nl/ml/migrate) ships in the SQL `theodb` extension, which OWNS the
-- `theodb` schema. Create it FIRST. CASCADE pulls vector + vectorscale. Its plpgsql function
-- bodies reference theodb.embed lazily (late-bound at call time), so theodb.embed need not exist yet.
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
-- M17: theodb.embed is served by TheoDB's own Rust extension theodb_rs (plan ADR D1). It requires
-- `theodb` (the schema owner) and installs the public theodb.embed INTO the existing theodb schema
-- (its own objects live in schema theodb_rs). CASCADE pulls `vector`.
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;
-- M61: pg_duckdb (columnar/HTAP, MIT — ADR 0013/0020) is part of the distribution but NOT a theodb.control
-- dependency (it is an analytical adjunct, not required by the vector/AI surface), so create it explicitly.
-- Requires shared_preload_libraries='pg_duckdb' (set in postgresql.conf.sample at build).
CREATE EXTENSION IF NOT EXISTS pg_duckdb;
EOF

# M62 — the HTAP snapshot directory (theodb.htap_refresh_sql writes row→Parquet snapshots here, server-side as
# the postgres OS user). Created + chowned at build so the COPY does not fail with "No such file or directory".
# Outside PGDATA (/var/lib/postgresql/data volume) — snapshots are regenerable (re-refresh), not durable state.
RUN mkdir -p /var/lib/postgresql/htap && chown postgres:postgres /var/lib/postgresql/htap

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
