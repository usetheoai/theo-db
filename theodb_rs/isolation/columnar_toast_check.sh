#!/usr/bin/env bash
# #190 — teste de regressão: INSERT no `theodb_columnar` com valores TOAST.
#
# O bug: o segundo flush de stripe chama `pg_detoast_datum_copy` sobre um ponteiro TOAST guardado no
# buffer, e no caminho de pré-commit (`columnar.rs:206`) NÃO há snapshot ativo →
# `cannot fetch toast data without an active snapshot`. Consequência: nenhuma carga real (com texto
# acima de ~2 KB) entra no colunar.
#
# Este harness é o RED do plano `columnar-toast-materialize`. Cobre, além do repro:
#   EC-4  valores próximos ao limiar de TOAST (1.900/2.000/2.100 bytes). Com `repeat()` esses valores
#         COMPRIMEM e ficam inline/comprimidos-inline — o estado EXTERNO (o que o #190 quebrava) é
#         coberto pelo `src` fixture (STORAGE EXTERNAL + non-vacuity gate), não por esta seção.
#   EC-5  tipos de tamanho fixo trafegam intactos (int, bigint, uuid, char)
#   EC-6  os dois lotes em transações DIFERENTES — o caminho de pré-commit só roda no COMMIT,
#         e é justamente ele que não tem snapshot; um teste sem commit entre lotes não o exercita
#   RET   INSERT ... RETURNING de coluna TOAST (#190 v2): prova que `accumulate_row` NÃO deixa o slot
#         do executor apontando para memória liberada — o use-after-free que o /review pegou. O md5 dos
#         valores RETORNADOS pelo INSERT tem de bater com a origem; com o slot corrompido, difere/crasha.
#
# A asserção de md5 é a defesa principal contra EC-2/EC-3 (corrupção silenciosa): um fix que grave
# lixo passaria num teste que só conta linhas.
#
# Uso: columnar_toast_check.sh   — imprime TOAST_CHECK_OK / TOAST_CHECK_FAIL <motivo>.
# Exit: 0 = OK, 1 = regressão, 2 = falha de infraestrutura.
set -uo pipefail
PGINST="${PGINST:-$HOME/.pgrx/18.4/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=$(mktemp -d /tmp/toastchk.XXXXXX); PORT="${PORT:-$(( 47000 + RANDOM % 9000 ))}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT

fail() { echo "TOAST_CHECK_FAIL $*"; exit 1; }

