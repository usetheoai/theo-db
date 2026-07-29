#!/usr/bin/env python3
"""Summarize `m168_stream_ab.sql` runs into the throughput table — and refuse to print one from a malformed run.

WHY THIS EXISTS. A tabela de memória do M168 sobreviveu à revisão porque um script a produzia a partir do log
commitado. A de throughput foi transcrita à mão e não sobreviveu: publicou uma execução que não estava no artefato,
citou faixas de um bloco que fora filtrado do log, e arredondou o ganho para cima e a regressão para baixo. Este
script aplica à segunda tabela a disciplina que salvou a primeira.

Ele imprime o que o verdict deve copiar — medianas, faixas por par, dispersão intra-braço — e sai não-zero se:
  - faltar alguma das 4 queries
  - algum braço tiver contagem de pares diferente da esperada (default 6)
  - o número de pares for ÍMPAR (com ordem alternada, ímpar não contrabalanceia: um braço come uma
    posição-primeira a mais, incluindo o par 1, que é o mais frio)
  - o `so_md5` divergir entre execuções (medições de binários diferentes agregadas numa tabela só)
  - faltar o bloco per-pair (é ele que sustenta qualquer alegação de faixa)

Usage:  python3 benchmarks/m168_ab_summarize.py <log> [--pairs N]
"""
import argparse
import re
import statistics
import sys

MD5 = re.compile(r"so_md5=([0-9a-f]{32})")
ROW = re.compile(r"^\s*(q\d+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*(\d+)\s*$")
PER = re.compile(r"^\s*(q\d+)\s*\|\s*([\d.\s]+?)\s*\|\s*([\d.\s]+?)\s*$")
QUERIES = ("q23", "q24", "q25", "q26")


def parse(path):
    runs, cur, md5s, seen_per_pair = [], None, [], False
    for line in open(path, errors="replace"):
        m = MD5.search(line)
        if m:
            md5s.append(m.group(1))
            cur = {"md5": m.group(1), "median": {}, "pairs": {}}
            runs.append(cur)
            continue
        if cur is None:
            continue
        if "per-pair detail" in line:
            seen_per_pair = True
            continue
        r = ROW.match(line)
        if r:
            cur["median"][r.group(1)] = (float(r.group(2)), float(r.group(3)), float(r.group(4)), int(r.group(5)))
            continue
        p = PER.match(line)
        if p and p.group(1) in QUERIES:
            ea = [float(x) for x in p.group(2).split()]
            st = [float(x) for x in p.group(3).split()]
            if ea and st:
                cur["pairs"][p.group(1)] = (ea, st)
    return runs, md5s, seen_per_pair


def _reject_fallback(path, problems):
    """Um braço 'streaming' que degradou para o eager em silêncio alimentaria a tabela publicada como se fosse
    streaming. Agora que o fail-open existe, o log tem de ser rejeitado se ele disparou."""
    for line in open(path, errors="replace"):
        if "theodb_topk_stream_fallback" in line:
            problems.append("log contém theodb_topk_stream_fallback: um braço degradou para o eager em silêncio")
            return


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("log")
    ap.add_argument("--pairs", type=int, default=6)
    a = ap.parse_args()

    runs, md5s, seen_per_pair = parse(a.log)
    problems = []
    _reject_fallback(a.log, problems)
    if not runs:
        print("FAIL: nenhuma execução encontrada (o log tem a linha so_md5=?)")
        return 2
    if len(set(md5s)) > 1:
        problems.append(f"so_md5 diverge entre execuções ({sorted(set(md5s))}) — binários diferentes numa tabela só")
    if not seen_per_pair:
        problems.append("bloco per-pair ausente do log — nenhuma alegação de faixa é verificável")
    if a.pairs % 2:
        problems.append(f"--pairs={a.pairs} é ÍMPAR: com ordem alternada isso não contrabalanceia")

    print(f"execuções: {len(runs)}  ·  so_md5: {md5s[0][:12]}…  ·  pares esperados: {a.pairs}\n")
    print(f"{'q':<5} {'razões por execução':<28} {'mediana':>8} {'faixa':>14}  leitura")
    for q in QUERIES:
        ratios, spreads, overlap_all = [], [], []
        for r in runs:
            if q not in r["median"]:
                problems.append(f"{q}: ausente de uma execução")
                continue
            ea_ms, st_ms, ratio, n = r["median"][q]
            if n != a.pairs:
                problems.append(f"{q}: {n} pares, esperado {a.pairs}")
            ratios.append(ratio)
            if q in r["pairs"]:
                ea, st = r["pairs"][q]
                spreads.append(max(ea) / min(ea))
                overlap_all.append(not (max(st) < min(ea) or max(ea) < min(st)))
        if not ratios:
            continue
        med = statistics.median(ratios)
        lo, hi = min(ratios), max(ratios)
        if overlap_all and not any(overlap_all):
            verdict = "efeito real (faixas nunca se sobrepõem)"
        elif overlap_all:
            verdict = "dentro da dispersão (faixas se sobrepõem)"
        else:
            verdict = "SEM per-pair — não classificável"
        disp = f" · dispersão eager {statistics.median(spreads):.2f}x" if spreads else ""
        print(f"{q:<5} {' '.join(f'{r:.3f}' for r in ratios):<28} {med:>8.3f} {lo:>6.3f}-{hi:<7.3f} {verdict}{disp}")

    if problems:
        print("\nFALHAS (a tabela acima não é publicável):")
        for p in dict.fromkeys(problems):
            print(f"  - {p}")
        return 1
    print("\nok: 4 queries, pares pares e completos, um binário, per-pair presente")
    return 0


if __name__ == "__main__":
    sys.exit(main())
