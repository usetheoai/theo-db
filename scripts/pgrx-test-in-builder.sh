#!/usr/bin/env bash
# Roda cargo (build) OU cargo pgrx test do theodb_rs DENTRO do builder image (toolchain pgrx+pg17),
# source mountado, target cacheado. `cargo pgrx test` precisa de non-root (initdb recusa root) → cria
# um usuário `builder` (uid 1001, alinhado ao host) com o toolchain acessível e roda como ele.
# Uso: scripts/pgrx-test-in-builder.sh [build --lib | pgrx test pg17 <filtro>]
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
ARGS="${*:-build --lib}"
docker run --rm \
  -v "$REPO/theodb_rs":/build \
  -v /tmp/m51-target:/build/target \
  -w /build theodb-builder:m51-test \
  bash -lc "
    set -e
    # perms: builder (uid 1001) precisa executar o toolchain de root e escrever no target
    id builder >/dev/null 2>&1 || useradd -m -u 1001 builder
    chmod a+rx /root 2>/dev/null || true
    chmod -R a+rX /root/.cargo /root/.rustup 2>/dev/null || true
    mkdir -p /home/builder/.pgrx && cp -rf /root/.pgrx/* /home/builder/.pgrx/ 2>/dev/null || true
    chown -R builder /home/builder /build/target 2>/dev/null || true
    # pgrx test faz 'cargo pgrx install' nos dirs de extensão do pg17 (root-owned) → torná-los graváveis
    PKGLIB=\$(/usr/bin/pg_config --pkglibdir); EXT=\$(/usr/bin/pg_config --sharedir)/extension
    chmod -R a+w \"\$PKGLIB\" \"\$EXT\" 2>/dev/null || true
    su builder -c 'export PATH=/root/.cargo/bin:\$PATH; export PGRX_HOME=/home/builder/.pgrx; export CARGO_HOME=/root/.cargo; cd /build && cargo $ARGS' 2>&1 | tail -45
  "
