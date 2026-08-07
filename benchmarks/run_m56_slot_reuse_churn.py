#!/usr/bin/env python3
"""M56 fase 2 (T4.3) — slot-reuse churn benchmark: does `aminsert`'s in-place slot-reuse BOUND the index relation
size under a sustained DELETE+INSERT workload?

The M56 fase-2 DoD-1 claim: reusing tombstoned slots on insert (instead of always growing the pending region)
keeps the index relation from bloating under churn. This harness measures it directly, A/B via the
`theodb.hnsw_slot_reuse` GUC:

  build an N-row `theodb_hnsw` index → run C churn cycles, each: DELETE a fraction, VACUUM (tombstone in place),
  INSERT the same count of fresh rows → after each cycle record the index relation size (`pg_relation_size`).

  - reuse=ON  : inserts REVIVE tombstoned slots → the relation stays ~flat across cycles.
  - reuse=OFF : inserts append to the pending region → the relation grows every cycle (until a compaction fold).

The delta between the two curves is the measured value of the slot-reuse feature. NOT a competitive claim
(public-copy.md): characterization on one box, size in bytes from `pg_relation_size`, recall re-checked at the
end of each mode so a "smaller index" is not bought with broken search. Reuses the psycopg2 conventions of the
M51/M55 harnesses (Rule 9).
"""
import argparse
import json
import os

import psycopg2

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55491")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "postgres")
SEED = 42
DIM = 8


def _conn():
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres")
    c.autocommit = True
    return c


def _vec(rnd, dim):
    return "[" + ",".join(f"{rnd.gauss(0, 1):.4f}" for _ in range(dim)) + "]"


def _index_bytes(cur, idx):
    cur.execute("SELECT pg_relation_size(%s::regclass)", (idx,))
    return int(cur.fetchone()[0])


def _recall_at_10(cur, table, rnd, dim, nq=20):
    """Quick recall@10 sanity: index-scan top-10 vs exact seqscan top-10, over nq gaussian probes."""
    hit = tot = 0
    for _ in range(nq):
        q = _vec(rnd, dim)
        cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
        cur.execute(f"SELECT id FROM {table} ORDER BY e <-> '{q}'::vector LIMIT 10")
        exact = {r[0] for r in cur.fetchall()}
        cur.execute("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
        cur.execute("SET theodb_hnsw.ef_search = 200")
        cur.execute(f"SELECT id FROM {table} ORDER BY e <-> '{q}'::vector LIMIT 10")
        got = {r[0] for r in cur.fetchall()}
        hit += len(exact & got)
        tot += len(exact)
    return round(hit / max(tot, 1), 4)


def _run_mode(cur, reuse, n, dim, cycles, churn_frac, compact_pct):
    import random
    rnd = random.Random(SEED)
    table = "m56churn"
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int PRIMARY KEY, e vector({dim}))")
    for i in range(n):
        cur.execute(f"INSERT INTO {table} VALUES ({i}, '{_vec(rnd, dim)}')")
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING theodb_hnsw (e)")
    cur.execute(f"SET theodb.hnsw_slot_reuse = {'on' if reuse else 'off'}")
    # compact_pct=0 isolates slot-reuse (no fold); >0 = production lifecycle (the fold periodically REPAIRS the
    # graph + reclaims), which is what maintains recall over churn (DoD 3). Both legs tell the honest full story.
    cur.execute(f"SET theodb.hnsw_tombstone_compact_pct = {compact_pct}")

    idx = f"{table}_idx"
    sizes = [_index_bytes(cur, idx)]
    k = int(n * churn_frac)
    next_id = n
    for _ in range(cycles):
        # delete k random live ids, VACUUM (tombstone in place), then insert k fresh rows.
        cur.execute(f"SELECT id FROM {table} ORDER BY random() LIMIT {k}")
        victims = [r[0] for r in cur.fetchall()]
        cur.execute(f"DELETE FROM {table} WHERE id = ANY(%s)", (victims,))
        cur.execute(f"VACUUM {table}")
        for _ in range(k):
            cur.execute(f"INSERT INTO {table} VALUES ({next_id}, '{_vec(rnd, dim)}')")
            next_id += 1
        sizes.append(_index_bytes(cur, idx))
    recall = _recall_at_10(cur, table, rnd, dim)
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    return {"sizes_bytes": sizes, "growth_bytes": sizes[-1] - sizes[0],
            "growth_pct": round((sizes[-1] - sizes[0]) / max(sizes[0], 1) * 100, 1), "recall_at_10": recall}


