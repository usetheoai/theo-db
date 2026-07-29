#!/usr/bin/env python3
"""Summarize a `m168_peak.sql` run into the eager-vs-streaming memory table.

Exists because the numbers it produces go into a published verdict, and a number transcribed by hand from a log is
a number that can disagree with its own artifact — which is exactly what a reviewer caught in the first draft
(the q23 median was read off the wrong run). This reads the committed log and prints the table; the verdict copies
what this prints.

It also ASSERTS the things the table silently assumes, so a malformed run fails loudly instead of producing a
plausible table:
  - both arms present for every query
  - the eager arm emits exactly one batch per query (it decodes the whole relation at once, by definition)
  - the streaming arm emits more than one (otherwise nothing streamed and the comparison is vacuous)
  - the streaming arm includes the probe (`probe=1`), or the maximum is a maximum over N-1 chunk-groups

Usage:  python3 benchmarks/m168_peak_summarize.py <log>        # exits non-zero if any assertion fails
"""
import re
import sys

EAGER = re.compile(r"theodb_decode_batch: rows=(\d+) bytes=(\d+)")
STREAM = re.compile(r"theodb_decode_batch_stream: rows=(\d+) bytes=(\d+)(?: probe=(\d))?")


def parse(path):
    arm, query = None, None
    # {arm: {query: [(rows, bytes, is_probe)]}}
    out = {"eager": {}, "stream": {}}
    for line in open(path, errors="replace"):
        if "ARM=eager" in line:
            arm = "eager"
        elif "ARM=stream" in line:
            arm = "stream"
        m = re.search(r"===(q\d+)===", line)
        if m:
            query = m.group(1)
            if arm:
                out[arm].setdefault(query, [])
            continue
        if arm is None or query is None:
            continue
        e = EAGER.search(line)
        if e:
            out[arm][query].append((int(e.group(1)), int(e.group(2)), False))
        s = STREAM.search(line)
        if s:
            out[arm][query].append((int(s.group(1)), int(s.group(2)), s.group(3) == "1"))
    return out


def _reject_fallback(path, problems):
    """Um braço 'streaming' que degradou para o eager em silêncio alimentaria a tabela publicada como se fosse
    streaming. Agora que o fail-open existe, o log tem de ser rejeitado se ele disparou."""
    for line in open(path, errors="replace"):
        # Ancorado no prefixo do servidor: um match por substring casaria com a própria prosa dos
        # gates, que menciona o nome do evento (achado de review).
        if "LOG:  theodb_topk_stream_fallback" in line or "WARNING:  theodb_topk_stream_fallback" in line:
            problems.append("log contém theodb_topk_stream_fallback: um braço degradou para o eager em silêncio")
            return


def main(path):
    d = parse(path)
    problems = []
    _reject_fallback(path, problems)
    queries = sorted(set(d["eager"]) | set(d["stream"]), key=lambda q: int(q[1:]))
    if not queries:
        print("FAIL: no queries found — was the postmaster started with THEODB_ADMIT_TRACE=1?")
        return 2

    print(f"{'q':<5} {'eager batches':>14} {'eager bytes':>14} {'stream batches':>15} "
          f"{'stream max':>12} {'probe?':>7} {'razão':>8}")
    for q in queries:
        ea, st = d["eager"].get(q, []), d["stream"].get(q, [])
        if not ea:
            problems.append(f"{q}: braço eager sem batch — o caminho eager não rodou")
            continue
        if not st:
            problems.append(f"{q}: braço stream sem batch — o caminho streaming não rodou")
            continue
        if len(ea) != 1:
            problems.append(f"{q}: braço eager emitiu {len(ea)} batches; por definição decodifica tudo em UM")
        if len(st) <= 1:
            problems.append(f"{q}: braço stream emitiu {len(st)} batch — nada foi transmitido, comparação vazia")
        has_probe = any(p for _, _, p in st)
        if not has_probe:
            problems.append(f"{q}: nenhum batch marcado probe=1 — o máximo é sobre N-1 chunk-groups, não N")
        e_bytes = ea[0][1]
        s_max = max(b for _, b, _ in st)
        print(f"{q:<5} {len(ea):>14} {e_bytes:>14,} {len(st):>15} {s_max:>12,} "
              f"{'sim' if has_probe else 'NAO':>7} {e_bytes / s_max:>7.1f}x")

    if problems:
        print("\nFALHAS (a tabela acima não é confiável):")
        for p in problems:
            print(f"  - {p}")
        return 1
    print("\nok: os dois braços presentes, eager em 1 batch, stream em >1 com sonda instrumentada")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
