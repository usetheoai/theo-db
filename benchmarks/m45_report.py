"""M45 — presentation: render the rigorous Pareto report markdown from the run() report dict.

Split from run_m45_pareto.py (SRP: measurement vs rendering) so the driver stays within its
size budget. Pure string assembly — no I/O, no DB.
"""
from __future__ import annotations


def render_md(res, order) -> str:
    p = res["params"]
    v = res["verdict"]
    lines = [
        "# M45 — rigorous mean±std recall×QPS Pareto: theodb_hnsw vs pgvector hnsw on SIFT1M",
        "",
        f"**Verdict:** **{v['verdict']}** ({v.get('reason','')}).  ",
        f"**Config:** n={p['n']}, dim={p['dim']}, nq={p['nq']}, runs={p['runs']} (mean±std), "
        f"ef_grid={p['ef_grid']}, seed={p['seed']}, metric={p['metric']}, dataset={p['dataset']}.  ",
        f"**Build (matched):** {p['build_params']}. theodb build={res['build_s'].get('theodb_hnsw')}s, "
        f"pgvector build={res['build_s'].get('pgvector_hnsw')}s.  ",
        "**Type:** measurement — the rigorous mean±std the M42 signal "
        "(`docs/benchmarks/sift1m-carrier-verdict.md`) lacked. Delivers `public-copy.md` §4 half 1 "
        "(reproducible artifact); half 2 (independent third-party reproduction) remains OPEN.",
        "",
        "## Pareto frontiers (mean ± std over runs, recall@10 vs exact GT)",
        "",
    ]
    for index in order:
        lines += [f"### {index}", "", "| ef_search | recall@10 | QPS (mean ± std) | nq |", "|---|---|---|---|"]
        for pt in res["frontier"][index]:
            lines.append(f"| {pt['ef']} | {pt['recall']} | {pt['qps_mean']} ± {pt['qps_std']} | {pt['nq']} |")
        lines.append("")
    lines += ["## Matched-recall margin (Pareto interpolation, effect > variance gate)", "",
              "| shared recall | QPS theodb | QPS pgvector | margin (×) | effect>variance |",
              "|---|---|---|---|---|"]
    for m in v.get("margins", []):
        lines.append(f"| {m['recall']} | {m['qps_theodb']} | {m['qps_pgvector']} | {m['margin']} | "
                     f"{'yes' if m['effect_gt_variance'] else 'no'} |")
    lines += [
        "",
        f"**Honest verdict:** {v['verdict']}. A `SUPERIOR` verdict is licensed only when theodb QPS "
        "exceeds pgvector's at EVERY shared recall level by >5% AND the gap exceeds the combined std "
        "(PRD D3, anti-sunk-cost). `PARITY` = no defensible claim; `INFERIOR` = pgvector wins.",
        "",
        "## Honest caveats",
        "- Single machine, warm cache, local dev CPU — the matched-recall margin with variance is the "
        "deliverable; absolute QPS is machine-specific.",
        "- `public-copy.md` §4 **half 2 (independent reproduction) is OPEN** — this artifact is built for it "
        "(fixed seed, pinned images, exact command) but is not itself independent.",
        "- theodb_hnsw scan is O(ef·M) since M35+M41 (the M42 200-query cap is lifted here to "
        f"nq={p['nq']}).",
        "",
        "## Reproduce",
        "```",
        f"python3 benchmarks/run_m45_pareto.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 "
        f"--nq {p['nq']} --runs {p['runs']} --port <theo-db:m44 port> --write-doc",
        "```",
    ]
    return "\n".join(lines) + "\n"
