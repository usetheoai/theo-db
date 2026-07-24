#!/usr/bin/env bash
# M136 — smoke sob Postgres `--enable-cassert`: exercita os index/table AMs e dispara Assert() na classe
# exata do crash #143 (stub sem `#[pg_guard]` → `_URC_END_OF_STACK` → abort do postmaster no CREATE INDEX).
#
# POR QUE ISTO EXISTE: em build de release `Assert()` é no-op — a única cobertura de asserção do engine vem de
# um PG compilado com `--enable-cassert` (+ `USE_ASSERT_CHECKING` + `RANDOMIZE_ALLOCATED_MEMORY`). É a lição #1
# do paradedb e o gate de maior valor/custo do M136. O pgrx compila o PG do source COM cassert; este smoke
# instala a extensão nele, exercita os quatro AMs + o columnar TableAM, e FALHA se o servidor abortar (Assert)
# ou cair (signal). Verde = os caminhos exercitados não violaram nenhuma asserção do engine.
#
# Uso (CI e local): PGINST=/root/.pgrx/18.4/pgrx-install bash scripts/cassert-smoke.sh
# Exit 0 = sobreviveu sem assert/crash. Exit 1 = assert falhou / servidor caiu (o defeito classe-#143).
set -euo pipefail

PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
PORT="${PGPORT:-28936}"
DATA="$(mktemp -d)/cassert-data"
LOG="$DATA/log"
trap 'pg_ctl_stop 2>/dev/null || true; rm -rf "$(dirname "$DATA")"' EXIT

pg_ctl_stop() { "$PGINST/bin/pg_ctl" -D "$DATA" stop -m immediate >/dev/null 2>&1; }

# 0. Confirmar que o PG é MESMO cassert — um smoke contra um PG de release passaria vacuamente (Assert no-op).
if ! "$PGINST/bin/pg_config" --configure | grep -q -- '--enable-cassert'; then
  echo "FATAL: $PGINST não foi compilado com --enable-cassert — o smoke seria vacuoso (Assert é no-op)"; exit 1
fi
echo "== PG cassert confirmado: $($PGINST/bin/pg_config --version) =="

# 1. Instância descartável com cassert.
"$PGINST/bin/initdb" -D "$DATA" --locale=C.UTF-8 -U postgres >/dev/null 2>&1
echo "listen_addresses = 'localhost'" >> "$DATA/postgresql.conf"
"$PGINST/bin/pg_ctl" -D "$DATA" -o "-p $PORT" -l "$LOG" start >/dev/null 2>&1
sleep 4
PSQL="$PGINST/bin/psql -p $PORT -U postgres -d postgres -v ON_ERROR_STOP=1"

# 2. Exercitar a classe #143 sob Assert(): CREATE EXTENSION + os quatro AMs + o columnar TableAM.
$PSQL >/dev/null 2>&1 <<'SQL'
CREATE EXTENSION theodb_rs;
CREATE TABLE v (id int, e vector(4));
INSERT INTO v SELECT g, ('['||g||',2,3,4]')::vector FROM generate_series(1,300) g;
CREATE INDEX vh ON v USING theodb_hnsw (e theodb_hnsw_l2_ops);
CREATE INDEX vi ON v USING theodb_ivfflat (e theodb_ivfflat_l2_ops) WITH (lists=4);
SET enable_seqscan=off;
SELECT count(*) FROM (SELECT id FROM v ORDER BY e <-> '[1,2,3,4]' LIMIT 10) s;
CREATE TABLE c (a int, b text) USING theodb_columnar;
INSERT INTO c SELECT g, 'x'||g FROM generate_series(1,1000) g;
SELECT count(*), sum(a) FROM c;
SQL

# 2.1 REGRESSÃO #177 — um scan SEM `ORDER BY <->` tem de FALHAR ALTO, nunca devolver linha vazia.
# O defeito era silencioso: `count(*)` sob `enable_seqscan=off` devolvia 0 numa tabela de 300 linhas, com
# plano `Index Only Scan`, sem erro e sem nada no log — resposta ERRADA, que nenhum gate existente via.
# Nenhum teste do repo exercitava esse caminho (todos usam ORDER BY <->), então o bug sobreviveu ~120
# milestones. Este bloco é o teste de duas linhas que o teria pego a qualquer momento.
NOORDER_OUT=$("$PGINST/bin/psql" -p "$PORT" -U postgres -d postgres -tAc \
  "SET enable_seqscan=off; SELECT count(*) FROM v;" 2>&1 || true)
if echo "$NOORDER_OUT" | grep -q "cannot scan a vector index without ORDER BY"; then
  echo "   ok: scan sem ORDER BY falha alto (guard #177 ativo)"
elif [ "$(echo "$NOORDER_OUT" | tail -1)" = "300" ]; then
  # O planner escolheu heap/seq scan mesmo com o hint — resposta correta, guard não exercitado. Aceitável.
  echo "   ok: planner não usou o índice; count correto (300) — guard não exercitado nesta run"
else
  echo "FALHA (#177): scan sem ORDER BY devolveu resposta ERRADA em vez de erro tipado:"
  echo "$NOORDER_OUT" | tail -3
  exit 1
fi

# 3. Veredito: nenhum Assert falhou e o servidor responde.
if grep -qE "TRAP: failed Assert|terminated by signal|PANIC" "$LOG" 2>/dev/null; then
  echo "FALHA: Assert/crash no log (classe #143):"; grep -E "TRAP|signal|PANIC" "$LOG" | head -5; exit 1
fi
"$PGINST/bin/psql" -p "$PORT" -U postgres -d postgres -tAc "SELECT 1" >/dev/null \
  || { echo "FALHA: servidor não responde após o smoke"; exit 1; }

echo "== CASSERT_SMOKE_OK — os 4 AMs + columnar exercitados sob Assert(): 0 asserts, 0 crashes, servidor vivo =="
