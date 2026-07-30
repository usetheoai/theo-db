"""M169 baseline summarizer — the non-vacuity gate over the 100M ClickBench run.

T1.2's entire output is one number: how many of the 43 ClickBench queries complete at 100M. That number is only
meaningful if the run REACHED all 43. A run that dies halfway publishes "19/43 completam" that reads identically
to "19 completed and 24 genuinely failed" — the first is a fact about the harness, the second about the product,
and nothing downstream can tell them apart.

The M162 run was OOM-killed at 24/43, leaving 19 queries never executed. This gate exists so that shape fails
loudly instead of becoming a headline.

Two design decisions worth stating, because both are places where guessing is tempting:

- **A dropped connection is `crash`, not `oom`.** A backend that vanished MIGHT have been OOM-killed — or
  segfaulted, or been terminated by an operator. Upgrading to `oom:kernel` requires a kernel line naming that
  backend's PID, passed in explicitly. Naming the cause without reading the log is a diagnosis invented to fit
  the narrative — and `oom:palloc` (SQLSTATE 53200, the backend SURVIVED) is a different fact with a different
  remediation, so the two never share a bucket.
- **Zero A/B comparisons is `n/a`, never "byte-identical".** `run_m128_clickbench.decide_ab_verdict` publishes
  "byte-identical (columnar==heap)" when every `result_ab_identical` is None — which is exactly what happens when
  `hits_heap` is absent. Zero comparisons is not agreement.

Usage:

    python3 benchmarks/m169_baseline_summarize.py docs/benchmarks/m169-artifacts/baseline.json   # exit != 0 on a
                                                                                                 # vacuous run
"""
from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field

TOTAL_QUERIES = 43

# The closed verdict vocabulary. An unrecognised token is a harness defect, not a bucket to widen silently.
#
# The plan's AC declares four states (`ok|error|timeout|oom`); six are needed, because `oom` is not one thing and
# two of the shapes are NOT attributable to the query that observed them:
#
#   ok                — no exception
#   timeout           — SQLSTATE 57014, the statement hit `statement_timeout`
#   oom:palloc        — SQLSTATE 53200, PostgreSQL caught the allocation failure; the backend SURVIVED
#   error:<sqlstate>  — any other SQLSTATE
#   crash             — the backend died; the cause is NOT yet known
#   oom:kernel        — `crash` PLUS a kernel line naming that backend's PID
#   crash:collateral  — SQLSTATE 57P02: some OTHER backend crashed and this one was recycled. Attributing this
#                       to the current query would blame it for a neighbour's death.
OK = "ok"
TIMEOUT = "timeout"
CRASH = "crash"
CRASH_COLLATERAL = "crash:collateral"
OOM_PALLOC = "oom:palloc"
OOM_KERNEL = "oom:kernel"
_ERROR_PREFIX = "error:"
# The runner failed, not the product. A distinct namespace so a harness defect is never counted as a query
# failure — and so `_is_known` does not reject the whole run for a token it has never seen.
HARNESS_PREFIX = "harness:"

# PostgreSQL SQLSTATEs. 57014 = query_canceled, 53200 = out_of_memory, 57P02 = crash_shutdown.
SQLSTATE_QUERY_CANCELED = "57014"
SQLSTATE_OUT_OF_MEMORY = "53200"
SQLSTATE_CRASH_SHUTDOWN = "57P02"


@dataclass
class Summary:
    ok: bool
    reason: str = ""
    completed: int = 0
    total_expected: int = TOTAL_QUERIES
    by_verdict: dict = field(default_factory=dict)
    ab_verdict: str = ""


def classify_failure(sqlstate: str | None, message: str, *, oom_evidence: bool = False) -> str:
    """Map one failure to the closed vocabulary.

    `oom_evidence` must come from the KERNEL log naming this backend's PID — never from the message text. The
    client sees the same 'server closed the connection unexpectedly' for an OOM kill, a segfault and an operator
    terminate; picking one of the three because it fits the story is a diagnosis, not a measurement."""
    if sqlstate == SQLSTATE_QUERY_CANCELED:
        return TIMEOUT
    if sqlstate == SQLSTATE_OUT_OF_MEMORY:
        return OOM_PALLOC
    if sqlstate == SQLSTATE_CRASH_SHUTDOWN:
        return CRASH_COLLATERAL
    if sqlstate:
        return f"{_ERROR_PREFIX}{sqlstate}"
    return OOM_KERNEL if oom_evidence else CRASH