def run(n, dim, cycles, churn_frac, compact_pct):
    conn = _conn()
    cur = conn.cursor()
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    on = _run_mode(cur, True, n, dim, cycles, churn_frac, compact_pct)
    off = _run_mode(cur, False, n, dim, cycles, churn_frac, compact_pct)
    conn.close()
    verdict = {
        "reuse_on_growth_pct": on["growth_pct"],
        "reuse_off_growth_pct": off["growth_pct"],
        "slot_reuse_bounds_growth": on["growth_pct"] < off["growth_pct"],
        "size_reduction_ratio": round(off["sizes_bytes"][-1] / max(on["sizes_bytes"][-1], 1), 2),
        "recall_preserved_on": on["recall_at_10"] >= 0.9,
    }
    return {"milestone": "M56-fase2", "n": n, "dim": dim, "cycles": cycles, "churn_frac": churn_frac,
            "reuse_on": on, "reuse_off": off, "verdict": verdict,
            "caveats": [
                "compaction disabled (hnsw_tombstone_compact_pct=0) so the A/B isolates slot-reuse (else the fold "
                "would reclaim for both modes); in production the fold ALSO bounds the OFF case, just less promptly.",
                "size via pg_relation_size (bytes); recall@10 re-checked per mode so a smaller index is not bought "
                "with broken search. Characterization on one box, not a competitive claim.",
            ]}


def main():
    ap = argparse.ArgumentParser(description="M56 fase2 slot-reuse churn benchmark (index size under DELETE+INSERT).")
    ap.add_argument("--n", type=int, default=5000)
    ap.add_argument("--dim", type=int, default=DIM)
    ap.add_argument("--cycles", type=int, default=10)
    ap.add_argument("--churn-frac", type=float, default=0.2)
    ap.add_argument("--compact-pct", type=int, default=0)
    ap.add_argument("--out-json", default="benchmarks/artifacts/m56-slot-reuse-churn.json")
    ap.add_argument("--out-md", default="wiki/benchmarks/m56-slot-reuse-churn.md")
    args = ap.parse_args()
    data = run(args.n, args.dim, args.cycles, args.churn_frac, args.compact_pct)
    os.makedirs(os.path.dirname(args.out_json), exist_ok=True)
    with open(args.out_json, "w") as f:
        json.dump(data, f, indent=2)
    v = data["verdict"]
    lines = [
        "# M56 fase 2 — slot-reuse churn benchmark (índice não incha sob DELETE+INSERT)", "",
        f"Caracterização (NÃO comparação competitiva). N={data['n']}, dim={data['dim']}, {data['cycles']} ciclos de "
        f"churn ({int(data['churn_frac']*100)}% por ciclo), compaction desligada para isolar o slot-reuse.", "",
        "| Modo | Crescimento do índice | Tamanho final | recall@10 |",
        "|---|---|---|---|",
        f"| **reuse ON** | {data['reuse_on']['growth_pct']}% | {data['reuse_on']['sizes_bytes'][-1]} B | {data['reuse_on']['recall_at_10']} |",
        f"| reuse OFF | {data['reuse_off']['growth_pct']}% | {data['reuse_off']['sizes_bytes'][-1]} B | {data['reuse_off']['recall_at_10']} |",
        "",
        f"**Veredito:** slot-reuse limita o crescimento? **{v['slot_reuse_bounds_growth']}** "
        f"(ON {v['reuse_on_growth_pct']}% vs OFF {v['reuse_off_growth_pct']}%); índice OFF/ON = "
        f"**{v['size_reduction_ratio']}×**; recall preservado (ON): **{v['recall_preserved_on']}**.", "",
        "## Caveats honestos", "",
    ] + [f"- {c}" for c in data["caveats"]] + [""]
    with open(args.out_md, "w") as f:
        f.write("\n".join(lines))
    print(json.dumps({"verdict": v, "out_json": args.out_json, "out_md": args.out_md}, indent=2))


if __name__ == "__main__":
    main()
