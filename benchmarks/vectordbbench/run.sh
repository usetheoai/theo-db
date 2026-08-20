#!/usr/bin/env bash
# Corrida comparativa TheoDB x pgvector no VectorDBBench.
#
# Dois gates, e os dois existem porque a alternativa produz número errado com aparência
# de certo:
#
#   1. VERSÃO — se os dois PostgreSQL não forem a mesma versão, a corrida não roda. O
#      compose do upstream fixa pg16 e o TheoDB é PG18-only; comparar assim mediria a
#      diferença entre versões do PostgreSQL e a atribuiria ao índice.
#
#   2. FALHA — `vectordbbench` sai com 0 mesmo quando o caso falha, e imprime uma linha de
#      resumo com recall 0.0 que parece resultado. Medido: uma corrida com o banco
#      inalcançável saiu 0 e emitiu essa linha. Então o sucesso é verificado no log, não
#      no código de saída.
set -euo pipefail

CASE="${CASE:-Performance1536D50K}"
K="${K:-10}"
EF_SEARCH="${EF_SEARCH:-64}"
OUT="${OUT:-$(pwd)/results}"

echo "== gate 1: mesma versão de PostgreSQL dos dois lados =="
A=$(docker exec vdbb-theodb   psql -U postgres -d theo -tAc "SHOW server_version_num")
B=$(docker exec vdbb-pgvector psql -U postgres -d theo -tAc "SHOW server_version_num")
echo "   theodb=$A  pgvector=$B"
if [ "$A" != "$B" ]; then
  echo "   BLOQUEADO: versões divergem; a corrida mediria a versão, não o índice." >&2
  exit 1
fi

echo "== pré-voo: as portas publicadas aceitam TCP? =="
# Falha em 1 segundo em vez de depois da carga do dataset. Um contêiner "Up (healthy)"
# pode não ter publicado porta nenhuma — foi exatamente o que aconteceu quando um `up`
# parcial reaproveitou um contêiner com a rede quebrada.
for port in 55435 55436; do
  if ! timeout 3 bash -c "</dev/tcp/127.0.0.1/$port" 2>/dev/null; then
    echo "   BLOQUEADO: 127.0.0.1:$port não aceita TCP. \`docker ps\` pode dizer healthy" >&2
    echo "   e mesmo assim não publicar a porta; confira \`docker port\`." >&2
    exit 1
  fi
  echo "   127.0.0.1:$port ok"
done

mkdir -p "$OUT"

run() {   # $1 = comando da CLI, $2 = rótulo, $3 = porta
  local log="$OUT/$2.log"
  echo "== corrida: $2 =="
  # Redireciona direto para o arquivo em vez de `| tee`: num pipeline, se o shell pai
  # morre o `tee` morre junto e o filho bloqueia em pipe_read para sempre — a corrida
  # fica viva, sem escrever nada, e parece travada. Medido.
  vectordbbench "$1" \
      --host 127.0.0.1 --port "$3" --db-name theo \
      --user-name postgres --password theo \
      --db-label "$2" --case-type "$CASE" --k "$K" \
      --ef-search "$EF_SEARCH" --drop-old > "$log" 2>&1

  # O código de saída não distingue sucesso de falha; o log distingue.
  if grep -q "failed to run" "$log"; then
    echo "   FALHOU: $2 — motivo em $log:" >&2
    grep -m1 -o "reason=.*" "$log" | head -c 300 >&2; echo >&2
    return 1
  fi
  echo "   ok: $2"
}

failed=0
run theodbhnsw   theodb-pg18   55435 || failed=1
run pgvectorhnsw pgvector-pg18 55436 || failed=1

if [ "$failed" -ne 0 ]; then
  echo "== ao menos uma corrida falhou. NADA é publicado: uma tabela com metade dos" >&2
  echo "   motores é uma comparação que não aconteceu. ==" >&2
  exit 1
fi
echo "== as duas corridas completaram. Resultados em $OUT =="