def _is_known(verdict: str) -> bool:
    return (verdict in (OK, TIMEOUT, CRASH, CRASH_COLLATERAL, OOM_PALLOC, OOM_KERNEL)
            or verdict.startswith(_ERROR_PREFIX) or verdict.startswith(HARNESS_PREFIX))


def _ab_verdict(records: list[dict]) -> str:
    """Refuse the agreement headline when nothing was compared."""
    compared = [r.get("ab_identical") for r in records if r.get("ab_identical") is not None]
    if not compared:
        return "n/a — nenhuma comparação executada"
    diverged = sum(1 for c in compared if c is False)
    if diverged:
        return f"DIVERGIU em {diverged} de {len(compared)} comparações"
    return f"byte-identical (columnar==heap) em {len(compared)} comparações"


def summarize(records: list[dict], total_expected: int = TOTAL_QUERIES) -> Summary:
    """Judge one baseline run. Completeness is about COVERAGE of verdicts — a run where 40 of 43 queries fail is
    a valid measurement of a bad state; a run missing 19 verdicts is not a measurement at all."""
    if len(records) != total_expected:
        return Summary(ok=False, total_expected=total_expected,
                       reason=f"run incompleto: {len(records)}/{total_expected} consultas com registro")

    ids = [r.get("q") for r in records]
    if len(set(ids)) != len(ids):
        dupes = sorted({i for i in ids if ids.count(i) > 1})
        return Summary(ok=False, total_expected=total_expected,
                       reason=f"run incompleto: ids duplicados {dupes} — uma consulta nunca rodou")

    missing = [r.get("q") for r in records if not r.get("verdict")]
    if missing:
        return Summary(ok=False, total_expected=total_expected,
                       reason=f"run incompleto: {len(missing)}/{total_expected} consultas sem veredito {missing}")

    unknown = sorted({r["verdict"] for r in records if not _is_known(r["verdict"])})
    if unknown:
        return Summary(ok=False, total_expected=total_expected,
                       reason=f"veredito desconhecido {unknown} — defeito do harness, não do produto")

    by_verdict: dict[str, int] = {}
    for r in records:
        by_verdict[r["verdict"]] = by_verdict.get(r["verdict"], 0) + 1

    return Summary(ok=True, completed=by_verdict.get(OK, 0), total_expected=total_expected,
                   by_verdict=by_verdict, ab_verdict=_ab_verdict(records))


def load_jsonl(path: str) -> tuple[dict, list[dict]]:
    """Read what the runner ACTUALLY writes: one JSON document per line, the first carrying `header`.

    An earlier version used `json.load` — a single-document parser — and would have raised
    `JSONDecodeError: Extra data: line 2` AFTER the multi-hour run. Worse, a naive line reader would count the
    header as a 44th record with no verdict and fail a perfectly good run. Both are caught by the round-trip
    test, which is the only one that crosses the file boundary."""
    header, records = {}, []
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            if "header" in obj:
                header = obj["header"]
            else:
                records.append(obj)
    return header, records


def main() -> int:
    if len(sys.argv) != 2:
        print(f"uso: {sys.argv[0]} <baseline.jsonl>", file=sys.stderr)
        return 2
    header, records = load_jsonl(sys.argv[1])
    # The denominator comes from the run, not from a constant: with `--queries` pointing at another set, a fixed
    # 43 would fail the gate by construction and make "N/43" incomparable.
    expected = header.get("n_queries", TOTAL_QUERIES)

    r = summarize(records, total_expected=expected)
    if header:
        print(f"  label={header.get('label')} timeout={header.get('timeout_s')}s "
              f"work_mem={header.get('work_mem')}")
        ph = [k for k, v in (header.get("gucs_effective") or {}).items() if str(v).startswith("PLACEHOLDER")]
        if ph:
            print(f"  ATENÇÃO: GUCs que o servidor não conhece (SET sem efeito): {ph}")
    print(f"=== M169 baseline — {r.completed}/{r.total_expected} consultas completam ===")
    for verdict, n in sorted(r.by_verdict.items()):
        print(f"  {verdict:>18} : {n}")
    print(f"  A/B: {r.ab_verdict}")
    if not r.ok:
        print(f"  GATE FAIL: {r.reason}")
    return 0 if r.ok else 1


if __name__ == "__main__":
    sys.exit(main())
