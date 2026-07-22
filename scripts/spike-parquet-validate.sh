#!/usr/bin/env bash
# SPIKE Parquet (Fase 4) — valida o leitor Parquet own-code (theodb.read_parquet_agg_spike) contra um PG18 real,
# comparando com o pg_duckdb.read_parquet (paridade) e medindo o custo de tamanho do .so.
#
# Pré: `cargo pgrx install --release --features "pg18 spike-parquet" --pg-config ~/.pgrx/18.4/pgrx-install/bin/pg_config`
# já rodou (instala a extensão theodb_rs com o spike no PG18 do pgrx). Roda no e2e-runner (padrão M140).
#
# Gates (falha exit≠0): (1) own-code lê o Parquet e produz o agregado canônico do M62; (2) bate o GROUND TRUTH
# conhecido (a|2|15, b|1|5); (3) bate byte-a-byte o pg_duckdb (paridade). Reporta o delta de tamanho do .so.
set -euo pipefail

PGINST="$HOME/.pgrx/18.4/pgrx-install"
DATA=/tmp/spike-pqdata
PORT=6599
LOG=/tmp/spike-pq.log
PARQUET=/srv/spike/spike.parquet
fail() { echo "FAIL: $1" >&2; exit 1; }

# ── (A) gera o Parquet via o pg_duckdb da imagem htap + captura o baseline de paridade ──
echo "== gerando parquet + baseline pg_duckdb (via theodb:m142-htap) =="
docker rm -f pqgen >/dev/null 2>&1 || true
docker run -d --name pqgen -e POSTGRES_PASSWORD=postgres theodb:m142-htap >/dev/null
for _ in $(seq 1 30); do [ "$(docker inspect -f '{{.State.Health.Status}}' pqgen 2>/dev/null)" = healthy ] && break; sleep 2; done
docker exec pqgen psql -U postgres -q -c \
  "CREATE TABLE s(category text, amount double precision); INSERT INTO s VALUES ('a',10),('a',20),('b',5)"
docker exec pqgen psql -U postgres -q -c \
  "COPY (SELECT * FROM s) TO '/tmp/spike.parquet' (FORMAT parquet)"
DUCK_OUT="$(docker exec pqgen psql -U postgres -tAc \
  "SELECT * FROM duckdb.query(\$D\$ SELECT category, count(*) c, round(avg(amount),4) a FROM read_parquet('/tmp/spike.parquet') GROUP BY category ORDER BY category \$D\$)")"
mkdir -p /srv/spike && docker cp pqgen:/tmp/spike.parquet "$PARQUET" && chmod -R 755 /srv/spike
docker rm -f pqgen >/dev/null 2>&1 || true
echo "pg_duckdb baseline:"; echo "$DUCK_OUT"

# ── (B) sobe um PG18 real (pgrx install) como pgtest e roda o leitor own-code ──
echo "== subindo PG18 (pgrx) + lendo o Parquet own-code =="
id pgtest >/dev/null 2>&1 || useradd -m pgtest
rm -rf "$DATA"; mkdir -p "$DATA"; chown pgtest "$DATA"; chmod o+x "$HOME" 2>/dev/null || true
runuser -u pgtest -- "$PGINST/bin/initdb" -D "$DATA" -U postgres --locale=C.UTF-8 >/dev/null 2>&1
runuser -u pgtest -- bash -c "$PGINST/bin/pg_ctl -D $DATA -o '-p $PORT' -l $LOG start" >/dev/null 2>&1
sleep 2
PSQL="$PGINST/bin/psql -h 127.0.0.1 -p $PORT -U postgres -d postgres -tA"
trap 'runuser -u pgtest -- $PGINST/bin/pg_ctl -D $DATA stop -m immediate >/dev/null 2>&1 || true' EXIT
$PSQL -c "CREATE EXTENSION theodb_rs" >/dev/null 2>&1 || fail "CREATE EXTENSION theodb_rs (build com spike-parquet?)"

FQSCHEMA="$($PSQL -c "SELECT n.nspname FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE p.proname='read_parquet_agg_spike'" | tr -d ' ')"
[ -n "$FQSCHEMA" ] || fail "função read_parquet_agg_spike não existe (build com spike-parquet instalou?)"
echo "função own-code em: ${FQSCHEMA}.read_parquet_agg_spike"
OWN_OUT="$($PSQL -c "SELECT category||'|'||c||'|'||a FROM ${FQSCHEMA}.read_parquet_agg_spike('$PARQUET') ORDER BY category" 2>&1)" \
  || fail "read_parquet_agg_spike falhou: $OWN_OUT"
echo "own-code output:"; echo "$OWN_OUT"

# ── (C) gates ──
echo "== gates =="
echo "$OWN_OUT" | grep -q '^a|2|15$' || fail "own-code linha 'a' != a|2|15 (got: $OWN_OUT)"
echo "$OWN_OUT" | grep -q '^b|1|5$'  || fail "own-code linha 'b' != b|1|5 (got: $OWN_OUT)"
echo "GROUND_TRUTH_OK (a|2|15, b|1|5)"
# paridade byte-a-byte vs pg_duckdb (normaliza espaços)
DUCK_NORM="$(echo "$DUCK_OUT" | tr -d ' ' | sort)"
OWN_NORM="$(echo "$OWN_OUT" | tr -d ' ' | sort)"
[ "$DUCK_NORM" = "$OWN_NORM" ] || fail "paridade own-code vs pg_duckdb DIVERGE (duck='$DUCK_NORM' own='$OWN_NORM')"
echo "PARITY_OK (own-code == pg_duckdb, byte-a-byte)"

# ── (D) custo de tamanho do .so (baseline vs spike) ──
BASE_SO="$(docker run --rm --entrypoint bash theodb:m142-default -lc 'stat -c%s /usr/lib/postgresql/18/lib/theodb_rs.so' 2>/dev/null)"
SPIKE_SO="$(stat -c%s "$PGINST/lib/postgresql/theodb_rs.so" 2>/dev/null || echo 0)"
echo "== tamanho .so =="
echo "baseline (sem spike-parquet): $((BASE_SO/1024/1024)) MB ($BASE_SO bytes)"
echo "spike    (com spike-parquet): $((SPIKE_SO/1024/1024)) MB ($SPIKE_SO bytes)"
echo "delta do leitor Parquet own-code: $(( (SPIKE_SO-BASE_SO)/1024/1024 )) MB  (vs bundle DuckDB = 118 MB)"

echo "SPIKE_PARQUET_OK"
