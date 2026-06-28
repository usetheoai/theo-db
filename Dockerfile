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

# theodb.embed() — created on fresh DB init (idempotent script).
COPY sql/30-theodb-embed.sql /docker-entrypoint-initdb.d/30-theodb-embed.sql

# ai.hybrid_search_rrf() — M7-S1 hybrid search (FTS + vector + RRF), created on fresh DB init (idempotent).
COPY sql/40-theodb-hybrid.sql /docker-entrypoint-initdb.d/40-theodb-hybrid.sql

# ai.generate/if/analyze_sentiment/summarize/rank — M7-S3 generative-AI functions (idempotent).
COPY sql/50-theodb-ai.sql /docker-entrypoint-initdb.d/50-theodb-ai.sql

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
