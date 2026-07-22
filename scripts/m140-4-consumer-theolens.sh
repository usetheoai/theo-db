#!/bin/bash
# M140.4 — PROOF do primeiro consumidor real (theo-lens) da engine BM25. Reproduz o SHAPE de busca do theo-lens
# (a relevância de traces por texto do span = input_value || output_value, hoje `ts_rank` em
# trace-read-repository.ts:365) sobre a engine own-code `bm25_search`, provando que retorna o trace correto para
# um termo distintivo. É a evidência que alimenta o anchor do M141 (dogfood `running`); o CUTOVER de produção +
# os 30 dias são o M141. Requer `cargo pgrx install --features spike-lexical`.
set +e
PGINST=/root/.pgrx/18.4/pgrx-install; PORT=28963
P=$(mktemp -d); chmod 777 "$P"; DATA=$P/d; LOG=$P/log
chmod -R o+rX "$PGINST"; chmod o+x /root
runuser -u pgtest -- bash -c "
  $PGINST/bin/initdb -D $DATA -U postgres --locale=C.UTF-8 >/dev/null 2>&1
  echo \"listen_addresses='localhost'\" >> $DATA/postgresql.conf
  $PGINST/bin/pg_ctl -D $DATA -o '-p $PORT' -l $LOG start >/dev/null 2>&1
"
sleep 4
PSQL="$PGINST/bin/psql -h 127.0.0.1 -p $PORT -U postgres -d postgres -tA"
$PSQL -c "CREATE EXTENSION theodb_rs" >/dev/null 2>&1

FAIL=0
chk() { if [ "$2" = "$3" ]; then echo "OK   $1 = $2"; else echo "FAIL $1 = $2 (esperado $3)"; FAIL=1; fi; }

# 1. Spans no SHAPE do theo-lens (schema.ts: spans.input_value, spans.output_value; trace_id text).
$PSQL -c "CREATE TABLE spans (span_id text, trace_id text, input_value text, output_value text)" >/dev/null 2>&1
$PSQL -c "INSERT INTO spans VALUES
  ('s1','trace-aaa','user: qual o clima?','assistant: ensolarado 25 graus'),
  ('s2','trace-aaa','tool: weather_api call','result: sunny 25C'),
  ('s3','trace-bbb','user: erro no deploy blkfoxzz','assistant: verifique o timeout de conexao'),
  ('s4','trace-ccc','user: liste os arquivos','tool: ls /home retornou 12 arquivos'),
  ('s5','trace-ddd','claude_code.tool Bash','command git status; nothing to commit')" >/dev/null 2>&1

# 2. Tabela lexical no shape que o theo-lens indexaria: body = input_value || output_value por span, id estável
#    a partir do trace_id (o consumidor busca TRACES; um span basta para o trace casar — como o EXISTS do M24).
$PSQL -c "CREATE TABLE spans_lex AS
  SELECT (abs(hashtext(trace_id)))::bigint AS id, trace_id,
         coalesce(input_value,'')||' '||coalesce(output_value,'') AS body
  FROM spans" >/dev/null 2>&1

# 3. Índice BM25 own-code sobre o corpus do theo-lens.
N=$($PSQL -c "SELECT bm25_build(400,'spans_lex','id','body')")
chk "bm25_build indexa os spans do theo-lens" "$N" "5"

# 4. Busca de trace por termo distintivo (o que o theo-lens faz com ts_rank hoje) → retorna o trace certo.
HIT_TRACE=$($PSQL -c "SELECT sl.trace_id FROM bm25_search(400,'blkfoxzz',10) b JOIN spans_lex sl ON sl.id=b.id ORDER BY b.score DESC LIMIT 1")
chk "bm25_search retorna o TRACE certo p/ o termo distintivo 'blkfoxzz'" "$HIT_TRACE" "trace-bbb"

# 5. Termo do domínio Claude Code (o usuário primário do theo-lens) → o trace certo.
HIT2=$($PSQL -c "SELECT sl.trace_id FROM bm25_search(400,'git status commit',10) b JOIN spans_lex sl ON sl.id=b.id ORDER BY b.score DESC LIMIT 1")
chk "bm25_search casa o trace de tool Claude Code" "$HIT2" "trace-ddd"

# 6. Query natural (multi-termo) — o parser sanitiza como o theo-lens espera; retorna o trace de clima.
HIT3=$($PSQL -c "SELECT sl.trace_id FROM bm25_search(400,'clima ensolarado',10) b JOIN spans_lex sl ON sl.id=b.id ORDER BY b.score DESC LIMIT 1")
chk "bm25_search casa o trace por query natural multi-termo" "$HIT3" "trace-aaa"

runuser -u pgtest -- $PGINST/bin/pg_ctl -D $DATA stop -m immediate >/dev/null 2>&1
rm -rf "$P"
if [ "$FAIL" = "0" ]; then echo "CONSUMER_OK — theo-lens shape (input||output do span) busca traces via bm25_search"; else echo "CONSUMER_FAIL"; fi
exit $FAIL
