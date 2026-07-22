"""SWEEP-ONLY: reuse the already-built sift8_idx (v8 RaBitQ) + sift5_idx (v5 f32) — skip
load AND build entirely. Measures recall@10 vs official GT + QPS (best-of-3) + buffers/query."""
import struct, os, time
import psycopg2
SIFT=os.environ.get('SIFT','/home/theo'); PORT=int(os.environ.get('PGPORT','28817'))
DB=os.environ.get('PGDB','e1'); USER=os.environ.get('PGUSER','theo'); NQ=int(os.environ.get('NQ','200'))
def read_fvecs(path, limit):
    out=[]
    with open(path,'rb') as f:
        while len(out)<limit:
            b=f.read(4)
            if not b: break
            d=struct.unpack('<i',b)[0]; out.append(struct.unpack(f'<{d}f', f.read(d*4)))
    return out
def read_ivecs(path, limit):
    out=[]
    with open(path,'rb') as f:
        while len(out)<limit:
            b=f.read(4)
            if not b: break
            d=struct.unpack('<i',b)[0]; out.append(struct.unpack(f'<{d}i', f.read(d*4)))
    return out
queries=read_fvecs(f'{SIFT}/sift_query.fvecs', NQ); gt=read_ivecs(f'{SIFT}/sift_groundtruth.ivecs', NQ)
out=open('/tmp/e1_out.txt','a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
conn=psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur=conn.cursor()
for nm in ("sift8_idx","sift5_idx"):
    cur.execute(f"SELECT pg_size_pretty(pg_relation_size('{nm}'))"); emit(f"E1_SIZE {nm}={cur.fetchone()[0]}")
def setcfg(pr, of):
    cur.execute(f"SET theodb_hnsw.over_fetch={of}"); cur.execute(f"SET theodb_ivfflat.probes={pr}"); cur.execute("SET enable_seqscan=off")
def rq(tbl, probes, of, k=10):
    setcfg(probes, of); hit=0; best=1e9
    for r in range(3):
        ts=time.time()
        for qi in range(NQ):
            lit='['+','.join(map(str,queries[qi]))+']'
            cur.execute(f"SELECT id FROM {tbl} ORDER BY e <-> %s::vector LIMIT %s",(lit,k))
            got=set(x[0] for x in cur.fetchall())
            if r==0: hit+=len(got & set(gt[qi][:k]))
        best=min(best, time.time()-ts)
    return hit/(NQ*k), NQ/best
def buffers(tbl, probes, of, k=10, nq=40):
    setcfg(probes, of); tot=0
    for qi in range(nq):
        lit='['+','.join(map(str,queries[qi]))+']'
        cur.execute(f"EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT id FROM {tbl} ORDER BY e <-> '{lit}'::vector LIMIT {k}")
        plan=cur.fetchone()[0][0]['Plan']
        def walk(p):
            b=p.get('Shared Hit Blocks',0)+p.get('Shared Read Blocks',0)
            for c in p.get('Plans',[]): b+=walk(c)
            return b
        tot+=walk(plan)
    return tot/nq
for of in [8,16,32,64]:
    for pr in [32,64,128,256]:
        r8,q8=rq("sift8",pr,of); r5,q5=rq("sift5",pr,of)
        emit(f"E1_RESULT of={of} probes={pr} v8_recall={r8:.4f} v8_qps={q8:.1f} v5_recall={r5:.4f} v5_qps={q5:.1f} qps_ratio={(q8/q5 if q5>0 else 0):.2f} recall_delta={r8-r5:.4f}")
for of,pr in [(16,64),(32,128),(64,256)]:
    b8=buffers("sift8",pr,of); b5=buffers("sift5",pr,of)
    emit(f"E1_BUFFERS of={of} probes={pr} v8_bufpq={b8:.1f} v5_bufpq={b5:.1f} ratio={(b5/b8 if b8>0 else 0):.2f}")
emit("E1_DONE")
