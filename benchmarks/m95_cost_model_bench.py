#!/usr/bin/env python3
# M95 — honest cost model gate. Selectivity sweep on SIFT1M: at each selectivity, does the planner (honest cost,
# vecfilter_force=off) pick the node? And measure the FORCED node (INLINE) vs the native POST-filter — the recall
# the planner cannot see. Proves: (1) no over-selection at loose (node NOT chosen); (2) where the node IS
# chosen/forced, its recall+QPS relative to POST. Reuses the M92 fixture loader (Rule 9).
import struct, os, time, io
import psycopg2
SIFT = "/home/theo/sift"; PORT = int(os.environ.get('PORT', '28817')); N = int(os.environ.get('N', '1000000'))
NQ = 100; LISTS = 1000; K = 10

def rf(p, l):
    with open(p, 'rb') as f:
        d = f.read()
    o = []; i = 0
    while i < len(d) and len(o) < l:
        dim = struct.unpack('<i', d[i:i+4])[0]; i += 4
        o.append(struct.unpack(f'<{dim}f', d[i:i+dim*4])); i += dim*4
    return o

def emit(s): print(s, flush=True)

base = rf(f"{SIFT}/sift_base.fvecs", N); queries = rf(f"{SIFT}/sift_query.fvecs", NQ)
emit(f"M95_LOADED n={len(base)} nq={NQ}")
conn = psycopg2.connect(host='localhost', port=PORT, dbname='postgres', user='theo'); conn.autocommit = True; cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS aw"); cur.execute("CREATE TABLE aw (id int, cat int, e vector(128))")
buf = io.StringIO()
for i, v in enumerate(base): buf.write(f"{i}\t{i%1000}\t[{','.join(map(str,v))}]\n")
buf.seek(0); cur.copy_expert("COPY aw FROM STDIN", buf); emit("M95_COPIED")
t = time.time(); cur.execute(f"CREATE INDEX aw_e ON aw USING theodb_ivfflat (e) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)"); emit(f"M95_BUILD {time.time()-t:.0f}s")
cur.execute("CREATE INDEX aw_cat ON aw (cat)"); cur.execute("ANALYZE aw")
QL = ['['+','.join(map(str, q))+']' for q in queries]

def gt(where):
    cur.execute("SET theodb.enable_vecfilter=off; SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on"); o = []
    for ql in QL:
        cur.execute(f"SELECT id FROM aw WHERE {where} ORDER BY e <-> %s::vector LIMIT {K}", (ql,)); o.append(set(x[0] for x in cur.fetchall()))
    return o

def chosen(where):
    # Honest cost, no force: does the planner pick the node?
    cur.execute("SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=off; SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=on; SET theodb_ivfflat.probes=64")
    cur.execute(f"EXPLAIN (COSTS OFF) SELECT id FROM aw WHERE {where} ORDER BY e <-> %s::vector LIMIT {K}", (QL[0],))
    return any('theodb_vecfilter' in r[0] for r in cur.fetchall())

def run(where, GT, mode, probes):
    # mode: 'post' (native), 'inline' (forced node)
    if mode == 'post':
        cur.execute(f"SET theodb.enable_vecfilter=off; SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=on; SET theodb_ivfflat.probes={probes}")
    else:
        cur.execute(f"SET theodb.enable_vecfilter=on; SET theodb.vecfilter_force=on; SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=on; SET theodb_ivfflat.probes={probes}")
    hit = 0; tot = 0; ts = time.time()
    for i, ql in enumerate(QL):
        cur.execute(f"SELECT id FROM aw WHERE {where} ORDER BY e <-> %s::vector LIMIT {K}", (ql,)); got = set(x[0] for x in cur.fetchall())
        hit += len(got & GT[i]); tot += len(GT[i])
    return hit / max(tot, 1), NQ / (time.time() - ts)

# selectivity = cat<M / 1000 (cat = id%1000). Dense around the M92-bracketed crossover [5%, 25%].
SWEEP = [("0.1pct", "cat<1"), ("0.5pct", "cat<5"), ("1pct", "cat<10"), ("2pct", "cat<20"),
         ("5pct", "cat<50"), ("8pct", "cat<80"), ("12pct", "cat<120"), ("15pct", "cat<150"),
         ("25pct", "cat<250"), ("50pct", "cat<500")]
for label, where in SWEEP:
    GT = gt(where)
    ch = chosen(where)
    rp, qp = run(where, GT, 'post', 64)
    ri, qi = run(where, GT, 'inline', 64)
    emit(f"M95 sel={label} chosen_node={ch} POST recall={rp:.4f} qps={qp:.1f} | INLINE(forced) recall={ri:.4f} qps={qi:.1f}")
cur.execute("DROP TABLE aw"); emit("M95_DONE")
