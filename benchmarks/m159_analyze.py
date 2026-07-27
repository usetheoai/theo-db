#!/usr/bin/env python3
"""M159 — merge TheoDB + ClickHouse same-box ClickBench results into a per-query gap table + honest verdict.
Ratio = TheoDB_hot / ClickHouse_hot (owner target: TheoDB 2-3x SLOWER than ClickHouse => ratio in [2,3] is on-target).
Usage: m159_analyze.py theodb_cb.json ch_cb.jsonl > table.md
The pure computation (geomean/build_rows/classify) is factored out for unit tests (benchmarks/tests/test_m159_analyze.py).
"""
import json
import math
import sys


def geomean(xs):
    """Geometric mean of positive numbers; NaN on empty."""
    xs = [x for x in xs if x and x > 0]
    if not xs:
        return float("nan")
    return math.exp(sum(math.log(x) for x in xs) / len(xs))


def build_rows(theodb, ch):
    """Merge TheoDB harness JSON + ClickHouse {q: ch_hot_s} dict → per-query rows with ratio (or a note when
    incomparable). Ratio is None (never fabricated) whenever either side is missing / errored / below timer floor."""
    rows = []
    for e in theodb.get("queries", []):
        q = e["q"]
        td = e.get("hot")
        err = e.get("error")
        chv = ch.get(q)
        ratio, note = None, ""
        if err:
            note = f"TheoDB ERROR/timeout: {err[:40]}"
        elif td is None:
            note = "TheoDB no hot time"
        elif chv is None or chv < 0:
            note = "CH no time"
        elif chv == 0:
            note = "CH ~0s (below timer resolution)"
        else:
            ratio = td / chv
        rows.append(dict(q=q, td=td, ch=chv, ratio=ratio, pushdown=e.get("columnar_customscan"),
                         ab=e.get("result_ab_identical"), note=note, sql=e.get("sql", "")))
    return rows


def classify(rows):
    """Summary stats over comparable ratios: overall/pushdown/non-pushdown geomean + on-target/gap/structural counts."""
    comp = [r["ratio"] for r in rows if r["ratio"] and r["ratio"] > 0]
    pd = [r["ratio"] for r in rows if r["ratio"] and r["pushdown"]]
    npd = [r["ratio"] for r in rows if r["ratio"] and r["pushdown"] is False]
    return dict(
        n_comparable=len(comp), n_total=len(rows),
        geomean_all=geomean(comp), geomean_pushdown=geomean(pd), geomean_nonpushdown=geomean(npd),
        on_target=len([r for r in comp if r <= 3]), faster=len([r for r in comp if r < 1]),
        gap=len([r for r in comp if 3 < r <= 10]), structural=len([r for r in comp if r > 10]),
        n_pushdown=len(pd), n_nonpushdown=len(npd))


def _load_ch(path):
    ch = {}
    for line in open(path):
        line = line.strip()
        if line:
            o = json.loads(line)
            ch[o["q"]] = o["ch_hot_s"]
    return ch


def main(argv):
    theodb = json.load(open(argv[1]))
    ch = _load_ch(argv[2])
    rows = build_rows(theodb, ch)
    print("| q | TheoDB hot (s) | ClickHouse hot (s) | ratio (TD/CH) | pushdown | A/B | note |")
    print("|---|---|---|---|---|---|---|")
    for r in rows:
        ratio = f"{r['ratio']:.2f}x" if r["ratio"] else "—"
        td = f"{r['td']:.4f}" if isinstance(r["td"], (int, float)) else "—"
        chv = f"{r['ch']:.4f}" if isinstance(r["ch"], (int, float)) and r["ch"] >= 0 else "—"
        pd = "yes" if r["pushdown"] else ("no" if r["pushdown"] is False else "?")
        ab = "✓" if r["ab"] else ("✗" if r["ab"] is False else "?")
        print(f"| q{r['q']} | {td} | {chv} | {ratio} | {pd} | {ab} | {r['note']} |")
    s = classify(rows)
    print()
    print(f"**Comparable queries:** {s['n_comparable']}/{s['n_total']}")
    print(f"**Geomean ratio (TheoDB/ClickHouse):** {s['geomean_all']:.2f}x")
    print(f"**On-target (ratio ≤ 3×, incl. faster):** {s['on_target']}/{s['n_comparable']}  "
          f"(of which TheoDB FASTER, <1×: {s['faster']})")
    print(f"**Gap (3×–10×):** {s['gap']}/{s['n_comparable']}")
    print(f"**Structural gap (>10×):** {s['structural']}/{s['n_comparable']}")
    print(f"**Geomean ratio — pushdown queries ({s['n_pushdown']}):** {s['geomean_pushdown']:.2f}x")
    print(f"**Geomean ratio — non-pushdown queries ({s['n_nonpushdown']}):** {s['geomean_nonpushdown']:.2f}x")


if __name__ == "__main__":
    main(sys.argv)
