#!/usr/bin/env python3
"""M148 — reproduz a tabela de self-time por alavanca do veredito do flamegraph a partir dos folded
committados (benchmarks/artifacts/m148-artifacts/*-folded.txt). Torna `wiki/benchmarks/m148-flamegraph-scan.md`
reproduzível (o tests-pillar review apontou que a tabela vinha de um script inline não-versionado).

Uso: python3 benchmarks/m148_selftime.py [DIR]
  DIR default = benchmarks/artifacts/m148-artifacts
Imprime, por query (slow/scanpuro): self-time bruto por folha, o eixo I/O, e a tabela de PRODUÇÃO com os
frames exclusivos de `cassert` (randomize_mem / verify_compact_attribute / …) descontados — a mesma
metodologia do doc. Determinístico, sem rede.
"""
import collections
import re
import sys
from pathlib import Path

# Frames que só existem (ou só inflam) sob --enable-cassert — descontados para a tabela de produção.
CASSERT_ONLY = ("randomize_mem", "verify_compact_attribute", "populate_compact_attribute", "clear_page_erms")
# Sinais de I/O real de disco (se somarem > 5% do tempo, a query é I/O-bound).
IO_SIGNALS = ("pread", "__read", "preadv", "FileRead", "mdread", "io_uring")


def bucket(fn: str) -> str:
    if any(s in fn for s in ("form_row", "heap_form_tuple", "heap_fill_tuple", "fill_val", "heap_compute_data_size")):
        return "materializa-row (M151)"
    if "heap_deform_tuple" in fn:
        return "deform (M151)"
    if any(s in fn for s in ("decode_column", "columnar_codec", "ZSTD", "zstd")):
        return "decode/zstd (M149)"
    if any(s in fn for s in ("strcoll", "Sort", "tuplesort", "comparetup")):
        return "sort/collation (agg, nao-scan)"
    if any(s in fn for s in ("malloc", "cfree", "free", "memcpy", "memset", "libc.so", "AllocSet", "palloc")):
        return "alocacao por-linha"
    return "outro/executor-PG"


def analyze(path: Path) -> None:
    self_time: collections.Counter = collections.Counter()
    total = io = 0
    for line in path.read_text().splitlines():
        m = re.match(r"^(.*) (\d+)$", line)
        if not m:
            continue
        stack, n = m.group(1), int(m.group(2))
        total += n
        self_time[stack.split(";")[-1]] += n
        if any(s in stack for s in IO_SIGNALS):
            io += n
    if not total:
        print(f"  {path.name}: vazio")
        return
    cassert = sum(n for fn, n in self_time.items() if any(c in fn for c in CASSERT_ONLY))
    prod_total = total - cassert or 1
    agg: collections.Counter = collections.Counter()
    for fn, n in self_time.items():
        if any(c in fn for c in CASSERT_ONLY):
            continue
        agg[bucket(fn)] += n
    io_pct = io * 100 / total
    print(f"\n=== {path.name} — total={total}  I/O={io_pct:.2f}% ({'I/O-BOUND' if io_pct > 5 else 'CPU-BOUND'})"
          f"  cassert-descontado={cassert * 100 / total:.1f}% ===")
    for k, n in agg.most_common():
        print(f"   {n * 100 / prod_total:5.1f}%  {k}")


def main() -> int:
    d = Path(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/artifacts/m148-artifacts")
    folded = sorted(d.glob("*-folded.txt"))
    if not folded:
        print(f"nenhum *-folded.txt em {d}", file=sys.stderr)
        return 2
    for f in folded:
        analyze(f)
    return 0


if __name__ == "__main__":
    sys.exit(main())
