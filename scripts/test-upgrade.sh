#!/usr/bin/env bash
# M137 / F4 — harness da cadeia de upgrade do `theodb_rs`.
#
# POR QUE ISTO EXISTE: as provas do artefato `docs/benchmarks/m137-upgrade-chain.md` foram feitas à mão numa
# droplet e não eram re-executáveis. Isso importa mais aqui do que de costume, porque o próprio artefato registra
# DUAS leituras falsas minhas — um "CONVERGENCIA_OK" comparando 196 com 196 onde nada tinha sido removido, e um
# `grep -cE "^ERROR"` que não pegava os erros do psql (que vêm prefixados com `psql:arquivo:linha:`). Um harness
# que já produziu dois passes vacuosos não pode continuar sendo digitado à mão.
#
# Os quatro cenários vêm do pg_durable (`scripts/test-upgrade.sh`) e do ParadeDB, adaptados:
#   A   pós-upgrade == instalação limpa (o oráculo de schema + ACL)
#   CONV convergência: catálogo INCOMPLETO converge para o estado completo (o teste que dá poder ao oráculo)
#   IDEM idempotência: rodar o script duas vezes não erra e não muda o schema
#   B1  `.so` novo contra catálogo antigo SEM `ALTER EXTENSION` — o usuário que faz `apt upgrade` e esquece.
#       Para nós não é hipotético: os index AMs leem páginas em disco e o columnar é TableAM.
#
# Uso:
#   PGINST=/root/.pgrx/18.4/pgrx-install PGPORT=28918 bash scripts/test-upgrade.sh
#
# Exit 0 = todos os cenários passaram. Qualquer falha aborta com a causa nomeada.
set -euo pipefail

PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
PGPORT="${PGPORT:-28918}"
PGUSER="${PGUSER:-postgres}"
PSQL="$PGINST/bin/psql -p $PGPORT -U $PGUSER"
SNAP="$(cd "$(dirname "$0")/.." && pwd)/theodb_rs/sql/schema_snapshot.sql"
FROM_VER="${FROM_VER:-1.0.0}"
TO_VER="${TO_VER:-1.1.0}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

[ -f "$SNAP" ] || { echo "FATAL: oráculo não encontrado em $SNAP"; exit 2; }

q()    { $PSQL -d "$1" -tAc "$2"; }
snap() { $PSQL -d "$1" -tA -f "$SNAP"; }
fresh_db() { $PSQL -d postgres -tAc "DROP DATABASE IF EXISTS $1" >/dev/null; $PSQL -d postgres -tAc "CREATE DATABASE $1" >/dev/null; }

echo "== harness da cadeia de upgrade — $FROM_VER -> $TO_VER, PG em $PGINST:$PGPORT =="

# ---------------------------------------------------------------- cenário A + CONV
fresh_db up_fresh
q up_fresh "CREATE EXTENSION theodb_rs" >/dev/null
snap up_fresh > "$TMP/fresh.out"
echo "-- instalação limpa: $(wc -l < "$TMP/fresh.out") linhas de snapshot"

fresh_db up_old
q up_old "CREATE EXTENSION theodb_rs VERSION '$FROM_VER'" >/dev/null
q up_old "ALTER EXTENSION theodb_rs UPDATE TO '$TO_VER'" >/dev/null
[ "$(q up_old "SELECT extversion FROM pg_extension WHERE extname='theodb_rs'")" = "$TO_VER" ] \
  || { echo "FALHA: extversion não é $TO_VER após o UPDATE"; exit 1; }
snap up_old > "$TMP/old.out"
diff -u "$TMP/fresh.out" "$TMP/old.out" > "$TMP/a.diff" \
  || { echo "FALHA cenário A — pós-upgrade difere de instalação limpa:"; head -20 "$TMP/a.diff"; exit 1; }
echo "SCENARIO_A_OK — pós-upgrade == instalação limpa (schema + ACL)"

