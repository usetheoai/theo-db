#!/usr/bin/env python3
# M91 Phase-3 validation: ADAPTIVE v7 build, sweep selectivity at DEFAULT probes=64. Envelope gate:
# recall@10 must recover at ultra-selective (>=0.97 @ 0.01%) vs the 0.741 fixed baseline, and QPS ~= baseline at loose.
import struct, os, time, io
import psycopg2, numpy as np
SIFT="/home/theo/sift"; PORT=int(os.environ.get('PORT','28817')); N=1000000; NQ=100; LISTS=1000; PROBES=64; K=10
SEL=[0.0001,0.001,0.005,0.01,0.05,0.10,0.30]
def rf(p,l):
    o=[]
    with open(p,'rb') as f:
        while len(o)<l:
            b=f.read(4)
            if not b: break
            d=struct.unpack('<i',b)[0]; o.append(struct.unpack(f'<{d}f',f.read(d*4)))
    return o
def emit(s): print(s,flush=True)
base=rf(f'{SIFT}/sift_base.fvecs',N); queries=rf(f'{SIFT}/sift_query.fvecs',NQ)
rng=np.random.default_rng(7); labels=np.full(N,99,dtype=int); idx=0
for b,c in enumerate([max(1,int(s*N)) for s in SEL]): labels[idx:idx+c]=b; idx+=c
rng.shuffle(labels)
conn=psycopg2.connect(host='localhost',port=PORT,dbname='theo',user='theo'); conn.autocommit=True; cur=conn.cursor()
cur.execute("SELECT extversion FROM pg_extension WHERE extname='theodb_rs'"); emit(f"M91V_EXT {cur.fetchone()}")
# rebuild table fresh (ensure clean)
cur.execute("DROP TABLE IF EXISTS s91v"); cur.execute("CREATE TABLE s91v (id int, lbl smallint[], e vector(128))")
buf=io.StringIO()
for i,v in enumerate(base): buf.write(f"{i}\t{{{labels[i]}}}\t[{','.join(map(str,v))}]\n")
buf.seek(0); cur.copy_expert("COPY s91v FROM STDIN", buf); emit("M91V_COPIED")
QL=['['+','.join(map(str,q))+']' for q in queries]
t=time.time(); cur.execute(f"CREATE INDEX s91v_idx ON s91v USING theodb_ivfflat (e, lbl) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)"); emit(f"M91V_BUILD {time.time()-t:.0f}s")
def gt(lab):
    cur.execute("SET enable_indexscan=off; SET enable_seqscan=on"); o=[]
    for ql in QL: cur.execute(f"SELECT id FROM s91v WHERE {lab} ORDER BY e <-> %s::vector LIMIT {K}",(ql,)); o.append(set(x[0] for x in cur.fetchall()))
    return o
def meas(lab,GT):
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on"); cur.execute(f"SET theodb_ivfflat.probes={PROBES}")
    hit=0;tot=0;ts=time.time()
    for i,ql in enumerate(QL): cur.execute(f"SELECT id FROM s91v WHERE {lab} ORDER BY e <-> %s::vector LIMIT {K}",(ql,)); g=set(x[0] for x in cur.fetchall()); hit+=len(g&GT[i]); tot+=len(GT[i])
    return hit/max(tot,1), NQ/(time.time()-ts)
for b,s in enumerate(SEL):
    cur.execute(f"SELECT count(*) FROM s91v WHERE lbl && '{{{b}}}'::smallint[]"); act=cur.fetchone()[0]/N
    lab=f"lbl && '{{{b}}}'::smallint[]"; GT=gt(lab); r,q=meas(lab,GT); emit(f"M91V sel={s} sel_a={act:.4f} ADAPTIVE_v7 recall={r:.4f} qps={q:.1f} probes_default={PROBES}")
cur.execute("DROP INDEX s91v_idx"); cur.execute("DROP TABLE s91v")
emit("M91V_DONE")
