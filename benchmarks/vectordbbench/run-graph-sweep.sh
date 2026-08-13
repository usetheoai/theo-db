#!/usr/bin/env bash
# B-046 — varredura de qualidade de grafo: recall a `ef_search` FIXO, variando o build.
#
# A pergunta que esta corrida responde é a primeira metade da decomposição do déficit de
# QPS: o TheoDB entrega recall 0,9600 onde o pgvector entrega 0,9835 no MESMO
# `ef_search=64`. Se algum ponto de (m, ef_construction) fechar essa diferença, o déficit
# é qualidade de grafo. Se nenhum fechar, é a varredura — e aí a segunda metade
# (páginas por consulta a recall casado) diz quanto.
#
# TRÊS coisas que este runner faz de propósito, e a razão de cada uma:
#
#   1. `ef_search` é FIXO em 64. É o que isola a variável: qualquer mudança de recall vem
#      do grafo, não da busca. Varrer os dois ao mesmo tempo mediria a soma.
#
#   2. A grade é declarada AQUI, no arquivo versionado, antes de rodar. Escolher os
#      pontos depois de ver o resultado é como uma varredura vira caça ao ponto
#      conveniente — e o resultado seria irreprodutível por construção.
#
#   3. O sucesso é lido do JSON de resultado, NUNCA do código de saída: o `vectordbbench`
#      sai 0 mesmo quando o caso falha, e imprime uma linha de resumo com recall 0.0 que
#      parece resultado. Medido no B-035, e é o motivo de o `run.sh` fazer o mesmo.
#
#   4. `--search-serial` SEM `--search-concurrent`. Medido na T1.0: o estágio concorrente
#      varre 8 níveis (1,5,10,20,30,40,60,80) × 30 s ≈ 4 min por ponto, e produz QPS —
#      que a D2 proíbe publicar a partir do host. Pior: o JSON de resultado só é escrito
#      no fim da tarefa INTEIRA, então interromper o sweep de concorrência perdeu o
#      recall que a corrida já tinha medido. Quatro minutos por ponto para produzir um
#      número que não pode ser usado, com risco de perder o que pode.
#
# QPS NÃO é medido aqui e nenhum número de QPS desta corrida entra em artefato: recall é
# função do grafo e da consulta, e não muda com a máquina; QPS muda. O QPS do artefato
# sai do droplet de referência (ADR-0061), como no b035.
set -euo pipefail

CASE="${CASE:-Performance1536D50K}"
K="${K:-10}"
EF_SEARCH="${EF_SEARCH:-64}"
OUT="${OUT:-$(pwd)/results-graph-sweep}"

# A grade da D3 do plano. `M_GRID`/`EFC_GRID` são sobreponíveis para o corte de escopo
# que a T1.0 autoriza (12 → 6 pontos) — e o corte, se acontecer, é registrado com o
# número de pegada que o motivou, nunca em silêncio.
M_GRID="${M_GRID:-16 24 32}"
EFC_GRID="${EFC_GRID:-64 128 200 400}"

echo "== pré-voo: a porta do TheoDB aceita TCP? =="
if ! timeout 3 bash -c "</dev/tcp/127.0.0.1/55435" 2>/dev/null; then
  echo "   BLOQUEADO: 127.0.0.1:55435 não aceita TCP." >&2
  exit 1
fi

echo "== pré-voo: o servidor honra as reloptions? (a sonda do cliente faria isso na carga) =="
docker exec vdbb-theodb psql -U postgres -d theo -q -c \
  'CREATE TEMP TABLE _sweep_probe (e vector(2)); CREATE INDEX ON _sweep_probe USING hnsw (e vector_l2_ops) WITH (m = 32, ef_construction = 200);' \
  >/dev/null 2>&1 || {
    echo "   BLOQUEADO: este TheoDB não registra m/ef_construction como reloptions (pré-B-036)." >&2
    exit 1
  }

mkdir -p "$OUT"
echo "== grade declarada: m ∈ {$M_GRID} × ef_construction ∈ {$EFC_GRID}, ef_search=$EF_SEARCH =="

for m in $M_GRID; do
  for efc in $EFC_GRID; do
    label="sweep-m${m}-efc${efc}"
    echo "-- ponto $label --"
    RESULTS_LOCAL_DIR="$OUT" vectordbbench theodbhnsw \
      --user-name postgres --password theo \
      --host 127.0.0.1 --port 55435 --db-name theo \
      --case-type "$CASE" --k "$K" \
      --m "$m" --ef-construction "$efc" --ef-search "$EF_SEARCH" \
      --db-label "$label" --drop-old --load --search-serial \
      2>&1 | tee "$OUT/$label.log" || true
    # Um ponto que falha NÃO interrompe a grade: a tabela final mostra o buraco em vez de
    # omitir a linha. Omitir faria uma grade incompleta parecer completa.
  done
done

echo "== tabela: recall por ponto, lida do JSON (nunca do resumo impresso) =="
python3 - "$OUT" <<'PYEOF'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = []
for f in sorted(out.glob("*/*.json")):
    data = json.loads(f.read_text())
    for r in data.get("results", []):
        label = (r.get("task_config", {}).get("db_config") or {}).get("db_label", "")
        if not label.startswith("sweep-"):
            continue
        m = r.get("metrics", {}) or {}
        rows.append((label, m.get("recall"), m.get("qps"), m.get("load_duration")))
if not rows:
    print("NENHUM ponto produziu resultado — a grade está vazia, não plana.")
    sys.exit(1)
print(f"{'ponto':22} {'recall':>8} {'qps(host, NÃO publicável)':>26}")
for label, recall, qps, _ in sorted(rows):
    print(f"{label:22} {recall if recall is not None else '—':>8} {qps if qps is not None else '—':>26}")
distinct = {r[1] for r in rows if r[1] is not None}
if len(rows) > 1 and len(distinct) == 1:
    print("\nFALHA: todos os pontos deram o MESMO recall. O `WITH` não chegou ao build —")
    print("publicar esta tabela seria publicar uma varredura que não variou nada.")
    sys.exit(1)
PYEOF
