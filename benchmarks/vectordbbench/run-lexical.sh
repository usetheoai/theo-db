#!/usr/bin/env bash
# Comparação lexical de três motores, MESMA máquina, MESMA corrida.
#
# O gate leu duas versões erradas antes de acertar:
#   1ª: procurava "failed to run" no log — um erro de USO do click não contém isso, e um motor que nunca
#       rodou passou como ok.
#   2ª: exigia a linha de métricas com o rótulo — mas o OSSOpenSearch emite a linha com a coluna de rótulo
#       VAZIA, então uma corrida bem-sucedida foi rejeitada.
# Agora o critério é o JSON de resultado: existe, tem o motor certo, e tem recall e qps não nulos.
set -uo pipefail
export PATH=/root/b047/venv/bin:$PATH
OUT=/root/b047/results; mkdir -p "$OUT"
RES=/root/b047/venv/lib/python3.11/site-packages/vectordb_bench/results
CASE=FTSBm25Performance; K=10

echo "== pré-voo =="
curl -fs localhost:9200/_cluster/health >/dev/null || { echo "   BLOQUEADO: elastic" >&2; exit 1; }
curl -fs localhost:9201/_cluster/health >/dev/null || { echo "   BLOQUEADO: opensearch" >&2; exit 1; }
docker exec lex-theodb psql -U postgres -d theo -q -c \
  "DROP TABLE IF EXISTS probe; CREATE TABLE probe(id bigint primary key, body text); INSERT INTO probe VALUES (1,'the fox jumps');" >/dev/null 2>&1
docker exec lex-theodb psql -U postgres -d theo -tAc "SELECT bm25_build(1,'probe','id','body')" >/dev/null 2>&1
echo "   stemming do TheoDB ativo: $(docker exec lex-theodb psql -U postgres -d theo -tAc "SELECT count(*) FROM bm25_search(1,'jumping',5)")  (1 = sim)"

verifica() {  # $1 = nome do motor no JSON, $2 = rótulo
  python3 - "$RES" "$1" "$2" <<'PY'
import json, sys, glob, os
res, db, label = sys.argv[1], sys.argv[2], sys.argv[3]
best = None
for f in glob.glob(os.path.join(res, "*", "*.json")):
    try: d = json.load(open(f))
    except Exception: continue
    for r in d.get("results", []):
        t = r.get("task_config", {}) or {}; m = r.get("metrics", {}) or {}
        if t.get("db") != db: continue
        if (t.get("db_config") or {}).get("db_label") not in (label, "", None): continue
        if not (m.get("recall") and m.get("qps")): continue
        k = os.path.getmtime(f)
        if best is None or k > best[0]: best = (k, m)
if best is None:
    print("SEM_RESULTADO"); sys.exit(1)
m = best[1]
print(f"recall={m.get('recall'):.4f} ndcg={m.get('ndcg')} mrr={m.get('mrr')} "
      f"qps={m.get('qps'):.1f} p99={m.get('serial_latency_p99')} load={m.get('load_duration'):.2f}")
PY
}

corrida() {  # $1 = comando, $2 = nome no JSON, $3 = rótulo, $4.. = args
  local cmd="$1" dbname="$2" label="$3"; shift 3
  echo "== $label =="
  vectordbbench "$cmd" --db-label "$label" --case-type "$CASE" --k "$K" --drop-old "$@" > "$OUT/$label.log" 2>&1
  local rc=$?
  local out; out=$(verifica "$dbname" "$label")
  if [ $rc -ne 0 ] || [ "$out" = "SEM_RESULTADO" ]; then
    echo "   FALHOU (exit=$rc): $({ grep -m1 -o 'reason=.*' "$OUT/$label.log" || tail -3 "$OUT/$label.log"; } 2>/dev/null | head -c 250)" >&2
    return 1
  fi
  echo "   $out"
  echo "   ok"
}

fail=0
corrida theodbhnsw       TheoDB        theodb-b044     --host 127.0.0.1 --port 55435 --db-name theo --user-name postgres --password theo || fail=1
corrida elasticcloudhnsw ElasticCloud  elastic-8153    --host 127.0.0.1 --port 9200 --scheme http --user elastic --password changeme     || fail=1
corrida ossopensearch    OSSOpenSearch opensearch-2171 --host 127.0.0.1 --port 9201                                                      || fail=1

[ "$fail" -ne 0 ] && { echo "== ao menos um motor falhou. NADA é publicado. ==" >&2; exit 1; }
echo "== os três completaram =="
