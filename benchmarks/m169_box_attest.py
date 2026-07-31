"""M169 box attestation — the executable gate behind T1.1's acceptance criteria.

Three layers, deliberately separated:

- `attest()` is PURE — it judges a `BoxFacts` and returns a verdict. That is what makes it a regression test
  (`test_m169_box_attest.py`) instead of a check that only runs on the right machine. Same shape as the M164
  guards in `run_m128_clickbench.py`.
- `collect()` MAPS the world into `BoxFacts` — which command, which timeout, which default, which parse. This
  layer is where every defect of this file has lived, so it takes its shell/psql runners as injected callables
  and IS unit-tested with fakes.
- `_sh` / `_psql_int` are the only code that touches a real kernel. Their shell strings' correctness against a
  real system is what genuinely cannot be unit-tested — not "collect is I/O, therefore untestable".

Why an attestation and not a provisioning script: the box already exists. A script that creates a droplet, which
nobody ran and nobody will run, is retroactive documentation dressed as an artifact. What IS needed is a header
that Phase 1 and Phase 4 both emit, because ADR-3 requires them to run on the same box and a header nobody can
compare cannot prove that.

Usage (on the box):

    python3 benchmarks/m169_box_attest.py            # human-readable header, exit != 0 when the box is wrong
    python3 benchmarks/m169_box_attest.py --json     # machine-readable, for pasting into an artifact
"""
from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import sys
from dataclasses import asdict, dataclass, field

# The published ClickBench corpus. NOT 100_000_000 — `hits.tsv` has 99,997,497 rows, and `wc -l` on the loaded TSV
# measured exactly that. An acceptance criterion written as the round number cannot be satisfied by the data.
# Duplicated from `run_m128_clickbench.py:77` on purpose: importing it would drag `psycopg2` into a pure-logic
# test. `test_m169_box_attest.py` pins the two literals against each other so the duplication cannot drift.
HITS_ROWS = 99_997_497

# ADR-3 declared the box: 16 vCPU / 32 GB. `free -g` truncates, so 32 GB of RAM reports as 31.
MIN_NPROC = 16
MIN_MEM_GB = 30
# A benchmark started on a busy box measures the box, not the code. Inherited from the plan's AC ("< 2"); the plan
# does not derive WHY 2 — on 16 vCPUs it is ~12.5% utilisation. Kept as a plan-inherited number, not a measured one.
MAX_LOADAVG1 = 2.0
# Rebuilding `hits_heap` writes the heap twin AND its columnar derivative. Derived from the project's canonical
# sizing policy (`run_m128_clickbench.py` EST_*_BYTES_PER_ROW) rather than picked round, so the two do not disagree.
# The TSV is NOT counted: it already exists on disk and is not rewritten.
_EST_HEAP_BYTES_PER_ROW = 1000
_EST_COLUMNAR_BYTES_PER_ROW = 150
MIN_DISK_FREE_GB = int(HITS_ROWS * (_EST_HEAP_BYTES_PER_ROW + _EST_COLUMNAR_BYTES_PER_ROW) / 1024**3 * 1.12)

# `wc -l` over the 69.7 GB corpus. 60 s would imply >1.1 GB/s sustained and time out on a CONFORMING box, whose
# fail-closed default then accuses the corpus of being the wrong size. `run_m128_clickbench.py:190` runs the
# identical command on the identical file with the same ceiling.
WC_TIMEOUT_S = 600
SH_TIMEOUT_S = 60
# `count(*)` over the 100M columnar table takes TENS OF MINUTES (row-by-row materialisation, the M148
# bottleneck). Deliberately not stated as a precise figure: both runs observed so far overlapped with another
# query for part of their window, so the order of magnitude is solid and the exact number is not. The ceiling
# below is sized from the order of magnitude, which is all a ceiling needs.
# The 60 s default kills the psql CLIENT and leaves the BACKEND running orphaned — the check then reports
# UNREACHABLE while the server burns CPU for another half hour. That is the same defect the `wc -l` ceiling fixed;
# fixing one instance and not the class is how it survived.
COUNT_TIMEOUT_S = 3600


