#!/usr/bin/env bash
# Corrida full-text (BM25) do TheoDB no VectorDBBench.
#
# Herda os dois gates do run.sh pela mesma razão: a alternativa produz número errado com
# aparência de certo. E acrescenta um terceiro, próprio do FTS.
set -euo pipefail

CASE="${CASE:-FTSBm25Performance}"
K="${K:-10}"
OUT="${OUT:-$(pwd)/results}"
LABEL="${LABEL:-theodb-fts}"

echo "== gate 1: o banco responde e é o que dizemos ser =="
A=$(docker exec vdbb-theodb psql -U postgres -d theo -tAc "SHOW server_version_num")
echo "   theodb: PostgreSQL $A"
docker exec vdbb-theodb psql -U postgres -d theo -tAc \
  "SELECT 1 FROM pg_proc WHERE proname='bm25_search'" | grep -q 1 || {
  echo "   BLOQUEADO: bm25_search não existe nesta imagem." >&2; exit 1; }

echo "== gate 2: a porta publicada aceita TCP =="
timeout 3 bash -c "</dev/tcp/127.0.0.1/55435" 2>/dev/null || {
  echo "   BLOQUEADO: 127.0.0.1:55435 não aceita TCP." >&2; exit 1; }
echo "   ok"

mkdir -p "$OUT"
LOG="$OUT/$LABEL.log"

echo "== corrida FTS: $CASE =="
# Sem `| tee`: num pipeline, se o shell pai morre o tee morre junto e o filho trava em
# pipe_read — corrida viva, sem escrever nada, parecendo travada. Medido no B-035.
START=$(date +%s)
vectordbbench theodbhnsw \
  --host 127.0.0.1 --port 55435 --db-name theo \
  --user-name postgres --password theo \
  --db-label "$LABEL" --case-type "$CASE" --k "$K" \
  --drop-old > "$LOG" 2>&1 || true
echo "   duração total (inclui download do dataset): $(( $(date +%s) - START ))s"

# O código de saída não distingue sucesso de falha; o log distingue.
if grep -q "failed to run" "$LOG"; then
  echo "   FALHOU — motivo:" >&2
  grep -m1 -o "reason=.*" "$LOG" | head -c 300 >&2; echo >&2
  echo "== nada é publicado: uma corrida que falhou não vira tabela. ==" >&2
  exit 1
fi

# Gate 3, próprio do FTS: QPS sem recall/NDCG/MRR não é o produto deste item.
echo "== gate 3: as três métricas de qualidade saíram? =="
grep -m1 -E "TheoDB \|" "$LOG" | cut -c1-240 || true
echo "== corrida completa. Log em $LOG =="
