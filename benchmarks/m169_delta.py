"""M169 T4.1 — render the delta between the baseline (T1.2) and the post-fix re-run (T4.1).

The milestone's headline is one subtraction, and a subtraction is only meaningful when everything except the
binary held still. This module's load-bearing part is therefore what it REFUSES: a differing timeout ceiling, a
differing box, a differing corpus, or the SAME binary on both sides. Each of those turns the difference into a
number about the environment wearing the label of a number about the code — which is precisely what the T1.2
artifact spends a section warning against, using the M162 run as the example.

It also separates gains that routed through the aggregate from gains that did not. A query that completes without
ever entering the columnar aggregate path is not evidence of this fix, and counting it inflates the claim.
"""
from __future__ import annotations

import json
import sys

TOTAL_QUERIES = 43


class IncomparableRuns(Exception):
    """Raised instead of publishing a difference between runs that were not made under the same conditions."""


def _facts(box: dict) -> dict:
    return box.get("facts", box)


def _ok(records: list[dict]) -> set[int]:
    return {r["q"] for r in records if r.get("verdict") == "ok"}


def _routed_failures(records: list[dict]) -> dict[int, dict]:
    return {r["q"]: r for r in records if r.get("verdict") != "ok" and r.get("agg_routed")}


def _check_comparable(hb: dict, ha: dict, fb: dict, fa: dict) -> None:
    if hb.get("timeout_s") != ha.get("timeout_s"):
        raise IncomparableRuns(
            f"teto de tempo diferente ({hb.get('timeout_s')}s vs {ha.get('timeout_s')}s): completar sob tetos "
            "distintos são medições distintas, e a subtração reportaria a mudança do teto como se fosse o fix.")
    for key, human in (("nproc", "box"), ("mem_gb", "box"), ("data_directory", "box")):
        if fb.get(key) != fa.get(key):
            raise IncomparableRuns(
                f"a box mudou entre as corridas ({key}: {fb.get(key)!r} vs {fa.get(key)!r}). O ADR-3 exige as duas "
                "fases na MESMA máquina; senão o delta mistura efeito de código com efeito de máquina, e nenhum "
                "dos dois se isola depois.")
    if fb.get("hits_rows") != fa.get("hits_rows"):
        raise IncomparableRuns(
            f"o corpus mudou ({fb.get('hits_rows')} vs {fa.get('hits_rows')} linhas) — a comparação não é sobre "
            "o mesmo dado.")
    if fb.get("so_md5") and fb.get("so_md5") == fa.get("so_md5"):
        raise IncomparableRuns(
            f"o binário é o MESMO nas duas corridas (so_md5={fb.get('so_md5')[:12]}). A alegação inteira é que o "
            "fix moveu o número; medir o mesmo `.so` duas vezes mede a variância da box, e o resultado se lê "
            "como 'o fix não fez nada'.")


def render(before: tuple[dict, list[dict], dict], after: tuple[dict, list[dict], dict]) -> str:
    hb, rb, bb = before
    ha, ra, ba = after
    fb, fa = _facts(bb), _facts(ba)
    _check_comparable(hb, ha, fb, fa)

    ok_b, ok_a = _ok(rb), _ok(ra)
    gained = sorted(ok_a - ok_b)
    lost = sorted(ok_b - ok_a)
    routed_before = _routed_failures(rb)
    # Um ganho SÓ conta para este milestone se a consulta falhava NO caminho agregado antes. As demais melhoram
    # por outra razão (ou por ruído), e creditá-las aqui infla a alegação.
    gained_routed = [q for q in gained if q in routed_before]
    gained_other = [q for q in gained if q not in routed_before]
    delta = len(ok_a) - len(ok_b)

    lines = [
        f"# M169 — delta medido: {len(ok_b)}/{TOTAL_QUERIES} → {len(ok_a)}/{TOTAL_QUERIES} "
        f"({'+' if delta >= 0 else ''}{delta})",
        "",
        "Medição de **conclusão**, não de velocidade: o critério é *a consulta termina* sob o mesmo teto.",
        "",
        "## O que ficou constante — sem isto a subtração não significa nada",
        "",
        "| | antes (T1.2) | depois (T4.1) |",
        "|---|---|---|",
        f"| `so_md5` | `{fb.get('so_md5')}` | `{fa.get('so_md5')}` |",
        f"| `nproc` / `mem_gb` | {fb.get('nproc')} / {fb.get('mem_gb')} GB | {fa.get('nproc')} / {fa.get('mem_gb')} GB |",
        f"| `data_directory` | `{fb.get('data_directory')}` | `{fa.get('data_directory')}` |",
        f"| linhas em `hits` | {fb.get('hits_rows')} | {fa.get('hits_rows')} |",
        f"| `statement_timeout` | {hb.get('timeout_s')} s | {ha.get('timeout_s')} s |",
        f"| `work_mem` | {hb.get('work_mem')} | {ha.get('work_mem')} |",
        "",
        "O `so_md5` é a ÚNICA linha que muda, e é essa a variável independente.",
        "",
        "## Ganhos, separados por atribuição",
        "",
        f"- **{len(gained_routed)} atribuíveis a este milestone** — falhavam COM roteamento agregado e agora "
        f"completam: {', '.join(f'q{q:02d}' for q in gained_routed) or '(nenhuma)'}",
        f"- **{len(gained_other)} NÃO atribuíveis** — não falhavam no caminho agregado, então este fix não é a "
        f"explicação: {', '.join(f'q{q:02d}' for q in gained_other) or '(nenhuma)'}",
        "",
    ]
    if lost:
        lines += [
            "## REGRESSÕES — completavam antes e falham agora",
            "",
            "A linha mais importante do documento, e a que um resumo de 'quantas a mais passam?' esconde.",
            "",
            "| q | veredito depois | `agg_routed` | erro |",
            "|---|---|---|---|",
        ]
        by_q = {r["q"]: r for r in ra}
        lines += [f"| q{q:02d} | `{by_q[q].get('verdict')}` | {by_q[q].get('agg_routed')} | "
                  f"{(by_q[q].get('error') or '')[:56]} |" for q in lost]
        lines.append("")
    else:
        lines += ["## Regressões", "", "Nenhuma: todo `q` que completava antes continua completando.", ""]

    still = sorted(_routed_failures(ra))
    lines += [
        "## Ainda falhando NO caminho agregado",
        "",
        (", ".join(f"q{q:02d}" for q in still) if still else "(nenhuma)")
        + " — o que resta no caminho que este milestone toca.",
        "",
    ]
    return "\n".join(lines)


def _load(jsonl_path: str) -> tuple[dict, list[dict]]:
    header, records = {}, []
    with open(jsonl_path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            (header.update(obj["header"]) if "header" in obj else records.append(obj))
    return header, records


def main() -> int:
    if len(sys.argv) != 5:
        print(f"uso: {sys.argv[0]} <antes.jsonl> <antes-box.json> <depois.jsonl> <depois-box.json>",
              file=sys.stderr)
        return 2
    hb, rb = _load(sys.argv[1])
    ha, ra = _load(sys.argv[3])
    with open(sys.argv[2]) as fh:
        bb = json.load(fh)
    with open(sys.argv[4]) as fh:
        ba = json.load(fh)
    try:
        print(render((hb, rb, bb), (ha, ra, ba)))
    except IncomparableRuns as e:
        print(f"RECUSA PUBLICAR O DELTA: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
