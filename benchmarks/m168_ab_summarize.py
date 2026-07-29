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


def paired_wins(runs, q):
    """Quantos PARES o streaming venceu, sobre o total. É a estatística que o desenho produz."""
    wins = n = 0
    for r in runs:
        if q not in r["pairs"]:
            continue
        ea, st = r["pairs"][q]
        for a, b in zip(ea, st):
            n += 1
            if b < a:
                wins += 1
    return wins, n


def sign_test_p(wins, n):
    """Teste do sinal exato, bilateral. Sem dependência externa — é uma soma binomial."""
    if n == 0:
        return 1.0
    from math import comb
    k = max(wins, n - wins)
    tail = sum(comb(n, i) for i in range(k, n + 1)) / (2 ** n)
    return min(1.0, 2 * tail)


def _reject_fallback(path, problems):
    """Um braço 'streaming' que degradou para o eager em silêncio alimentaria a tabela publicada como se fosse
    streaming. Agora que o fail-open existe, o log tem de ser rejeitado se ele disparou.

    NÃO-VACUIDADE PRIMEIRO. O sinal que este guard procura (`theodb_decode_batch:`) só é emitido quando
    `THEODB_ADMIT_TRACE=1` está no ambiente do POSTMASTER — e essa variável é lida pelo backend, que a herda do
    postmaster, então ela some sem erro algum se a coleta reiniciar o servidor sem ela. Um review demonstrou por
    construção que, com o canal desligado, este arquivo imprimia "ok" e saía 0: o guard ficava cego e a tabela
    saía publicada como se ele tivesse verificado alguma coisa. Se não há NENHUM trace de streaming no log, o
    guard não pode afirmar nada — e dizer isso é a única resposta honesta."""
    if "theodb_decode_batch_stream:" not in open(path, errors="replace").read():
        problems.append(
            "nenhum `theodb_decode_batch_stream:` no log — o canal de trace estava desligado "
            "(THEODB_ADMIT_TRACE ausente do ambiente do postmaster), então o guard de degradação é CEGO: "
            "ele não conseguiria distinguir um braço streaming íntegro de um que caiu no eager")
        return
    in_stream_arm = False
    for line in open(path, errors="replace"):
        if "ARM=stream" in line:
            in_stream_arm = True
        elif "ARM=eager" in line:
            in_stream_arm = False
        # Ancorado no prefixo do servidor: um match por substring casaria com a própria prosa dos
        # gates, que menciona o nome do evento (achado de review).
        if "LOG:  theodb_topk_stream_fallback" in line or "WARNING:  theodb_topk_stream_fallback" in line:
            problems.append("log contém theodb_topk_stream_fallback: um braço degradou para o eager em silêncio")
            return
        # O marcador acima quase nunca aparece: `pgrx::log!` escreve no log do SERVIDOR, e o coletor captura só o
        # stdout do psql (achado de review — o guard era teatro). O sinal que REALMENTE aparece quando um braço
        # streaming degrada é o trace do decode eager, que só pode vir do caminho antigo.
        if in_stream_arm and "theodb_decode_batch: rows=" in line:
            problems.append("braço streaming emitiu trace de decode EAGER: ele degradou e alimentaria a tabela")
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
        wins, n = paired_wins(runs, q)
        pval = sign_test_p(wins, n)
        if n == 0:
            verdict = "SEM per-pair — não classificável"
        elif pval <= 0.05:
            verdict = f"EFEITO PAREADO (sinal {wins}/{n}, p={pval:.4f})"
        else:
            verdict = f"sem efeito pareado (sinal {wins}/{n}, p={pval:.2f})"
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
