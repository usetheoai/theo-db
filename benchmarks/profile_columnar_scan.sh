#!/usr/bin/env bash
# M148 — flamegraph do scan colunar. Mede ONDE o tempo vai numa query lenta do ClickBench sobre o
# theodb_columnar, para priorizar M149 (projection pushdown) / M150 (chunk-filter) / M151 (vetorização)
# por EVIDÊNCIA, não por hipótese de código.
#
# SOTA (Jan Nidzwetzki, "Analyzing PostgreSQL Performance Using Flame Graphs", 2025; Rust Perf Book;
# flamegraph-rs): perf record --call-graph dwarf -F 111 -p <backend_pid> durante a query, depois
# stackcollapse-perf.pl | flamegraph.pl. `dwarf` reconstrói a pilha sem frame pointers (release build).
#
# EC-1 (MUST FIX do edge-case review): ABORTA se < 500 amostras — um flamegraph vazio prioriza o milestone
# errado (o análogo do harness vácuo do #190).
# EC-2: profila DUAS queries — a mais lenta (pode ser dominada por uma função do PG, não pelo nosso scan)
# e uma de scan puro — para o veredito não tomar uma query atípica como representativa.
#
# Uso: profile_columnar_scan.sh <N_ROWS>   (default 1000000)
# Saídas em OUT_DIR (default /root/m148-out): m148-slow-flamegraph.svg (query lenta) + m148-slow-folded.txt,
#   m148-scanpuro-flamegraph.svg (scan puro) + m148-scanpuro-folded.txt. O eixo CPU-vs-I/O é derivado dos
#   folded no fim (não há mais perf-stat anexado — era frágil).
set -uo pipefail
N="${1:-1000000}"
PGINST="${PGINST:-/root/.pgrx/18.4/pgrx-install}"
FLAME="${FLAME:-/root/FlameGraph}"
OUT_DIR="${OUT_DIR:-/root/m148-out}"
export PATH="$PGINST/bin:$PATH"
mkdir -p "$OUT_DIR"
DATA=$(mktemp -d /tmp/m148.XXXXXX); PORT="${PORT:-28948}"

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; }
trap cleanup EXIT
fail() { echo "M148_FAIL $*"; exit 1; }

command -v perf >/dev/null || fail "perf ausente"
[ -x "$FLAME/flamegraph.pl" ] || fail "FlameGraph ausente em $FLAME"

