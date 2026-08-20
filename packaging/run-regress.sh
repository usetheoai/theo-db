#!/usr/bin/env bash
# Start a throwaway TheoDB cluster and run the upstream PG18 regression suite against it (pg_regress
# directly, so we control bindir/outputdir/dlpath). Runs as the postgres user. Exits 0 only if ALL pass.
set -uo pipefail
export PGDATA=/tmp/pgdata PGHOST=/tmp PGPORT=5432
# Derived from pg_config, never pinned. A hardcoded major is what broke this gate: when the engine
# moved 17 -> 18 the path /usr/lib/postgresql/17/bin simply stopped existing, so the command
# substitution below yielded the EMPTY string and the version guard reported the baffling
# "engine is not 18.4 (got: )" — a missing binary disguised as a version mismatch. Fail loud and
# specific instead (Unbreakable Rule 8). Drift is also guarded statically by
# benchmarks/tests/test_packaging_pg_major.py.
BIN="$(pg_config --bindir 2>/dev/null || true)"
if [ -z "$BIN" ] || [ ! -x "$BIN/postgres" ]; then
  echo "ERROR: no postgres binary via 'pg_config --bindir' (got: '${BIN:-<empty>}')" >&2
  exit 2
fi
REG=/src/src/test/regress
OUT=/tmp/regress_out

# Prove the engine-under-test matches the SOURCE TAG this image cloned. Comparado contra `$PG_TAG`
# (exportado pelo Dockerfile) e não contra uma constante escrita aqui: a versão de patch é decisão do
# upstream, e duas constantes independentes divergem no dia em que a imagem base avança — foi o que
# aconteceu em 2026-08-20 (`engine is not 18.4 (got 18.6)`), com o gate reprovando por desatualização
# própria e não por defeito do produto.
EXPECTED="${PG_TAG:?PG_TAG ausente — a imagem de regressão deve exportá-lo}"
EXPECTED="${EXPECTED#REL_}"; EXPECTED="${EXPECTED//_/.}"
VER="$("$BIN/postgres" --version)"
echo "engine under test: $VER   (source tag: $PG_TAG -> esperado $EXPECTED)"
echo "$VER" | grep -q "$EXPECTED" || {
  echo "ERROR: engine is not $EXPECTED (got: $VER) — o fonte da suíte e o engine divergem" >&2
  echo "Se a imagem base subiu de patch, derive PG_TAG do engine no caller em vez de editar aqui." >&2
  exit 2; }

rm -rf "$PGDATA"; mkdir -p "$PGDATA" "$OUT"
"$BIN/initdb" -D "$PGDATA" -U postgres -E UTF8 >/tmp/initdb.log 2>&1 || { tail -20 /tmp/initdb.log; exit 2; }
"$BIN/pg_ctl" -D "$PGDATA" -w -t 60 \
  -o "-c unix_socket_directories=/tmp -p 5432 -c listen_addresses=''" start >/tmp/pgstart.log 2>&1 \
  || { tail -20 /tmp/pgstart.log; exit 2; }

set -o pipefail
# --bindir='' => use psql from PATH (the TheoDB binaries); --dlpath/--inputdir => the built test tree.
"$REG/pg_regress" \
  --inputdir="$REG" --outputdir="$OUT" --bindir='' --dlpath="$REG" \
  --host=/tmp --port=5432 --user=postgres --dbname=regression \
  --schedule="$REG/parallel_schedule" --max-concurrent-tests=20 2>&1 | tee /tmp/regress.log
rc=${PIPESTATUS[0]}

echo "======================================================"
echo "  UPSTREAM PG18.4 REGRESSION SUITE — TheoDB distro"
echo "======================================================"
grep -E "All [0-9]+ tests passed|[0-9]+ of [0-9]+ tests failed" /tmp/regress.log | tail -3
if [ "$rc" -ne 0 ]; then
  echo "--- failed test diffs (head) ---"
  [ -f "$OUT/regression.diffs" ] && head -50 "$OUT/regression.diffs"
fi
"$BIN/pg_ctl" -D "$PGDATA" stop -m immediate >/dev/null 2>&1 || true
exit "$rc"
