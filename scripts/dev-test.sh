#!/usr/bin/env bash
# dev-test.sh — roda os gates de Rust no contêiner com o toolchain pinado, RÁPIDO.
#
# Uso:
#   bash scripts/dev-test.sh                 # a suíte inteira (~6 min) — o gate de entrega
#   bash scripts/dev-test.sh lexical         # só os testes cujo nome casa `lexical` (segundos)
#   bash scripts/dev-test.sh --lint          # cargo fmt --check + clippy com o baseline
#
# POR QUE ESTE ARQUIVO EXISTE (B-054), e são duas razões independentes.
#
# 1. A RECEITA ESTAVA COPIADA EM OITO CABEÇALHOS DE WORKFLOW. Oito cópias de um procedimento que
#    ninguém pode executar a partir do comentário sem colar à mão. Um lugar, um comando.
#
# 2. A RECEITA ERA LENTA POR UM MOTIVO EVITÁVEL. Ela fazia `cp -r /src/. /tmp/b/` para levar o
#    código para dentro do contêiner, e **`cp -r` não preserva mtime** — carimba a hora atual em
#    cada arquivo. O cargo decide o que recompilar por mtime, então do ponto de vista dele o
#    DataFusion inteiro é novo a cada corrida.
#
#    MEDIDO, não estimado:
#
#      | forma                        | crates recompilados | tempo    |
#      |------------------------------|---------------------|----------|
#      | `cp -r` (a receita antiga)   | 363                 | ~8 min   |
#      | `cp -a` (preserva mtime)     | 109                 | —        |
#      | montagem direta (esta)       | **0**               | 53s/75s  |
#
#    As duas primeiras linhas foram medidas em 2026-08-13 pelo ciclo que sofreu com isso; a
#    terceira em 2026-08-20, em duas corridas consecutivas sem mudança de código (o critério que o
#    DoD do B-054 exige). Montagem direta não copia nada, então não há mtime a perder.
#
# 3. O TERCEIRO CUSTO ERA DE MÉTODO, e é o maior. Rodar os 481 `pg_test` para validar uma mudança
#    em UM módulo custa ~6 min de teste que não olham o que você mexeu. O filtro posicional existe
#    para isso e é o default do fluxo de implementação; a suíte completa é o gate de ENTREGA, não
#    o de iteração. Um ciclo lento não é só desconfortável — ele empurra para usar gates mais
#    baratos e errados.
#
# O contêiner monta `theodb_rs/` COM ESCRITA porque o cargo precisa escrever em `target/`. É o
# mesmo `target/` da máquina (13 GB), e é justamente por isso que a segunda corrida custa zero.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGEM="theodb-builder"

if ! docker image inspect "$IMAGEM" >/dev/null 2>&1; then
    echo "==> imagem $IMAGEM ausente; construindo o estágio builder (demora na primeira vez)"
    docker build --target theodb-rs-builder -t "$IMAGEM" "$RAIZ"
fi

PREPARO='
  apt-get update -qq >/dev/null 2>&1
  apt-get install -y -qq sudo >/dev/null 2>&1
  useradd -m postgres 2>/dev/null || true
  mkdir -p /pgdata && chown postgres /pgdata
  chmod -R a+rwX /src
  cd /src
  export CARGO_PGRX_TEST_RUNAS=postgres
  export CARGO_PGRX_TEST_PGDATA=/pgdata
  export RUSTFLAGS="-Clink-arg=-Wl,--unresolved-symbols=ignore-all"
'

if [ "${1:-}" = "--lint" ]; then
    # `.clippy_args` é lido de dentro, do mesmo arquivo que o CI lê — um baseline, não dois.
    COMANDO='rustup component add clippy rustfmt --toolchain 1.97.0-x86_64-unknown-linux-gnu >/dev/null 2>&1
             cargo fmt -- --check && cargo clippy --features pg18 --no-deps -- $(grep -v "^#" .clippy_args | tr "\n" " ")'
else
    FILTRO="${1:-}"
    COMANDO="cargo pgrx test pg18 ${FILTRO}"
fi

exec docker run --rm -v "$RAIZ/theodb_rs:/src" "$IMAGEM" bash -c "${PREPARO}${COMANDO}"
