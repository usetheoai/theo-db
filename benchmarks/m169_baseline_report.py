"""M169 T1.2 — turn the baseline run into the artifact the acceptance criterion demands.

The facts already exist: the two box attestations carry `so_md5`/`nproc`/`free`/`loadavg`, and the JSONL carries
one verdict per query. What was missing is the document — and a criterion that requires an artifact nobody
generates gets satisfied by hand-assembly, which is exactly how provenance goes missing.

The load-bearing part of this file is what it REFUSES to emit. A report with `so_md5: unknown` names no binary;
a report over a truncated run publishes a number about the harness as if it were about the product; a report
whose `.so` changed mid-run averages two binaries. All three would look complete, and an artifact is read long
after anyone remembers what was in it.
"""
from __future__ import annotations

import json
import sys

TOTAL_QUERIES = 43


class IncompleteProvenance(Exception):
    """Raised instead of emitting a document that cannot say what produced it."""


def _facts(box: dict) -> dict:
    return box.get("facts", box)


def render(header: dict, records: list[dict], box_before: dict, box_after: dict) -> str:
    b, a = _facts(box_before), _facts(box_after)
    expected = header.get("n_queries", TOTAL_QUERIES)

    if b.get("so_md5", "unknown") == "unknown":
        raise IncompleteProvenance(
            "so_md5=unknown — o artefato não conseguiria dizer QUAL binário produziu estes números. Este projeto "
            "já pagou por isso: um oráculo passou contra o `.so` ANTIGO porque o postmaster não fora reiniciado.")
    if a.get("so_md5") and a["so_md5"] != b["so_md5"]:
        raise IncompleteProvenance(
            f"so_md5 mudou durante a corrida ({b['so_md5'][:12]} -> {a['so_md5'][:12]}) — a medição mistura dois "
            "binários e não é atribuível a nenhum.")
    if len(records) != expected:
        raise IncompleteProvenance(
            f"run incompleto: {len(records)}/{expected} consultas com registro. Publicar 'N/{expected} completam' "
            "a partir de uma corrida truncada é uma afirmação sobre o produto feita a partir de um fato sobre o "
            "harness.")

    by: dict[str, int] = {}
    for rec in records:
        by[rec.get("verdict") or "sem_veredito"] = by.get(rec.get("verdict") or "sem_veredito", 0) + 1
    completed = by.get("ok", 0)

    compared = [r.get("ab_identical") for r in records if r.get("ab_identical") is not None]
    ab = ("**n/a — nenhuma comparação columnar-vs-heap foi executada** (o gêmeo `hits_heap` estava ausente). "
          "Correção NÃO foi verificada nesta corrida." if not compared else
          (f"DIVERGIU em {sum(1 for c in compared if c is False)} de {len(compared)}"
           if any(c is False for c in compared) else
           f"byte-identical em {len(compared)} comparações"))

    placeholders = {k: v for k, v in (header.get("gucs_effective") or {}).items()
                    if str(v).startswith("PLACEHOLDER")}

    lines = [
        f"# M169 — baseline ClickBench a 100M ({header.get('label')})",
        "",
        f"**{completed}/{expected} consultas completam.** Este é o número que o milestone existe para mover; ele é",
        "uma medição de CONCLUSÃO, não de velocidade — o critério é *a consulta termina*.",
        "",
        "## Proveniência",
        "",
        "| | |",
        "|---|---|",
        f"| `so_md5` | `{b['so_md5']}` |",
        f"| `nproc` | {b.get('nproc')} |",
        f"| `free -g` (total) | {b.get('mem_gb')} GB |",
        f"| `loadavg1` antes / depois | {b.get('loadavg1')} / {a.get('loadavg1')} |",
        f"| `data_directory` | `{b.get('data_directory')}` |",
        f"| `hits` (linhas, da tabela) | {b.get('hits_rows')} |",
        f"| `hits_heap` | {'ausente' if b.get('hits_heap_rows', 0) < 0 else b.get('hits_heap_rows')} |",
        f"| `statement_timeout` | {header.get('timeout_s')} s |",
        f"| `work_mem` | {header.get('work_mem')} |",
        "",
        f"O teto de {header.get('timeout_s')} s é o do M162 — o `19/43` contra o qual este número se compara só é",
        "comparável sob o MESMO teto.",
        "",
        "## Vereditos",
        "",
        "| veredito | n |",
        "|---|---|",
    ]
    lines += [f"| `{k}` | {v} |" for k, v in sorted(by.items())]
    lines += [
        "",
        f"**A/B columnar vs heap:** {ab}",
        "",
        "## GUCs efetivas",
        "",
        "Lidas de volta de `pg_settings` após o `SET` — um parâmetro desconhecido no prefixo de uma extensão é",
        "aceito como *placeholder* silencioso, então declarar a GUC pedida não prova que ela existe.",
        "",
    ]
    lines += [f"- `{k}` = `{v}`" for k, v in (header.get("gucs_effective") or {}).items()]
    if placeholders:
        lines += ["", f"> **ATENÇÃO:** {list(placeholders)} não existem no servidor — o `SET` sucedeu sem efeito.",
                  "> A corrida mediu uma configuração que não está ligada."]
    lines += [
        "",
        "## Reprodução",
        "",
        "```bash",
        "ALLOW_MISSING_HEAP=1 bash benchmarks/m169_baseline_100m.sh",
        "python3 benchmarks/m169_baseline_summarize.py docs/benchmarks/m169-artifacts/baseline-100m.jsonl",
        "```",
        "",
    ]
    return "\n".join(lines)


def build(jsonl_path: str, before_path: str, after_path: str) -> str:
    header, records = {}, []
    with open(jsonl_path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            (header.update(obj["header"]) if "header" in obj else records.append(obj))
    with open(before_path) as fh:
        before = json.load(fh)
    with open(after_path) as fh:
        after = json.load(fh)
    return render(header, records, before, after)


def main() -> int:
    if len(sys.argv) != 4:
        print(f"uso: {sys.argv[0]} <baseline.jsonl> <box-before.json> <box-after.json>", file=sys.stderr)
        return 2
    try:
        print(build(sys.argv[1], sys.argv[2], sys.argv[3]))
    except IncompleteProvenance as e:
        print(f"RECUSA EMITIR O ARTEFATO: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