@dataclass(frozen=True)
class BoxFacts:
    """One place the environment is named. `attest` judges it and the artifact header prints it, so a field can
    never be checked-but-unpublished (which would silently break ADR-3's same-box comparison at T4.1)."""
    nproc: int
    mem_gb: int
    loadavg1: float
    unattended_state: str
    hits_rows: int
    hits_heap_rows: int
    tsv_rows: int
    disk_free_gb: int
    so_md5: str = "unknown"
    data_directory: str = "unknown"
    # M169 T4.1 — o ORÇAMENTO DE DESCRITORES entrou aqui depois de uma regressão medida: q08/q09
    # (`COUNT(DISTINCT`) falharam com `EMFILE` no spill do DataFusion enquanto havia 205 GB de disco livres. A
    # causa não é memória nem disco: o PostgreSQL pode segurar até `max_files_per_process` (1000) dentro de um
    # soft limit de 1024, deixando folga quase nula para arquivos abertos fora do gerenciador de VFD dele.
    # Sem estes dois campos no cabeçalho, "30/43" é um número NÃO REPRODUZÍVEL e a causa-raiz fica invisível
    # para quem tentar repetir a corrida noutra caixa.
    # Default = -1, o MESMO valor de `UNREACHABLE` (definido abaixo, depois desta classe — por isso o
    # literal aqui em vez do nome). Significa 'não consegui ler', nunca 'sem limite'.
    ulimit_nofile_soft: int = -1
    max_files_per_process: int = -1


@dataclass
class Verdict:
    ok: bool
    failures: list[str] = field(default_factory=list)
    facts: dict = field(default_factory=dict)


# A collector that could not run its command reports this instead of a number. It exists so an infrastructure
# failure is never reported as a data defect — the #132 lesson (a generic error erasing the cause).
UNREACHABLE = -1
# The dataset checks were deliberately not run (see `collect(quick=True)`). Distinct from UNREACHABLE, which
# means they were attempted and failed, and from 0, which means the table is empty.
SKIPPED = -2
# The relation does not exist. Distinct from 0 (exists, empty) and from UNREACHABLE (could not ask): a caller
# deciding whether to tolerate a missing heap twin must be able to tell "absent" from "I could not look".
ABSENT = -3

# Stable identifiers, so a caller tests an ID and never a substring of prose. Same contract as
# `code-quality-golden-rule.md § 2` and its `hard_caps_triggered`.
ID_UNREACHABLE = "collector_unreachable"
ID_TSV_WRONG = "tsv_rowcount_wrong"
ID_COPY_LOST = "hits_rowcount_disagrees_with_tsv"
ID_HEAP_ABSENT = "hits_heap_absent"
ID_HEAP_MISMATCH = "hits_heap_rowcount_mismatch"


