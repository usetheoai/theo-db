#!/usr/bin/env bash
# M169 T1.2 — entry point for the 100M ClickBench baseline (the DoD invokes THIS file).
#
# Its whole job is to PIN the environment, because every value below is one that has silently broken a run
# before: the harness default PGPORT is 28900 (this box is 5432); the harness default statement_timeout is 60 s
# (the M162 "19/43" this milestone compares against was measured at 300 s); and the aggregate pushdown is OFF by
# default, without which q20's `byte array offset overflow` never fires and the baseline "proves" there is no bug.
#
# It refuses to start on a box that is not the one ADR-3 declared, and emits the attestation header BEFORE and
# AFTER the run — if loadavg rose above the threshold in the closing header, something ran alongside and the
# measurement is contaminated.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ART="${ART_DIR:-benchmarks/artifacts/m169-artifacts}"
# Default FORA de `/root` (modo 700): um caminho que atravessa 700 é inalcançável para qualquer processo que
# não seja root, por mais permissivo que o ARQUIVO seja. Ver o invariante do bit x em todo o caminho.
TSV="${TSV_PATH:-/srv/bench-data/hits_sample.tsv}"
LABEL="${LABEL:-baseline-100m}"
ALLOW_MISSING_HEAP="${ALLOW_MISSING_HEAP:-0}"

export PGHOST="${PGHOST:-127.0.0.1}" PGPORT="${PGPORT:-5432}"
export PGDATABASE="${PGDATABASE:-postgres}" PGUSER="${PGUSER:-postgres}" PGOSUSER="${PGOSUSER:-pgtest}"

mkdir -p "$ART"

echo "=== atestação da box (ANTES) ==="
python3 "$HERE/m169_box_attest.py" --tsv "$TSV" --json > "$ART/${LABEL}-box-before.json"
ATTEST_RC=$?
cat "$ART/${LABEL}-box-before.json"

if [ "$ATTEST_RC" -ne 0 ]; then
  # The ONLY tolerated failure is the absent heap twin, and only when asked for explicitly. Everything else —
  # wrong box, busy box, unattended-upgrades live, row-count mismatch — aborts. A gate silenced by `|| true`
  # is a gate that does not exist.
  # Testa o ID ESTAVEL, nunca substring de prosa: "hits_heap is ABSENT" e "hits_heap_rows disagrees" contem
  # ambos "hits_heap", entao um casamento por texto toleraria tambem o gemeo com populacao ERRADA -- que e pior
  # que ausente, porque o A/B compararia duas populacoes e passaria.
  ONLY_HEAP=$(python3 - "$ART/${LABEL}-box-before.json" <<'PYEOF'
import json, sys
f = json.load(open(sys.argv[1]))["failures"]
print("1" if f and all(x.split(" | ", 1)[0] == "hits_heap_absent" for x in f) else "0")
PYEOF
)
  if [ "$ONLY_HEAP" = "1" ] && [ "$ALLOW_MISSING_HEAP" = "1" ]; then
    echo "  AVISO: hits_heap ausente — corrida segue SEM oráculo A/B (ALLOW_MISSING_HEAP=1)."
    echo "  O artefato registra 'n/a — nenhuma comparação executada'; NUNCA 'byte-identical'."
  else
    echo "  ABORTA: a box não satisfaz os critérios de T1.1. Veja as falhas acima." >&2
    exit 1
  fi
fi

echo "=== corrida das 43 consultas (agg=on, 1 execução por consulta) ==="
python3 "$HERE/m169_baseline_run.py" --out "$ART/${LABEL}.jsonl" --label "$LABEL" --agg "$@"
RUN_RC=$?