initdb -D "$DATA" -U theo >/dev/null 2>&1 || { echo "TOAST_CHECK_FAIL initdb"; exit 2; }
{ echo "port=$PORT"; echo "shared_preload_libraries='theodb_rs'"; echo "autovacuum=off"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null 2>&1 || { echo "TOAST_CHECK_FAIL start"; exit 2; }

psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U theo -d postgres -c "CREATE EXTENSION theodb_rs CASCADE;" \
  >/dev/null 2>&1 || { echo "TOAST_CHECK_FAIL create_extension"; exit 2; }

q() { psql -X -q -p "$PORT" -U theo -d postgres -tAc "$1" 2>&1; }
run_file() { psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U theo -d postgres -f "$1" 2>&1; }

# ---------------------------------------------------------------- fixture
# `big` gera ~4 KB de bytes ALEATÓRIOS por linha, com STORAGE EXTERNAL. Ambos são necessários: o TOAST
# comprime ANTES de externalizar, então um valor repetitivo (ex.: repeat(md5(..),100)) ficaria inline e o
# teste passaria sem nunca tocar o caminho sob teste — foi exatamente o que aconteceu na 1ª versão deste
# harness, descoberto pelo teste de não-vacuidade.
cat > "$DATA/fixture.sql" <<'SQL'
CREATE TABLE src (
    id     int,
    n_int  int,
    n_big  bigint,
    u      uuid,
    c10    char(10),
    big    text
);
-- STORAGE EXTERNAL desliga a compressão: sem isso o TOAST comprime primeiro e um valor repetitivo
-- fica INLINE, jamais alcançando a toast table — o caminho que exige snapshot nunca seria exercitado.
ALTER TABLE src ALTER COLUMN big SET STORAGE EXTERNAL;
-- ~4 KB por linha. Com STORAGE EXTERNAL a compressão está desligada, então o valor vai INTEIRO para a
-- toast table (só built-ins: `gen_random_bytes` exigiria pgcrypto). O md5 por linha mantém cada valor
-- distinto, para que o md5 agregado detecte troca de conteúdo.
INSERT INTO src
SELECT g,
       g * 7,
       g::bigint * 1000000007,
       md5(g::text)::uuid,
       lpad(g::text, 10, '0'),
       repeat(md5(g::text), 128)               -- 32 * 128 = 4096 bytes → TOAST EXTERNO (sem compressão)
FROM generate_series(1, 10000) g;
SQL
run_file "$DATA/fixture.sql" >/dev/null 2>&1 || { echo "TOAST_CHECK_FAIL fixture"; exit 2; }

# Gate de não-vacuidade: se nada foi externalizado, o teste não exercita o caminho sob teste e
# passaria mesmo com o defeito presente — falhar alto é melhor que um verde vazio.
EXT=$(q "SELECT count(*) FROM src WHERE pg_column_compression(big) IS NULL AND octet_length(big) > 2000;")
[ "${EXT:-0}" -gt 0 ] || { echo "TOAST_CHECK_FAIL fixture nao gerou TOAST externo (teste seria vacuo)"; exit 2; }

SRC_MD5=$(q "SELECT md5(string_agg(big, '|' ORDER BY id)) FROM src;")
SRC_FIXED=$(q "SELECT md5(string_agg(n_int::text||n_big::text||u::text||c10, '|' ORDER BY id)) FROM src;")
[ -n "$SRC_MD5" ] || { echo "TOAST_CHECK_FAIL fixture_md5_vazio"; exit 2; }

# ---------------------------------------------------------------- (1) repro do #190: 2 lotes, MESMA txn
cat > "$DATA/same_txn.sql" <<'SQL'
CREATE TABLE t_same (LIKE src) USING theodb_columnar;
BEGIN;
INSERT INTO t_same SELECT * FROM src WHERE id <= 5000;
INSERT INTO t_same SELECT * FROM src WHERE id > 5000;
COMMIT;
SQL
OUT=$(run_file "$DATA/same_txn.sql")
echo "$OUT" | grep -qi "cannot fetch toast data" && fail "mesma_txn: $(echo "$OUT" | grep -i 'cannot fetch' | head -1)"
CNT=$(q "SELECT count(*) FROM t_same;")
[ "$CNT" = "10000" ] || fail "mesma_txn count=$CNT (esperado 10000)"

# ---------------------------------------------------------------- (2) EC-6: lotes em txns DIFERENTES
# O caminho sem snapshot é o callback de pré-commit; ele só roda no COMMIT. Sem este cenário, o teste
# pode passar sem tocar o código que originou o defeito.
cat > "$DATA/diff_txn.sql" <<'SQL'
CREATE TABLE t_diff (LIKE src) USING theodb_columnar;
BEGIN; INSERT INTO t_diff SELECT * FROM src WHERE id <= 5000; COMMIT;
BEGIN; INSERT INTO t_diff SELECT * FROM src WHERE id > 5000;  COMMIT;
SQL
OUT=$(run_file "$DATA/diff_txn.sql")
echo "$OUT" | grep -qi "cannot fetch toast data" && fail "txns_distintas (EC-6): $(echo "$OUT" | grep -i 'cannot fetch' | head -1)"
CNT=$(q "SELECT count(*) FROM t_diff;")
[ "$CNT" = "10000" ] || fail "txns_distintas count=$CNT (esperado 10000)"

# ---------------------------------------------------------------- (3) valores preservados (EC-2/EC-3)
# Um fix que grave lixo passa nas asserções de contagem acima; só o md5 pega corrupção silenciosa.
GOT_MD5=$(q "SELECT md5(string_agg(big, '|' ORDER BY id)) FROM t_diff;")
[ "$GOT_MD5" = "$SRC_MD5" ] || fail "md5 do conteudo TOAST difere da origem (corrupcao silenciosa)"

# ---------------------------------------------------------------- (4) EC-5: tipos de tamanho fixo intactos
GOT_FIXED=$(q "SELECT md5(string_agg(n_int::text||n_big::text||u::text||c10, '|' ORDER BY id)) FROM t_diff;")
[ "$GOT_FIXED" = "$SRC_FIXED" ] || fail "EC-5: tipos de tamanho fixo alterados (int/bigint/uuid/char)"

# ---------------------------------------------------------------- (5) EC-4: valores no limiar de TOAST
# 1.900/2.000/2.100 bytes com `repeat()` são altamente compressíveis → ficam inline/comprimidos-inline
# (NÃO externalizam). Esta seção valida que valores de tamanhos variados ao redor do limiar de ~2 KB
# entram sem erro; o caminho EXTERNO (o que o #190 quebrava) é coberto pelo `src` fixture acima, com o
# gate de não-vacuidade que garante que houve TOAST externo de fato.
cat > "$DATA/threshold.sql" <<'SQL'
CREATE TABLE t_thr (id int, v text) USING theodb_columnar;
BEGIN;
INSERT INTO t_thr SELECT g, repeat('a', 1900) FROM generate_series(1,200) g;
INSERT INTO t_thr SELECT 1000+g, repeat('b', 2000) FROM generate_series(1,200) g;
INSERT INTO t_thr SELECT 2000+g, repeat('c', 2100) FROM generate_series(1,200) g;
COMMIT;
SQL
OUT=$(run_file "$DATA/threshold.sql")
echo "$OUT" | grep -qi "cannot fetch toast data" && fail "EC-4 limiar: $(echo "$OUT" | grep -i 'cannot fetch' | head -1)"
THR=$(q "SELECT count(*)||':'||sum(length(v)) FROM t_thr;")
[ "$THR" = "600:1200000" ] || fail "EC-4 limiar: $THR (esperado 600:1200000)"

# ---------------------------------------------------------------- (6) RET: INSERT ... RETURNING de TOAST
# Regressão do use-after-free que o /review pegou (#190 v2): `accumulate_row` costumava sobrescrever
# `(*slot).tts_values[i]` com um `pg_detoast_datum_copy` e dar `pfree` antes de retornar. O executor lê o
# MESMO slot depois do insert — `ExecProcessReturning` projeta dele via `slot_getattr` sem re-deform
# (PG18) — então RETURNING de uma coluna varlena dereferenciava memória liberada. Processar 10.000 linhas
# recicla o chunk liberado no próximo palloc da projeção, tornando a corrupção observável no md5 agregado.
# Com o slot preservado (fix v2), o md5 dos valores RETORNADOS bate com a origem.
cat > "$DATA/returning.sql" <<'SQL'
CREATE TABLE t_ret (LIKE src) USING theodb_columnar;
WITH ins AS (
    INSERT INTO t_ret SELECT * FROM src RETURNING id, big
)
SELECT md5(string_agg(big, '|' ORDER BY id)) FROM ins;
SQL
# -tA => saída sem bordas/header, o md5 sai como linha pura (o -q suprime as tags CREATE/INSERT). Sem isso
# o md5 vem indentado pela formatação de tabela e o grep ancorado não casaria (bug de parsing, não do fix).
RET_OUT=$(psql -X -q -tA -p "$PORT" -U theo -d postgres -f "$DATA/returning.sql" 2>&1)
echo "$RET_OUT" | grep -qiE "cannot fetch toast data|server closed|terminated|segmentation|connection to server" \
  && fail "RET (RETURNING #190v2): $(echo "$RET_OUT" | grep -iE 'cannot fetch|closed|terminated|segmentation|connection to' | head -1)"
RET_MD5=$(echo "$RET_OUT" | grep -oE '[0-9a-f]{32}' | head -1)
[ -n "$RET_MD5" ] || fail "RET (RETURNING #190v2): sem md5 na saida (crash/erro) — $RET_OUT"
[ "$RET_MD5" = "$SRC_MD5" ] || fail "RET (RETURNING #190v2): md5 dos valores RETURNING difere da origem (use-after-free no slot)"
# além do RETURNING, os valores persistidos também têm de bater (o slot preservado não pode ter alterado a gravação)
RET_STORED=$(q "SELECT md5(string_agg(big, '|' ORDER BY id)) FROM t_ret;")
[ "$RET_STORED" = "$SRC_MD5" ] || fail "RET: md5 persistido difere da origem"

echo "TOAST_CHECK_OK — 2 lotes mesma-txn + txns-distintas (EC-6), md5 do conteudo TOAST identico a origem, tipos fixos intactos (EC-5), limiar 1900/2000/2100 (EC-4), INSERT...RETURNING de TOAST sem use-after-free (RET/#190v2)"