def attest(box: BoxFacts) -> Verdict:
    """Judge one box against T1.1's acceptance criteria. Reports EVERY failure, never just the first — a
    first-failure-wins guard hides work and makes the operator re-run to discover the next problem.

    Each failure carries a STABLE ID as `id | prose`. A caller that greps the prose (as an earlier version of
    `m169_baseline_100m.sh` did) matches whatever sentence happens to contain a word — and `hits_heap is ABSENT`
    and `hits_heap_rows disagrees` both contain `hits_heap`, so a script meaning to tolerate the first silently
    tolerated the second: a heap twin with the WRONG population, which is worse than none."""
    failures: list[str] = []

    def fail(fid: str, prose: str) -> None:
        failures.append(f"{fid} | {prose}")

    if SKIPPED in (box.tsv_rows, box.hits_rows, box.hits_heap_rows):
        pass  # quick mode: the dataset was not asked about, so it is not judged. Box fitness below still is.
    else:
        if UNREACHABLE in (box.tsv_rows, box.hits_rows, box.hits_heap_rows):
            fail(ID_UNREACHABLE, "could not read the corpus or the database — cannot attest the dataset "
                                 "(check psql reachability, LD_LIBRARY_PATH, the TSV path, and the timeout) "
                                 "— NOT a data defect")
        if box.tsv_rows not in (UNREACHABLE,) and box.tsv_rows != HITS_ROWS:
            fail(ID_TSV_WRONG, f"tsv_rows={box.tsv_rows} but the ClickBench corpus has {HITS_ROWS}")
        if box.hits_rows not in (UNREACHABLE,) and box.tsv_rows not in (UNREACHABLE,) \
                and box.hits_rows != box.tsv_rows:
            fail(ID_COPY_LOST, f"hits_rows={box.hits_rows} disagrees with tsv_rows={box.tsv_rows} — the COPY "
                               "lost or duplicated rows")
        if box.hits_heap_rows in (ABSENT, 0):
            fail(ID_HEAP_ABSENT, "hits_heap is ABSENT — T2.1/T4.1 have no byte-identity oracle without the twin")
        elif box.hits_heap_rows != UNREACHABLE and box.hits_rows not in (UNREACHABLE,) \
                and box.hits_heap_rows != box.hits_rows:
            fail(ID_HEAP_MISMATCH, f"hits_heap_rows={box.hits_heap_rows} disagrees with hits_rows="
                                   f"{box.hits_rows} — the A/B would compare two different populations")

    if box.nproc < MIN_NPROC:
        fail("nproc_below_adr3", f"nproc={box.nproc} below the {MIN_NPROC} ADR-3 declared")
    if box.mem_gb < MIN_MEM_GB:
        fail("mem_below_adr3", f"mem_gb={box.mem_gb} below the {MIN_MEM_GB} ADR-3 declared")
    if box.loadavg1 >= MAX_LOADAVG1:
        fail("box_busy", f"loadavg1={box.loadavg1} >= {MAX_LOADAVG1} — something else is running; the "
                         "measurement would be contaminated")
    if box.unattended_state != "masked":
        fail("unattended_upgrades_live", f"unattended-upgrades is '{box.unattended_state}', not masked — it "
                                         "restarts PostgreSQL mid-run")
    if box.disk_free_gb < MIN_DISK_FREE_GB:
        fail("disk_insufficient", f"disk_free_gb={box.disk_free_gb} below the {MIN_DISK_FREE_GB} needed")

    return Verdict(ok=not failures, failures=failures, facts=asdict(box))


def _sh_any_rc(cmd: str, timeout: int = SH_TIMEOUT_S) -> str | None:
    """For commands whose EXIT CODE encodes an answer rather than a failure.

    `systemctl is-enabled` returns 1 for `masked` and for `disabled` — the state IS the stdout, and the exit code
    merely restates it. Treating non-zero as "could not run" turned a correctly-masked unit into `unknown`, which
    then failed the gate for the wrong reason. This is the third time in this milestone that a strict returncode
    check mislabelled a working command; the fix is not to loosen `_sh` (whose strictness is what keeps "psql is
    down" from becoming "the COPY lost rows") but to be explicit about which commands answer via exit code."""
    try:
        r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout)
        return (r.stdout.strip() or r.stderr.strip()) or None
    except (subprocess.SubprocessError, OSError):
        return None


def _sh(cmd: str, timeout: int = SH_TIMEOUT_S) -> str | None:
    """Run a shell command. Returns None when it could NOT run (non-zero exit, timeout, spawn failure) — the
    caller must distinguish that from a command that ran and produced '0'. Returning a magic number here is what
    turns 'psql is down' into 'the COPY lost rows'."""
    try:
        r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout)
        return r.stdout.strip() if r.returncode == 0 else None
    except (subprocess.SubprocessError, OSError):
        return None


def _psql(sql: str, timeout: int = SH_TIMEOUT_S) -> str | None:
    """Every interpolated value is shell-quoted: `run_m128_clickbench.py:214` carries an explicit CWE-78 guard for
    a less-exposed value, and undercutting the project's own standard 20 lines away is not a defensible shortcut."""
    q = shlex.quote
    return _sh(f'sudo -u {q(os.environ.get("PGOSUSER", "pgtest"))} env LD_LIBRARY_PATH=/opt/pg18/lib '
               f'/opt/pg18/bin/psql -p {q(os.environ.get("PGPORT", "5432"))} '
               f'-U {q(os.environ.get("PGUSER", "postgres"))} -d {q(os.environ.get("PGDATABASE", "postgres"))} '
               f'-Atc {q(sql)}', timeout)


