#!/usr/bin/env bash
# Segunda rodada: Elastic e OpenSearch com analisador `english` (stemming + stopwords), equivalente ao
# `theodb_en`. A primeira rodada mediu product-default e é honesta como tal; esta mede o RANQUEAMENTO,
# removendo a assimetria de pré-processamento.
set -uo pipefail
# Pré-requisito: o analisador `english` como padrão dos índices dos dois motores.
for P in 9200 9201; do
  curl -s -X PUT "localhost:$P/_index_template/vdbb_english" -H 'Content-Type: application/json' -d '{
    "index_patterns": ["vdb_bench_index*", "vdb_bench_indice*"],
    "priority": 500,
    "template": {"settings": {"index": {"analysis": {"analyzer": {"default": {"type": "english"}, "default_search": {"type": "english"}}}}}}
  }' >/dev/null
done
export PATH=/root/b047/venv/bin:$PATH
OUT=/root/b047/results; mkdir -p "$OUT"
RES=/root/b047/venv/lib/python3.11/site-packages/vectordb_bench/results

verifica() {
  python3 - "$RES" "$1" "$2" <<'PY'
import json, sys, glob, os
res, db, label = sys.argv[1], sys.argv[2], sys.argv[3]
best=None
for f in glob.glob(os.path.join(res,"*","*.json")):
    try: d=json.load(open(f))
    except Exception: continue
    for r in d.get("results",[]):
        t=r.get("task_config",{}) or {}; m=r.get("metrics",{}) or {}
        if t.get("db")!=db: continue
        if (t.get("db_config") or {}).get("db_label") not in (label,"",None): continue
        if not (m.get("recall") and m.get("qps")): continue
        k=os.path.getmtime(f)
        if best is None or k>best[0]: best=(k,m)
if best is None: print("SEM_RESULTADO"); sys.exit(1)
m=best[1]
print(f"recall={m.get('recall'):.4f} ndcg={m.get('ndcg'):.4f} mrr={m.get('mrr'):.4f} qps={m.get('qps'):.1f} p99={m.get('serial_latency_p99')} load={m.get('load_duration'):.2f}")
PY
}
corrida() {
  local cmd="$1" dbname="$2" label="$3"; shift 3
  echo "== $label =="
  vectordbbench "$cmd" --db-label "$label" --case-type FTSBm25Performance --k 10 --drop-old "$@" > "$OUT/$label.log" 2>&1
  local rc=$?; local out; out=$(verifica "$dbname" "$label")
  if [ $rc -ne 0 ] || [ "$out" = "SEM_RESULTADO" ]; then
    echo "   FALHOU (exit=$rc): $({ grep -m1 -o 'reason=.*' "$OUT/$label.log" || tail -3 "$OUT/$label.log"; } 2>/dev/null | head -c 250)" >&2
    return 1
  fi
  echo "   $out"; echo "   ok"
}
fail=0
corrida elasticcloudhnsw ElasticCloud  elastic-english    --host 127.0.0.1 --port 9200 --scheme http --user elastic --password changeme || fail=1
corrida ossopensearch    OSSOpenSearch opensearch-english --host 127.0.0.1 --port 9201                                                  || fail=1
[ "$fail" -ne 0 ] && { echo "== falhou; nada publicado ==" >&2; exit 1; }
echo "== rodada justa completa =="
# NOTA: o template precisa casar os DOIS nomes de índice. O cliente Elastic usa `vdb_bench_indice`
# e o do OpenSearch usa `vdb_bench_index` — um padrão só cobre um deles, e a corrida sai idêntica
# ao product-default sem avisar. Medido: foi o que aconteceu na primeira tentativa.
