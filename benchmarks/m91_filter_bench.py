#!/usr/bin/env python3
# M91 DEFINITIVE sweep on SIFT1M (real neighbor structure → meaningful recall across the WHOLE selectivity range).
# Synthetic smallint[] labels at controlled selectivity. Compare INLINE(v7) vs POST(v5). GT = exact seqscan-filtered top-10.
import struct, os, time, io
import psycopg2, numpy as np
SIFT="/home/theo/sift"; PORT=int(os.environ.get('PORT','28817'))
N=int(os.environ.get('N','1000000')); NQ=int(os.environ.get('NQ','100')); LISTS=int(os.environ.get('LISTS','1000')); PROBES=int(os.environ.get('PROBES','64')); K=10
SEL=[0.0001,0.001,0.005,0.01,0.05,0.10,0.30]
def read_fvecs(path, limit):
    out=[]
    with open(path,'rb') as f:
        while len(out)<limit:
            b=f.read(4)
            if not b: break
            d=struct.unpack('<i',b)[0]; out.append(struct.unpack(f'<{d}f', f.read(d*4)))
    return out
def emit(s): print(s, flush=True)
base=read_fvecs(f'{SIFT}/sift_base.fvecs', N); queries=read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
emit(f"M91S_LOADED n={len(base)} nq={len(queries)} lists={LISTS} probes={PROBES}")
# labels: bucket b (0..6) -> floor(SEL[b]*N) rows; rest -> 99. shuffled.
rng=np.random.default_rng(7); labels=np.full(N,99,dtype=int); idx=0
counts=[max(1,int(s*N)) for s in SEL]
for b,c in enumerate(counts): labels[idx:idx+c]=b; idx+=c
rng.shuffle(labels)
conn=psycopg2.connect(host='localhost',port=PORT,dbname='theo',user='theo'); conn.autocommit=True; cur=conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs")
cur.execute("DROP TABLE IF EXISTS s91s"); cur.execute("CREATE TABLE s91s (id int, lbl smallint[], e vector(128))")
buf=io.StringIO()
for i,v in enumerate(base): buf.write(f"{i}\t{{{labels[i]}}}\t[{','.join(map(str,v))}]\n")
buf.seek(0); cur.copy_expert("COPY s91s FROM STDIN", buf); emit("M91S_COPIED")
QL=['['+','.join(map(str,q))+']' for q in queries]
def gt(lab):
    cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on"); out=[]
    for ql in QL:
        cur.execute(f"SELECT id FROM s91s WHERE {lab} ORDER BY e <-> %s::vector LIMIT {K}",(ql,)); out.append(set(x[0] for x in cur.fetchall()))
    return out
def meas(lab,GT):
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=off"); cur.execute(f"SET theodb_ivfflat.probes={PROBES}")
    hit=0;tot=0;ts=time.time()
    for i,ql in enumerate(QL):
        cur.execute(f"SELECT id FROM s91s WHERE {lab} ORDER BY e <-> %s::vector LIMIT {K}",(ql,)); got=set(x[0] for x in cur.fetchall()); hit+=len(got&GT[i]); tot+=len(GT[i])
    return hit/max(tot,1), NQ/(time.time()-ts)
def build(name,cols):
    t=time.time(); cur.execute(f"CREATE INDEX {name} ON s91s USING theodb_ivfflat ({cols}) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)"); emit(f"M91S_BUILD {name} {time.time()-t:.0f}s")
build("s91s_v7","e, lbl")
for b,s in enumerate(SEL):
    cur.execute(f"SELECT count(*) FROM s91s WHERE lbl && '{{{b}}}'::smallint[]"); act=cur.fetchone()[0]/N
    lab=f"lbl && '{{{b}}}'::smallint[]"; GT=gt(lab); r,q=meas(lab,GT); emit(f"M91S sel_t={s} sel_a={act:.4f} INLINE_v7 recall={r:.4f} qps={q:.1f}")
cur.execute("DROP INDEX s91s_v7")
build("s91s_v5","e")
for b,s in enumerate(SEL):
    cur.execute(f"SELECT count(*) FROM s91s WHERE lbl && '{{{b}}}'::smallint[]"); act=cur.fetchone()[0]/N
    lab=f"lbl && '{{{b}}}'::smallint[]"; GT=gt(lab); r,q=meas(lab,GT); emit(f"M91S sel_t={s} sel_a={act:.4f} POST_v5  recall={r:.4f} qps={q:.1f}")
cur.execute("DROP INDEX s91s_v5")
emit("M91S_DONE")

# --- M91 probe-recovery diagnostic (ultra-selective): appended for reproducibility ---
# Run separately: builds one v7 index, sweeps probes {64,128,256,500,1000} at sel {0.01%,0.1%,1%}.
# See m91_ultra.py logic in docs/benchmarks/m91-adaptive-filter.json probe_recovery_ultra_selective.