def _psql_int(sql: str, timeout: int = SH_TIMEOUT_S) -> int:
    """Three outcomes, deliberately distinct: a number, ABSENT, or UNREACHABLE. Collapsing them is how "psql is
    down" became "the COPY lost rows" — the #132 defect."""
    out = _psql(sql, timeout)
    if out is None:
        return UNREACHABLE
    # A ÚLTIMA linha não-vazia, não a primeira. Um `SET …;` antes do `select` imprime o tag `SET` em linha
    # própria, e `int("SET")` devolveria UNREACHABLE — que se lê como "o psql caiu" em vez de "o número está na
    # linha seguinte". É a mesma armadilha que fez o guard da recarga abortar. Para uma consulta de valor único
    # sem prefixo, primeira e última coincidem, então isto não muda nenhum chamador existente.
    lines = [ln for ln in out.splitlines() if ln.strip()]
    try:
        return int(lines[-1])
    except (ValueError, IndexError):
        return UNREACHABLE


def _count_rows(relation: str) -> int:
    """Probe existence first with `to_regclass`, which CANNOT fail on a missing relation (it returns NULL, exit
    0). Counting a nonexistent table exits non-zero, which is indistinguishable from psql being unreachable —
    and that ambiguity made the `ALLOW_MISSING_HEAP` escape hatch inert in exactly its own use case."""
    exists = _psql(f"select to_regclass('{relation}') is not null")
    if exists is None:
        return UNREACHABLE
    if exists.strip() != "t":
        return ABSENT
    # `SET theodb.enable_columnar_agg = on` primeiro. MEDIDO 2026-07-31 sobre as mesmas 99.997.497 linhas:
    # 11,4 s com o pushdown contra >948 s sem ele (backend a 99,9% de CPU, zero wait events — materialização
    # linha a linha, não I/O). Sem isto a atestação sozinha custa dezenas de minutos ANTES da medição começar.
    #
    # A captura toma a ÚLTIMA linha porque o `SET` imprime o próprio tag numa linha separada — a mesma armadilha
    # que fez o guard da recarga abortar quando apliquei este fix lá. Quarta vez nesta sessão que corrijo a
    # instância e não a classe: o guard foi consertado e ESTE caminho ficou para trás.
    return _psql_int(f"SET theodb.enable_columnar_agg = on; select count(*) from {relation}", COUNT_TIMEOUT_S)


def _int_pre(sh, cmd: str, default: int = 0) -> int:
    out = sh(cmd, SH_TIMEOUT_S)
    try:
        return int(out.split()[0]) if out else default
    except (ValueError, IndexError):
        return default