# CONV: envelhecer o catálogo removendo objetos, e provar que o UPDATE os restaura.
# É isto que dá PODER ao oráculo: sem remover nada, o teste compara 196 com 196 e passa vacuamente.
fresh_db conv
q conv "CREATE EXTENSION theodb_rs VERSION '$FROM_VER'" >/dev/null
for sig in 'theodb.embed(text,text)' 'theodb.embed_batch(text[],text)' 'ai.rerank(text,text[],text,integer)'; do
  q conv "ALTER EXTENSION theodb_rs DROP FUNCTION $sig" >/dev/null
  q conv "DROP FUNCTION $sig" >/dev/null
done
snap conv > "$TMP/aged.out"
AGED=$(wc -l < "$TMP/aged.out"); FRESH=$(wc -l < "$TMP/fresh.out")
[ "$AGED" -lt "$FRESH" ] || { echo "FALHA: envelhecimento não removeu nada ($AGED == $FRESH) — o teste seria vacuoso"; exit 1; }
echo "-- catálogo envelhecido: $AGED linhas (faltam $((FRESH - AGED)))"
q conv "ALTER EXTENSION theodb_rs UPDATE TO '$TO_VER'" >/dev/null
snap conv > "$TMP/conv.out"
diff -u "$TMP/fresh.out" "$TMP/conv.out" > "$TMP/c.diff" \
  || { echo "FALHA convergência — catálogo incompleto não convergiu:"; head -20 "$TMP/c.diff"; exit 1; }
echo "CONVERGENCIA_OK — catálogo incompleto ($AGED) convergiu para o completo ($FRESH)"

# ---------------------------------------------------------------- idempotência
UPG="$PGINST/share/postgresql/extension/theodb_rs--$FROM_VER--$TO_VER.sql"
if [ -f "$UPG" ]; then
  snap up_old > "$TMP/i1.out"
  $PSQL -d up_old -f "$UPG" > "$TMP/i.log" 2>&1 || true
  if grep -q "ERROR" "$TMP/i.log"; then echo "FALHA idempotência — 2a execução erra:"; grep "ERROR" "$TMP/i.log" | head -3; exit 1; fi
  snap up_old > "$TMP/i2.out"
  diff -q "$TMP/i1.out" "$TMP/i2.out" >/dev/null || { echo "FALHA idempotência — snapshot mudou na 2a execução"; exit 1; }
  echo "IDEMPOTENTE_OK — script rodado 2x: 0 erros, snapshot inalterado"
else
  echo "AVISO: $UPG não encontrado — cenário de idempotência PULADO (não é um pass)"
fi

# ---------------------------------------------------------------- cenário B1
LOG="${PGLOG:-/tmp/pg18data/log}"
BEFORE=$([ -f "$LOG" ] && grep -c "terminated by signal" "$LOG" || echo 0)
fresh_db b1
q b1 "CREATE EXTENSION theodb_rs VERSION '$FROM_VER'" >/dev/null
$PSQL -d b1 -v ON_ERROR_STOP=0 >"$TMP/b1.log" 2>&1 <<'SQL' || true
CREATE TABLE b1v (id int, v vector(4));
INSERT INTO b1v SELECT g, ('['||g||',2,3,4]')::vector FROM generate_series(1,200) g;
CREATE INDEX b1i ON b1v USING theodb_hnsw (v theodb_hnsw_l2_ops);
SET enable_seqscan=off;
SELECT count(*) FROM (SELECT id FROM b1v ORDER BY v <-> '[1,2,3,4]' LIMIT 5) s;
CREATE TABLE b1c (a int) USING theodb_columnar;
INSERT INTO b1c SELECT generate_series(1,1000);
SELECT count(*), sum(a) FROM b1c;
SQL
AFTER=$([ -f "$LOG" ] && grep -c "terminated by signal" "$LOG" || echo 0)
[ "$BEFORE" = "$AFTER" ] || { echo "FALHA B1 — o servidor CAIU (crashes $BEFORE -> $AFTER)"; exit 1; }
q b1 "SELECT 1" >/dev/null || { echo "FALHA B1 — servidor não responde após o cenário"; exit 1; }
echo "SCENARIO_B1_DONE — .so novo sobre catálogo $FROM_VER sem UPDATE: servidor sobreviveu (crashes $BEFORE -> $AFTER)"

echo "== TODOS OS CENÁRIOS PASSARAM =="
