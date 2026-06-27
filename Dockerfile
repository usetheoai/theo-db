# syntax=docker/dockerfile:1
# TheoDB image — PostgreSQL 17 + pgvector (M0) + pgvectorscale StreamingDiskANN (M2 DoD-2).
# Multi-stage: the `scale-builder` stage compiles the Rust/pgrx extension; the runtime stage copies
# ONLY the artifacts (no Rust toolchain shipped — runtime stays ~445 MB).

# ---- Stage 1: build pgvectorscale (Rust + cargo-pgrx) ----
FROM postgres:17-bookworm AS scale-builder
ARG PG_MAJOR=17
ARG PGVECTORSCALE_REF=57c88b7b4fe40a2afa20b195f60047a983279c19
ARG PGRX_VERSION=0.16.1
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential postgresql-server-dev-$PG_MAJOR libssl-dev pkg-config clang git curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN git clone https://github.com/timescale/pgvectorscale /tmp/pgvectorscale && \
    cd /tmp/pgvectorscale && git checkout $PGVECTORSCALE_REF
RUN cargo install --locked cargo-pgrx --version $PGRX_VERSION
RUN cd /tmp/pgvectorscale/pgvectorscale && \
    cargo pgrx init --pg$PG_MAJOR "$(which pg_config)" && \
    cargo pgrx install --release --features pg$PG_MAJOR

# ---- Stage 2: runtime (postgres:17 + pgvector + pgvectorscale) ----
FROM postgres:17-bookworm@sha256:17b6c778de50f4bb9a878c36e736110fbcd9b7020377d6fdfdf20f7c0347e40a
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

HEALTHCHECK --interval=5s --timeout=5s --start-period=10s --retries=5 \
  CMD pg_isready -h localhost -p 5432 -U postgres -q
