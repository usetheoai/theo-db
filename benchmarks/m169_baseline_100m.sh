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
ART="${ART_DIR:-docs/benchmarks/m169-artifacts}"
TSV="${TSV_PATH:-/root/theo-db/benchmarks/.cache/hits_sample.tsv}"
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
exit $RUN_RC
