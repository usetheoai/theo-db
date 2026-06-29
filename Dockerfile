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

# ---- Stage 2: runtime (postgres:17 + pgvector + pgvectorscale) ----
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

# plpython3u for theodb.embed (M2 DoD-3) — the DB calls a configurable model endpoint (AlloyDB pattern);
# NO model/torch ships in the image (lean). Kept (not removed) — runtime dependencies.
# ca-certificates is required for plpython3u's urllib to verify TLS when the endpoint is an HTTPS cloud
# provider (e.g. OpenAI); without it CREATE-CERT verification fails (SSL: CERTIFICATE_VERIFY_FAILED).
RUN apt-get update && \
    apt-get install -y --no-install-recommends postgresql-plpython3-$PG_MAJOR ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# M15 — TheoDB ships as an INSTALLABLE EXTENSION (CREATE EXTENSION theodb), not init-scripts.
# Build the install script (concat of the modular bodies in load order) and install the SQL-only extension
# by copying theodb.control + sql/theodb--*.sql into the PG extension dir. SQL-only install is a plain copy —
# no PGXS/make at runtime (the dev package was removed above; M15 ADR D2 / EC-2). Schemas ai/theodb/theodb_ml
# are created in-script by the extension. Deps (vector/vectorscale/plpython3u) come from theodb.control requires.
COPY theodb.control /tmp/theodb/theodb.control
COPY sql/ /tmp/theodb/sql/
RUN set -eux; \
    cd /tmp/theodb; \
    cat sql/30-theodb-embed.sql sql/40-theodb-hybrid.sql sql/50-theodb-ai.sql \
        sql/60-theodb-nl.sql sql/61-theodb-nl-config.sql sql/70-theodb-ml.sql > sql/theodb--1.0.sql; \
    install -m 0644 theodb.control sql/theodb--1.0.sql sql/theodb--1.0--1.1.sql \
        "/usr/share/postgresql/$PG_MAJOR/extension/"; \
    rm -rf /tmp/theodb

# Create the extension on fresh DB init (greenfield — M15 ADR D3). CASCADE pulls vector+vectorscale+plpython3u.
COPY <<'EOF' /docker-entrypoint-initdb.d/00-create-theodb.sql
-- M15: TheoDB surface is provisioned by the theodb extension (not raw init-scripts).
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
EOF

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
