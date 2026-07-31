#!/usr/bin/env bash
# M169 — reconstruir o gêmeo `hits_heap` a 100M, SEM tocar em `hits`.
#
# POR QUE ESTE ARQUIVO EXISTE. `hits_heap` é o oráculo de byte-identidade de T2.1 e T4.1, e ele NÃO EXISTE: era
# UNLOGGED e foi truncado por crash recovery — duas vezes. O caminho óbvio (`run_m128_clickbench.run()`) está
# proibido porque faz `DROP TABLE IF EXISTS hits CASCADE` incondicionalmente (`:324-325`) e apagaria os 16 GB
# colunares já carregados e verificados.
#
# A DECISÃO DE PERSISTÊNCIA, e por que ela não é a óbvia:
#
#   - `UNLOGGED` durante o `COPY` mantém a mitigação do checkpoint-storm que a memória do M162 registra.
#   - Mas manter UNLOGGED depois da carga é AGENDAR A TERCEIRA PERDA: a tabela de cenários de falha do próprio
#     T1.2 declara "OOM mata a conexão" como cenário ESPERADO, e um backend morto por OOM leva o postmaster a
#     crash recovery, que trunca toda tabela UNLOGGED. O gate que o milestone planeja exercitar é o mesmo
#     mecanismo que apaga o gêmeo.
#   - Portanto: UNLOGGED no `COPY`, `SET LOGGED` imediatamente depois, antes de qualquer consulta.
#
# HONESTIDADE SOBRE O CUSTO: `ALTER TABLE ... SET LOGGED` a ~100 GB reescreve a tabela inteira no WAL e ninguém
# neste projeto mediu quanto custa. O script CRONOMETRA e imprime, para que o próximo não precise adivinhar — é
# um número que vira `Measurement` no OKF.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Default FORA de `/root`: aquele diretório é 700, e um caminho que atravessa 700 é inalcançável para o
# cliente mesmo com o arquivo em 644 (ver a guarda de leitura adiante).
TSV="${TSV_PATH:-/srv/bench-data/hits_sample.tsv}"
CREATE_SQL="${CREATE_SQL:-$HERE/clickbench/theodb/create.sql}"
export PGHOST="${PGHOST:-127.0.0.1}" PGPORT="${PGPORT:-5432}"
export PGDATABASE="${PGDATABASE:-postgres}" PGUSER="${PGUSER:-postgres}" PGOSUSER="${PGOSUSER:-pgtest}"

psql_c() { sudo -u "$PGOSUSER" env LD_LIBRARY_PATH=/opt/pg18/lib /opt/pg18/bin/psql \
             -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" -v ON_ERROR_STOP=1 "$@"; }

echo "=== guarda: a box está livre? ==="
BACKENDS=$(psql_c -Atc "select count(*) from pg_stat_activity where backend_type='client backend' and pid<>pg_backend_pid();")
LOAD=$(cut -d' ' -f1 /proc/loadavg)
echo "  backends=$BACKENDS loadavg=$LOAD"
if [ "${BACKENDS:-1}" -gt 0 ]; then
  echo "  ABORTA: há consulta rodando. Recarregar durante uma medição a contamina — e isso já custou uma rodada." >&2
  exit 1
fi

echo "=== guarda: 'hits' está intacto ANTES de mexer em qualquer coisa? ==="
HITS=$(psql_c -Atc "select count(*) from public.hits;")
echo "  hits=$HITS"
if [ "${HITS:-0}" != "99997497" ]; then
  echo "  ABORTA: 'hits' não tem 99.997.497 linhas. Não mexo no heap sem o colunar íntegro." >&2
  exit 1
fi

echo "=== guarda: o CLIENTE consegue mesmo LER o TSV? ==="
# MEDIDO 2026-07-31: esta guarda não existia, e a corrida anterior dropou `hits_heap`, recriou vazia, e SÓ
# ENTÃO descobriu `Permission denied` — deixando uma tabela de 0 linhas onde antes não havia nenhuma. Uma
# tabela vazia é PIOR que ausente: a atestação a lê como divergência de contagem em vez de ausência.
#
# A causa não é a permissão do ARQUIVO. `\copy` é client-side, o cliente roda como $PGOSUSER, e o arquivo
# estava 644 — legível por todos. O bloqueio era o DIRETÓRIO-PAI: todo componente do caminho precisa do bit
# `x` para o processo que lê, e `/root` é 700. O erro aponta para o arquivo e a causa está no caminho.
#
# Ler 1 byte COMO O USUÁRIO QUE VAI LER é a única prova; `test -r` rodado por outro usuário não vale.
if ! sudo -u "$PGOSUSER" head -c1 "$TSV" >/dev/null 2>&1; then
  echo "  ABORTA: '$PGOSUSER' não consegue ler '$TSV' — e NADA foi dropado." >&2
  echo "  Cheque o bit x de CADA diretório do caminho, não só a permissão do arquivo:" >&2
  namei -l "$TSV" 2>/dev/null | sed 's/^/    /' >&2 || true
  exit 1
fi

echo "=== cria hits_heap UNLOGGED a partir do MESMO create.sql do colunar ==="
# `hits` NÃO é tocado: só o gêmeo é dropado e recriado. É por isso que `run()` está proibido.
psql_c -Atc "DROP TABLE IF EXISTS public.hits_heap CASCADE;"
sed -e 's/USING theodb_columnar//' -e 's/CREATE TABLE hits/CREATE UNLOGGED TABLE hits_heap/' "$CREATE_SQL" \
  | psql_c -q -f - || { echo "  ABORTA: create do heap falhou" >&2; exit 1; }

echo "=== COPY do TSV (69,7 GB) — UNLOGGED para conter o checkpoint-storm ==="
T0=$(date +%s)
psql_c -c "\\copy public.hits_heap FROM '$TSV' WITH (FORMAT text)" || {
  echo "  ABORTA: COPY falhou" >&2; exit 1; }
T_COPY=$(( $(date +%s) - T0 ))
echo "  COPY levou ${T_COPY}s"

echo "=== SET LOGGED — o passo que impede a TERCEIRA perda (custo NÃO medido antes; cronometrando) ==="
T1=$(date +%s)
psql_c -Atc "ALTER TABLE public.hits_heap SET LOGGED;" || {
  echo "  AVISO: SET LOGGED falhou — a tabela fica UNLOGGED e SERÁ truncada no próximo crash recovery." >&2
  echo "  Isso NÃO é aceitável para T4.1; resolva antes de medir." >&2; exit 1; }
T_LOGGED=$(( $(date +%s) - T1 ))

echo "=== verificação: o gêmeo bate com o colunar, e é permanente ==="
HEAP=$(psql_c -Atc "select count(*) from public.hits_heap;")
PERS=$(psql_c -Atc "select relpersistence from pg_class where relname='hits_heap';")
echo "  hits_heap=$HEAP  relpersistence=$PERS  (esperado: $HITS e 'p')"
echo "  COPY=${T_COPY}s  SET_LOGGED=${T_LOGGED}s"
if [ "$HEAP" != "$HITS" ] || [ "$PERS" != "p" ]; then
  echo "  FALHA: o gêmeo não é utilizável como oráculo (contagem divergente ou ainda UNLOGGED)." >&2
  exit 1
fi
echo "=== hits_heap pronto: $HEAP linhas, permanente. COPY=${T_COPY}s SET_LOGGED=${T_LOGGED}s ==="
