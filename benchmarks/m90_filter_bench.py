#!/usr/bin/env python3
# M90 gate — recall@10 sob filtro de label ~1%: INLINE (v7) vs M87 POST-filter (v5), same-data.
import os, time, random
import numpy as np, psycopg2, io
PORT=int(os.environ.get('M90_PORT','28817')); N=int(os.environ.get('M90_N','500000'))
DIM=128; LISTS=int(os.environ.get('M90_LISTS','200')); CLUST=200; NLAB=100; NQ=100; K=10; PROBES=32; CHUNK=100000
def emit(m): print(m, flush=True)
conn=psycopg2.connect(host='localhost',port=PORT,dbname='theo',user='theo'); conn.autocommit=True
cur=conn.cursor(); cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs")
emit(f"M90_START N={N} lists={LISTS} labels={NLAB} (~{100/NLAB:.1f}% selectivity)")
cur.execute("DROP TABLE IF EXISTS s90"); cur.execute("CREATE TABLE s90 (id int, lbl smallint[], e vector(%d))"%DIM)
rng=np.random.default_rng(7); centers=rng.standard_normal((CLUST,DIM)).astype(np.float32)*20  # well-separated
t0=time.time(); done=0
while done<N:
    b=min(CHUNK,N-done); lab=rng.integers(0,CLUST,size=b)
    vecs=(centers[lab]+rng.standard_normal((b,DIM)).astype(np.float32)).astype(np.float32)
    tags=rng.integers(0,NLAB,size=b)  # one label per row
    buf=[]
    for i in range(b):
        buf.append(f"{done+i}\t{{{tags[i]}}}\t[{','.join('%.4g'%x for x in vecs[i])}]\n")
    cur.copy_expert("COPY s90 FROM STDIN", io.StringIO(''.join(buf))); done+=b
emit(f"M90_LOAD done {done} in {time.time()-t0:.0f}s")
# queries: 100 random rows; target label = that row's label (matches guaranteed)
random.seed(11); qids=sorted(random.sample(range(0,N),NQ))
cur.execute("SELECT id, lbl[1], e::text FROM s90 WHERE id = ANY(%s) ORDER BY id",(qids,))
rows=cur.fetchall(); queries=[(r[1],r[2]) for r in rows]
# exact filtered top-K per query (seqscan)
cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
GT=[]
t0=time.time()
for lab,q in queries:
    cur.execute(f"SELECT id FROM s90 WHERE lbl && '{{{lab}}}'::smallint[] ORDER BY e <-> %s::vector LIMIT %s",(q,K))
    GT.append(set(x[0] for x in cur.fetchall()))
emit(f"M90_GT done in {time.time()-t0:.0f}s")
def build(name, cols, extra=""):
    t=time.time(); cur.execute(f"CREATE INDEX {name} ON s90 USING theodb_ivfflat ({cols}) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1{extra})"); return time.time()-t
def measure(name):
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=off")
    cur.execute(f"SET theodb_ivfflat.probes={PROBES}")
    # SINGLE timed pass (recall + QPS from the same loop — no throwaway pass; fixes the double-timing bug).
    hit=0; tot=0; ts=time.time()
    for i,(lab,q) in enumerate(queries):
        cur.execute(f"SELECT id FROM s90 WHERE lbl && '{{{lab}}}'::smallint[] ORDER BY e <-> %s::vector LIMIT %s",(q,K))
        got=set(x[0] for x in cur.fetchall()); hit+=len(got & GT[i]); tot+=len(GT[i])
    return hit/max(tot,1), NQ/(time.time()-ts)
# v7 INLINE (label in index)
bs=build("s90_v7","e, lbl"); r7,q7=measure("s90_v7"); emit(f"M90_V7_INLINE build_s={bs:.0f} recall={r7:.4f} qps={q7:.1f}")
cur.execute("DROP INDEX s90_v7")
# v5 POST-filter (M87: label NOT in index → post-filter + iterative)
bs=build("s90_v5","e"); r5,q5=measure("s90_v5"); emit(f"M90_V5_POST  build_s={bs:.0f} recall={r5:.4f} qps={q5:.1f}")
cur.execute("DROP INDEX s90_v5")
emit(f"M90_VERDICT inline_recall={r7:.4f} post_recall={r5:.4f} delta={r7-r5:+.4f} {'GO (inline>post)' if r7>r5 else 'HONEST-NEGATIVE (inline<=post)'}")
emit("M90_DONE")