initdb -D "$DATA" -U postgres >/dev/null 2>&1 || fail initdb
{ echo "port=$PORT"; echo "shared_preload_libraries='theodb_rs'"; echo "shared_buffers=4GB";
  echo "work_mem=256MB"; echo "maintenance_work_mem=2GB"; echo "autovacuum=off"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null 2>&1 || fail start
q() { psql -X -q -p "$PORT" -U postgres -d postgres -tAc "$1" 2>&1; }

echo "== carregando $N linhas do ClickBench hits REAL (105 colunas) no theodb_columnar =="
q "CREATE EXTENSION theodb_rs CASCADE;" >/dev/null
# O gargalo hipotético é o decode das 105 colunas — um fixture de poucas colunas mediria a coisa errada
# (EC-2/representatividade). Usa o schema + dados REAIS do ClickBench (105 colunas).
# Amostragem HEAD é OK AQUI: para PROFILING do custo de decode/deform por linha, o viés de cardinalidade
# (que afeta GROUP BY) é irrelevante — só o formato de 105 colunas e o volume importam. E head-n é rápido
# (não varre os ~100 GB). NÃO usar este dataset para claims de latência comparável — só para o flamegraph.
HITS_URL="https://datasets.clickhouse.com/hits_compatible/hits.tsv.gz"
CB_RAW="https://raw.githubusercontent.com/ClickHouse/ClickBench/main/postgresql"
CREATE_SQL="$DATA/create.sql"
curl -sL "$CB_RAW/create.sql" -o "$DATA/create_orig.sql" || fail "fetch create.sql"
# adapta o create.sql do ClickBench para theodb_columnar (troca o ');' final)
sed 's/);[[:space:]]*$/) USING theodb_columnar;/' "$DATA/create_orig.sql" | sed 's/CREATE TABLE hits/CREATE TABLE hits/' > "$CREATE_SQL"
# heap de controle: mesmo schema, storage default
sed 's/) USING theodb_columnar;/);/; s/CREATE TABLE hits/CREATE TABLE hits_heap/' "$CREATE_SQL" > "$DATA/create_heap.sql"
psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U postgres -d postgres -f "$DATA/create_heap.sql" >/dev/null 2>&1 || fail "create hits_heap"
psql -X -q -v ON_ERROR_STOP=1 -p "$PORT" -U postgres -d postgres -f "$CREATE_SQL" >/dev/null 2>&1 || fail "create hits (columnar)"
echo "  streaming $N linhas (head) do hits real..."
curl -sL "$HITS_URL" | zcat | head -n "$N" > "$DATA/hits.tsv" 2>/dev/null
[ -s "$DATA/hits.tsv" ] || fail "stream do hits falhou"
psql -X -q -p "$PORT" -U postgres -d postgres -c "\copy hits_heap FROM '$DATA/hits.tsv' WITH (FORMAT text)" >/dev/null 2>&1 || fail "copy para heap"
LOADED=$(q "INSERT INTO hits SELECT * FROM hits_heap; SELECT count(*) FROM hits;")
[ "$LOADED" = "$N" ] || fail "carga incompleta: $LOADED de $N (o #190 esta aplicado?)"
echo "  carregado: $LOADED linhas x 105 colunas"

# --- função de profiling de UMA query ---
profile_query() {
  local label="$1" sql="$2"
  echo "== profiling [$label]: ${sql:0:60}... =="
  local sqlf="$DATA/$label.sql"
  # abre a sessão, imprime o backend pid, ESPERA (pg_sleep) para o perf anexar, DEPOIS roda a query
  cat > "$sqlf" <<SQL
SELECT pg_backend_pid();
SELECT pg_sleep(1.5);
$sql
SQL
  # inicia a sessão em background; captura o pid. -tA => saída sem bordas, o pid sai como número puro.
  psql -X -q -tA -p "$PORT" -U postgres -d postgres -f "$sqlf" > "$DATA/$label.out" 2>&1 &
  local psql_pid=$!
  # espera o pid do backend aparecer
  local bpid=""
  for _ in $(seq 1 30); do
    bpid=$(grep -m1 -E "^[0-9]+$" "$DATA/$label.out" 2>/dev/null | head -1)
    [ -n "$bpid" ] && break
    sleep 0.2
  done
  [ -n "$bpid" ] || { kill $psql_pid 2>/dev/null; fail "$label: nao obtive o backend pid"; }
  # anexa o perf ao backend durante a query (a query roda apos o pg_sleep de 1.5s)
  perf record --call-graph dwarf -F 111 -p "$bpid" -o "$DATA/$label.perf" -- sleep 60 >/dev/null 2>&1 || true
  wait $psql_pid 2>/dev/null
  # EC-1: piso de amostras. `grep -c` já imprime "0" quando não casa (e sai 1); `|| true` evita que o
  # pipefail propague o exit, sem o `|| echo 0` que duplicava a saída em "0\n0" e quebrava o `-ge` abaixo.
  local nsamples
  nsamples=$(perf script -i "$DATA/$label.perf" 2>/dev/null | grep -c "^[a-z]" || true)
  nsamples=${nsamples:-0}
  echo "  amostras: $nsamples"
  [ "$nsamples" -ge 500 ] || fail "$label: perf capturou $nsamples amostras (< 500) — flamegraph seria vacuo (EC-1)"
  # folded + SVG
  perf script -i "$DATA/$label.perf" 2>/dev/null | "$FLAME/stackcollapse-perf.pl" > "$OUT_DIR/m148-$label-folded.txt" 2>/dev/null
  "$FLAME/flamegraph.pl" --title "M148 $label ($N rows)" "$OUT_DIR/m148-$label-folded.txt" > "$OUT_DIR/m148-$label-flamegraph.svg" 2>/dev/null
  # EC-3: os frames tem simbolos nossos?
  local sym
  sym=$(grep -cE "theodb|columnar|decode|zstd" "$OUT_DIR/m148-$label-folded.txt" || true)
  sym=${sym:-0}
  echo "  frames com simbolo relevante: $sym"
  # top-5 frames por tempo proprio (folded: cada linha "stack count"; o ultimo frame da stack e o self)
  echo "  --- top-5 frames por tempo proprio [$label] ---"
  awk '{n=$NF; $NF=""; split($0,a," "); leaf=a[length(a)]; c[leaf]+=n; tot+=n} END{for(k in c) printf "%d\t%s\n", c[k], k; print "TOTAL\t"tot > "/dev/stderr"}' \
    "$OUT_DIR/m148-$label-folded.txt" | sort -rn | head -5
}

# EC-2: DUAS queries. As colunas do ClickBench (CamelCase sem aspas) viram lowercase no PG — a query
# "slow" usa `referer` e funciona; a "scanpuro" usa `advengineid` (coluna real, ClickBench q2), scan sem
# GROUP BY nem função cara, para isolar se o gargalo persiste sem a agregação.
profile_query "slow" "SELECT referer, count(*) FROM hits GROUP BY referer ORDER BY 2 DESC LIMIT 10;"
profile_query "scanpuro" "SELECT count(*) FROM hits WHERE advengineid <> 0;"

# Eixo CPU-bound vs I/O-bound (R1): derivado dos folded já capturados — robusto e auto-contido (o
# perf-stat-anexado-a-um-backend-que-termina é frágil). Se qualquer frame de I/O real (pread/FileRead/
# mdread/io_uring) somar > 5% do tempo, a query é I/O-bound; senão é CPU-bound.
echo "== eixo CPU vs I/O (derivado dos folded) =="
for lbl in slow scanpuro; do
  awk -v L="$lbl" '{n=$NF; tot+=n; if ($0 ~ /pread|__read|FileRead|mdread|io_uring|preadv/) io+=n}
    END{ printf "  [%s] I/O-frames=%.2f%% -> %s\n", L, (tot?io*100/tot:0), (io*100/tot>5?"I/O-BOUND":"CPU-BOUND") }' \
    "$OUT_DIR/m148-$lbl-folded.txt"
done

echo "M148_PROFILE_OK — SVGs + folded + perfstat em $OUT_DIR"
