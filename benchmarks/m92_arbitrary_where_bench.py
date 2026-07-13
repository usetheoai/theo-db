#!/usr/bin/env python3
# M92 DoD — arbitrary-WHERE filtered vector search, MATCHED-RECALL methodology (M45): INLINE (Custom Scan node) probe
# sweep vs POST (native M87 iterative). The fair filtered-ANN comparison is QPS at matched recall, not raw recall.
import struct, os, time, io
import psycopg2
SIFT="/home/theo/sift"; PORT=int(os.environ.get('PORT','28817')); N=int(os.environ.get('N','1000000')); NQ=100; LISTS=1000; K=10
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
emit(f"M92W_LOADED n={len(base)} nq={NQ}")
conn=psycopg2.connect(host='localhost',port=PORT,dbname='postgres',user='theo'); conn.autocommit=True; cur=conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs")
cur.execute("DROP TABLE IF EXISTS aw"); cur.execute("CREATE TABLE aw (id int, cat int, e vector(128))")
buf=io.StringIO()
for i,v in enumerate(base): buf.write(f"{i}\t{i%1000}\t[{','.join(map(str,v))}]\n")
buf.seek(0); cur.copy_expert("COPY aw FROM STDIN", buf); emit("M92W_COPIED")
t=time.time(); cur.execute(f"CREATE INDEX aw_e ON aw USING theodb_ivfflat (e) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)"); emit(f"M92W_BUILD {time.time()-t:.0f}s")
cur.execute("CREATE INDEX aw_cat ON aw (cat)"); cur.execute("ANALYZE aw")
QL=['['+','.join(map(str,q))+']' for q in queries]
def gt(where):
    cur.execute("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on"); o=[]
    for ql in QL: cur.execute(f"SELECT id FROM aw WHERE {where} ORDER BY e <-> %s::vector LIMIT {K}",(ql,)); o.append(set(x[0] for x in cur.fetchall()))
    return o
def run(where, GT, inline, probes):
    g = "on" if inline else "off"
    cur.execute(f"SET theodb.enable_vecfilter={g}; SET enable_seqscan=off; SET enable_indexscan=on; SET enable_bitmapscan=on; SET theodb_ivfflat.probes={probes}")
    hit=0;tot=0;ts=time.time()
    for i,ql in enumerate(QL):
        cur.execute(f"SELECT id FROM aw WHERE {where} ORDER BY e <-> %s::vector LIMIT {K}",(ql,)); got=set(x[0] for x in cur.fetchall())
        hit+=len(got&GT[i]); tot+=len(GT[i])
    return hit/max(tot,1), NQ/(time.time()-ts)
for label,where in [("1pct","cat<10"),("5pct","cat<50")]:
    GT=gt(where)
    rp,qp=run(where,GT,False,64)  # POST baseline (M87 iterative, probes irrelevant-ish)
    emit(f"M92W sel={label} POST recall={rp:.4f} qps={qp:.1f}")
    for pr in [64,128,256,500]:  # INLINE probe sweep → the recall-QPS frontier
        ri,qi=run(where,GT,True,pr)
        emit(f"M92W sel={label} INLINE probes={pr} recall={ri:.4f} qps={qi:.1f}")
cur.execute("DROP TABLE aw"); emit("M92W_DONE")
