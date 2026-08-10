#!/bin/bash
# M169 T3.2 — driver da medição de pico do GROUP BY. Existe porque a ORDEM é uma pré-condição, não uma
# preferência, e escrevê-la em mensagem de commit não a faz acontecer.
#
# Por que restart: `admit_trace_enabled()` resolve `THEODB_ADMIT_TRACE` num `OnceLock` POR BACKEND
# (`columnar_agg.rs:221-224`). Backends herdam o ambiente do postmaster, então a variável só entra por um
# restart do postmaster — um `SET` de sessão ou um `export` no cliente não alcançam nada.
#
# Por que restaurar depois: o T1.2 rodou SEM o trace. Deixar o trace ligado para o T4.1 mudaria uma variável
# entre as duas corridas, e o ADR-3 exige que só o binário mude. O `trap` garante a restauração mesmo se a
# medição falhar no meio.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PGDATA="${PGDATA:-/srv/m169data}"
PGBIN="${PGBIN:-/opt/pg18/bin}"
PGOSUSER="${PGOSUSER:-pgtest}"
export PGPORT="${PGPORT:-5432}" PGUSER="${PGUSER:-postgres}" PGDATABASE="${PGDATABASE:-postgres}"
OUT="${OUT:-/root/m169_t32_peak.log}"

as_pg() { sudo -u "$PGOSUSER" env LD_LIBRARY_PATH=/opt/pg18/lib "$@"; }
psql_c() { as_pg "$PGBIN/psql" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" "$@"; }
pg_restart() { # $1 = valor de THEODB_ADMIT_TRACE ("" = sem a variável)
  if [ -n "$1" ]; then
    sudo -u "$PGOSUSER" env LD_LIBRARY_PATH=/opt/pg18/lib THEODB_ADMIT_TRACE="$1" \
      "$PGBIN/pg_ctl" -D "$PGDATA" -m fast -w restart -l "$PGDATA/pg.log" >/dev/null
  else
    as_pg "$PGBIN/pg_ctl" -D "$PGDATA" -m fast -w restart -l "$PGDATA/pg.log" >/dev/null
  fi
}

echo "=== guarda: a box está ociosa? (medir sob carga atribui ao código o efeito da concorrência) ==="
# `state <> 'idle'` e não apenas `client backend`: um psql ÓCIOSO esquecido por um ssh que caiu não
# consome CPU nem I/O, mas fazia esta guarda abortar — visto na box em 2026-07-31, um órfão com
# query_start de 91 s atrás. `idle in transaction` CONTINUA contando: esse segura locks e snapshot, e é
# exatamente o estado que contamina uma medição.
BACKENDS=$(psql_c -Atc "select count(*) from pg_stat_activity where backend_type='client backend' and state <> 'idle' and pid<>pg_backend_pid();")
LOAD=$(cut -d' ' -f1 /proc/loadavg)
echo "  backends=$BACKENDS loadavg=$LOAD"
if [ "${BACKENDS:-1}" -gt 0 ]; then
  echo "  ABORTA: há consulta rodando — o pico medido seria de dois trabalhos, não de um." >&2; exit 1
fi

echo "=== restart COM THEODB_ADMIT_TRACE=1 ==="
# A restauração é registrada ANTES do primeiro restart: se a medição morrer no meio, o servidor não fica com o
# trace ligado silenciosamente contaminando a corrida seguinte.
trap 'echo "=== restaurando: restart SEM o trace ==="; pg_restart ""' EXIT
pg_restart 1

echo "=== medição ==="
# `-f -` com REDIRECIONAMENTO, não `-f "$HERE/…"`. Quem abre o arquivo passa a ser o shell (que roda como root e
# atravessa `/root`), e o psql — que roda como $PGOSUSER — só lê o stdin. Isso remove a permissão da equação.
#
# MEDIDO 2026-07-31, e a sutileza vale registrar: o mesmo arquivo, mesmo usuário, funciona com
# `cd /root/theo-db && psql -f benchmarks/x.sql` e FALHA com `psql -f /root/theo-db/benchmarks/x.sql`. A checagem
# do bit `x` vale para os componentes resolvidos NA HORA do open; um caminho relativo parte do CWD já aberto e
# herdado, então não re-atravessa `/root`. Foi por isso que o RED do T2.1 passou e este driver não.
psql_c -f - < "$HERE/m169_groupby_peak.sql" > "$OUT" 2>&1
SQL_RC=$?

PEAKS=$(grep -c "peak_reserved=" "$OUT")
echo "  linhas de pico capturadas: $PEAKS de 5 esperadas   (sql_rc=$SQL_RC)"
grep -oE "theodb_stream_pool: peak_reserved=[0-9]+ reserved_at_end=[0-9]+ pool_limit=[0-9]+" "$OUT" | sed 's/^/  /'

if [ "$PEAKS" -eq 0 ]; then
  # ZERO linhas NÃO é "o pico foi baixo" — é "o instrumento não estava ligado". Tratar como sucesso seria
  # publicar ausência de dado como dado.
  echo "  FALHA: nenhuma linha de pico. O trace não chegou ao postmaster, ou o caminho não roteou." >&2
  exit 2
fi
[ "$SQL_RC" -ne 0 ] && { echo "  FALHA: o SQL saiu com $SQL_RC (veja $OUT)" >&2; exit "$SQL_RC"; }
echo "=== ok: $PEAKS medições de pico em $OUT ==="
exit 0