def collect(tsv_path: str, *, sh=_sh, sh_any_rc=_sh_any_rc, psql_int=_count_rows,
            quick: bool = False) -> BoxFacts:
    """Map the world into `BoxFacts`. The runners are injected so this mapping — the layer where every defect of
    this file has lived — is unit-testable without a box or a database.

    ASSUMPTION, stated because being silent about it is the `instrumento-cego-a-arquitetura` failure: `df` is run
    against PGDATA's filesystem, which is where the heap rebuild actually writes. The TSV cache is assumed to be
    on the same mount; if it is not, its filesystem is unchecked.

    `quick=True` skips the dataset checks (`count(*)` on 100M is tens of minutes, and `wc -l` on the 69.7 GB
    corpus is minutes — see the ceiling constants for why neither figure is stated precisely). It exists for the CLOSING header of a read-only run, whose question is "did anything run
    alongside?" — not "is the data still there?", which a read-only run cannot have changed. Using it for the
    OPENING header would skip the very checks T1.1 exists to make, so the two are not interchangeable and the
    resulting facts say which mode produced them."""
    def _int(cmd: str, timeout: int = SH_TIMEOUT_S, default: int = 0) -> int:
        out = sh(cmd, timeout)
        try:
            return int(out.split()[0]) if out else default
        except (ValueError, IndexError):
            return default

    # ORDER MATTERS, and it is not cosmetic. The dataset checks below are ~40 minutes of CPU-bound work; reading
    # `loadavg` AFTER them would measure the attestation's OWN footprint and then blame the box for it — the
    # instrument perturbing what it measures. Every environment fact is therefore sampled BEFORE the expensive
    # work, so `loadavg1` answers "was the box busy when we started?", which is the question the gate asks.
    env_nproc = _int_pre(sh, "nproc")
    env_mem_gb = _int_pre(sh, "free -g | awk '/^Mem:/{print $2}'")
    env_loadavg1 = float(sh("cut -d' ' -f1 /proc/loadavg", SH_TIMEOUT_S) or "99")
    env_unattended = sh_any_rc("systemctl is-enabled unattended-upgrades", SH_TIMEOUT_S) or "unknown"

    ddir = _psql_text(sh, "show data_directory") or "/"
    # SKIPPED is a distinct value from 0 and from UNREACHABLE: it says "not asked", which `attest` must not
    # mistake for "asked and found nothing".
    dataset = (SKIPPED, SKIPPED, SKIPPED) if quick else (
        psql_int("public.hits"),
        psql_int("public.hits_heap"),
        _int(f"wc -l < {shlex.quote(tsv_path)}", WC_TIMEOUT_S, default=UNREACHABLE),
    )
    return BoxFacts(
        nproc=env_nproc,
        mem_gb=env_mem_gb,
        loadavg1=env_loadavg1,
        unattended_state=env_unattended,
        hits_rows=dataset[0],
        hits_heap_rows=dataset[1],
        tsv_rows=dataset[2],
        disk_free_gb=_int(f"df -BG --output=avail {shlex.quote(ddir)} | tail -1 | tr -dc '0-9'", default=0),
        so_md5=sh("md5sum /opt/pg18/lib/postgresql/theodb_rs.so | cut -c1-32", SH_TIMEOUT_S) or "unknown",
        data_directory=ddir,
        # Lido do POSTMASTER, não do shell da atestação: quem abre os arquivos de spill é o backend, e o shell
        # pode ter um limite diferente. `head -1` do postmaster.pid é o PID do postmaster.
        ulimit_nofile_soft=_int(
            f"awk '/^Max open files/{{print $4}}' /proc/$(head -1 {shlex.quote(ddir)}/postmaster.pid)/limits",
            SH_TIMEOUT_S, default=UNREACHABLE),
        max_files_per_process=_as_int(
            _psql_text(sh, "show max_files_per_process"), default=UNREACHABLE),
    )


def _as_int(text: str | None, *, default: int) -> int:
    """Converte a saída de um `SHOW` em int. Devolve `default` (UNREACHABLE) quando a leitura falhou — nunca 0,
    que num campo de LIMITE se leria como 'sem descritor nenhum' em vez de 'não perguntei'."""
    try:
        return int((text or "").strip().split()[0])
    except (ValueError, IndexError):
        return default


def _psql_text(sh, sql: str) -> str | None:
    """`SHOW data_directory` through the injected runner, so `collect` stays testable end-to-end."""
    q = shlex.quote
    return sh(f'sudo -u {q(os.environ.get("PGOSUSER", "pgtest"))} env LD_LIBRARY_PATH=/opt/pg18/lib '
              f'/opt/pg18/bin/psql -p {q(os.environ.get("PGPORT", "5432"))} '
              f'-U {q(os.environ.get("PGUSER", "postgres"))} -d {q(os.environ.get("PGDATABASE", "postgres"))} '
              f'-Atc {q(sql)}', SH_TIMEOUT_S)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tsv", default="/srv/bench-data/hits_sample.tsv",
                    help="the loaded corpus; its line count is the authority the table count is checked against")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--quick", action="store_true",
                    help="skip the dataset checks (tens of minutes at 100M); for the CLOSING header of a "
                         "read-only run, whose question is contamination, not dataset integrity")
    args = ap.parse_args()

    v = attest(collect(args.tsv, quick=args.quick))
    if args.json:
        print(json.dumps({"ok": v.ok, "failures": v.failures, "facts": v.facts}, indent=1))
    else:
        print("=== M169 box attestation ===")
        for k, val in v.facts.items():
            print(f"  {k:>16} = {val}")
        print("  VERDICT: OK" if v.ok else "  VERDICT: FAIL")
        for fail in v.failures:
            print(f"    - {fail}")
    return 0 if v.ok else 1


if __name__ == "__main__":
    sys.exit(main())