# --quick: o cabeçalho de FECHAMENTO pergunta "algo rodou junto?", não "o dado ainda está lá?" — que uma corrida
# read-only não pode ter mudado. As checagens de dataset custam ~40 min MEDIDOS a 100M; pagá-las duas vezes
# acrescentaria mais de uma hora a uma corrida que já é de horas. O JSON registra que o modo foi quick.
echo "=== atestação da box (DEPOIS, --quick — prova de não-contaminação) ==="
python3 "$HERE/m169_box_attest.py" --tsv "$TSV" --quick --json > "$ART/${LABEL}-box-after.json"
AFTER_RC=$?
# O alerta TEM de reprovar. Numa versao anterior ele fazia sys.exit(1) e a linha seguinte descartava o codigo;
# e o "|| true" deixava o JSON faltar em silencio, fazendo o comparador estourar e o script seguir. Um guard
# cujo veredito ninguem le e decoracao.
if [ "$AFTER_RC" -ne 0 ] && [ ! -s "$ART/${LABEL}-box-after.json" ]; then
  echo "  ABORTA: a atestacao de fechamento nao produziu artefato -- a nao-contaminacao NAO foi verificada." >&2
  exit 1
fi
python3 - "$ART/${LABEL}-box-before.json" "$ART/${LABEL}-box-after.json" <<'PYEOF'
import json, sys
b = json.load(open(sys.argv[1]))["facts"]
a = json.load(open(sys.argv[2]))["facts"]
print(f"  loadavg1 antes={b['loadavg1']} depois={a['loadavg1']}")
print(f"  so_md5   antes={b['so_md5']} depois={a['so_md5']}")
if a["so_md5"] != b["so_md5"]:
    print("  ALERTA: o .so MUDOU durante a corrida -- a medicao mistura dois binarios", file=sys.stderr)
    sys.exit(1)
PYEOF
CONTAM_RC=$?
if [ "$CONTAM_RC" -ne 0 ]; then exit "$CONTAM_RC"; fi

echo "=== gate de nao-vacuidade + artefato ==="
python3 "$HERE/m169_baseline_summarize.py" "$ART/${LABEL}.jsonl"
SUMM_RC=$?
# O gerador RECUSA emitir quando a proveniencia esta incompleta (so_md5 desconhecido, binario trocado no meio,
# corrida truncada). Um artefato que parece completo e nao e sobrevive muito depois de alguem lembrar o contexto.
python3 "$HERE/m169_baseline_report.py" "$ART/${LABEL}.jsonl" \
  "$ART/${LABEL}-box-before.json" "$ART/${LABEL}-box-after.json" > "$ART/${LABEL}.md"
REPORT_RC=$?
if [ "$REPORT_RC" -ne 0 ]; then
  echo "  o gerador recusou emitir o artefato (veja o motivo acima)" >&2
  rm -f "$ART/${LABEL}.md"
fi
echo "  artefato: $ART/${LABEL}.md"

# T4.1 — o delta contra uma corrida anterior, quando `COMPARE_TO` nomeia o label dela.
#   LABEL=after-100m COMPARE_TO=baseline-100m bash benchmarks/m169_baseline_100m.sh --stream
# O gerador RECUSA publicar quando as condições não batem (teto, box, corpus, ou o MESMO binário dos dois lados),
# e a recusa é tratada aqui como falha da corrida — um delta que não pôde ser emitido não pode virar silêncio,
# porque silêncio se lê como "não havia delta".
DELTA_RC=0
if [ -n "${COMPARE_TO:-}" ]; then
  echo "=== delta contra '${COMPARE_TO}' ==="
  python3 "$HERE/m169_delta.py" \
    "$ART/${COMPARE_TO}.jsonl" "$ART/${COMPARE_TO}-box-before.json" \
    "$ART/${LABEL}.jsonl"      "$ART/${LABEL}-box-before.json" > "$ART/${LABEL}-delta.md"
  DELTA_RC=$?
  if [ "$DELTA_RC" -ne 0 ]; then
    echo "  o gerador RECUSOU publicar o delta (motivo acima) — as duas corridas não são comparáveis" >&2
    rm -f "$ART/${LABEL}-delta.md"
  else
    echo "  delta: $ART/${LABEL}-delta.md"
  fi
fi

if [ "$SUMM_RC" -ne 0 ]; then exit "$SUMM_RC"; fi
if [ "$DELTA_RC" -ne 0 ]; then exit "$DELTA_RC"; fi
if [ "$REPORT_RC" -ne 0 ]; then exit "$REPORT_RC"; fi
exit $RUN_RC
