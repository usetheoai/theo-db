#!/usr/bin/env python3
"""M169 — prova de BYTE-IDENTIDADE das consultas que saíram de erro para `ok`.

Existe porque `m169_baseline_run.py` mede **conclusão**, não correção: ele declara o campo `ab_identical` e
nunca o popula, então a corrida de 43 consultas responde "terminou?" e não "está certo?". O DoD do M169 pede as
duas coisas, e tratar uma pela outra seria aceitar um verde sem execução.

O que este script NÃO faz, deliberadamente:

- **não mede tempo.** `run_m128_clickbench._bench_query` roda cada consulta 5× (3 de timing + 2 do A/B); a 100M
  isso é ~25 min só para a q32. Aqui o objeto é correção, então cada lado roda UMA vez.
- **não toca nas tabelas.** `run_m128_clickbench.run()` DROPA `hits` e `hits_heap` — o gêmeo heap custou 56 min
  para materializar (COPY 1796 s + SET LOGGED 1561 s). Este script só lê.

O que ele REUSA de `run_m128_clickbench` (Regra 9 — não reinventar o oráculo):

- `_canonical` — forma canônica insensível à ordem;
- o regex que remove o `LIMIT N` final. Sem isso a comparação é falso-negativa: as consultas do ClickBench com
  `ORDER BY count DESC LIMIT 10` têm empates, e o corte escolhe 10 linhas ARBITRÁRIAS entre as válidas. A
  diferença seria de ordem de varredura, não de armazenamento.

Uso:
    PGDATABASE=postgres python3 benchmarks/m169_ab_verify.py --queries 20,32,33,34 \\
        --timeout-ms 3600000 --out docs/benchmarks/m169-ab-verify.md
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import psycopg2  # noqa: E402

from run_m128_clickbench import _canonical, _load_queries, LIMIT_RE  # noqa: E402


def _conn(timeout_ms: int):
    c = psycopg2.connect(
        dbname=os.environ.get("PGDATABASE", "postgres"),
        user=os.environ.get("PGUSER", "postgres"),
        host=os.environ.get("PGHOST", "/tmp"),
        port=int(os.environ.get("PGPORT", "5432")),
    )
    c.autocommit = True
    with c.cursor() as cur:
        # O lado HEAP varre 66 GB; com o teto de 300 s da corrida de benchmark ele nunca terminaria, e um
        # timeout aqui se leria como "não deu para provar" quando na verdade é só orçamento de tempo.
        cur.execute(f"SET statement_timeout = {int(timeout_ms)}")
    return c


def verify_one(cur, i: int, sql: str) -> dict:
    """Compara colunar (`hits`) vs heap (`hits_heap`) para UMA consulta. Sem LIMIT, sem timing."""
    rec = {"q": i, "identical": None, "rows_columnar": None, "rows_heap": None, "note": None}
    ab_sql = LIMIT_RE.sub("", sql.rstrip().rstrip(";"))
    try:
        t0 = time.perf_counter()
        cur.execute(ab_sql)
        rc = _canonical(cur.fetchall())
        rec["elapsed_columnar_s"] = round(time.perf_counter() - t0, 2)

        t0 = time.perf_counter()
        # `hits_heap` é o MESMO dado, carregado pelo mesmo COPY — a substituição textual do nome da tabela é o
        # que o oráculo do M128 já fazia; nas 43 consultas do ClickBench `hits` só aparece como nome de tabela.
        cur.execute(ab_sql.replace("hits", "hits_heap"))
        rh = _canonical(cur.fetchall())
        rec["elapsed_heap_s"] = round(time.perf_counter() - t0, 2)

        rec["rows_columnar"], rec["rows_heap"] = len(rc), len(rh)
        rec["identical"] = rc == rh
        if not rec["identical"]:
            # A primeira divergência concreta, não só "diferente" — um veredito sem exemplo não é acionável.
            rec["diverged"] = sum(1 for x, y in zip(rc, rh) if x != y) + abs(len(rc) - len(rh))
            for x, y in zip(rc, rh):
                if x != y:
                    rec["first_diff"] = {"columnar": x[:4], "heap": y[:4]}
                    break
    except Exception as e:  # noqa: BLE001 — qualquer falha vira nota honesta, nunca um verde
        rec["note"] = str(e).splitlines()[0][:160]
    return rec


def render(recs: list[dict]) -> str:
    proved = [r for r in recs if r["identical"] is True]
    failed = [r for r in recs if r["identical"] is False]
    unknown = [r for r in recs if r["identical"] is None]
    out = [
        "# M169 — byte-identidade colunar vs heap (100M)",
        "",
        "Prova de **correção**, complementar à corrida de conclusão. Cada lado roda UMA vez, sem `LIMIT`",
        "(empates escolheriam linhas arbitrárias entre válidas) e sem medição de tempo.",
        "",
        f"- idênticas: **{len(proved)}/{len(recs)}**",
        f"- divergentes: **{len(failed)}**",
        f"- não verificadas: **{len(unknown)}**",
        "",
        "| q | idêntico | linhas (colunar / heap) | colunar s | heap s | nota |",
        "|---|---|---|---|---|---|",
    ]
    for r in recs:
        mark = {True: "**sim**", False: "**NÃO**", None: "—"}[r["identical"]]
        out.append(
            f"| q{r['q']:02d} | {mark} | {r['rows_columnar']} / {r['rows_heap']} | "
            f"{r.get('elapsed_columnar_s', '—')} | {r.get('elapsed_heap_s', '—')} | {r['note'] or ''} |"
        )
    if unknown:
        out += ["", "> Uma consulta NÃO verificada não é uma consulta correta. O que impediu a verificação está",
                "> na coluna nota, e o veredito permanece em aberto."]
    return "\n".join(out) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--queries", required=True, help="índices separados por vírgula, ex.: 20,32,33,34")
    ap.add_argument("--timeout-ms", type=int, default=3_600_000)
    ap.add_argument("--out", default=None)
    ap.add_argument("--json-out", default=None)
    a = ap.parse_args()

    idx = [int(x) for x in a.queries.split(",") if x.strip()]
    queries = _load_queries()
    conn = _conn(a.timeout_ms)
    recs = []
    with conn.cursor() as cur:
        for i in idx:
            print(f"  q{i:02d} …", flush=True)
            r = verify_one(cur, i, queries[i])
            print(f"  q{i:02d} identical={r['identical']} note={r['note'] or ''}", flush=True)
            recs.append(r)

    text = render(recs)
    if a.out:
        os.makedirs(os.path.dirname(a.out), exist_ok=True)
        with open(a.out, "w") as fh:
            fh.write(text)
    if a.json_out:
        with open(a.json_out, "w") as fh:
            json.dump(recs, fh, indent=2)
    print(text)

    # Exit != 0 quando QUALQUER consulta não ficou provada idêntica — inclusive as não verificadas. "Não deu para
    # provar" e "está certo" são estados diferentes, e só um deles autoriza seguir.
    return 0 if all(r["identical"] is True for r in recs) else 1


if __name__ == "__main__":
    raise SystemExit(main())
