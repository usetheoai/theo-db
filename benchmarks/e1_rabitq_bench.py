import struct, time, os, io, json
import psycopg2
SIFT=os.environ.get('SIFT','/root'); PORT=int(os.environ.get('PGPORT','28817'))
DB=os.environ.get('PGDB','e1'); USER=os.environ.get('PGUSER','root')
def read_fvecs(path, limit):
    out=[]
    with open(path,'rb') as f:
        while len(out)<limit:
            b=f.read(4)
            if not b: break
            d=struct.unpack('<i',b)[0]
            out.append(struct.unpack(f'<{d}f', f.read(d*4)))
    return out
def read_ivecs(path, limit):
    out=[]
    with open(path,'rb') as f:
        while len(out)<limit:
            b=f.read(4)
            if not b: break
            d=struct.unpack('<i',b)[0]
            out.append(struct.unpack(f'<{d}i', f.read(d*4)))
    return out
N=int(os.environ.get('N','1000000')); NQ=int(os.environ.get('NQ','200')); LISTS=int(os.environ.get('LISTS','1000'))
BITS=int(os.environ.get('RABITQ_BITS','7'))
out=open('/tmp/e1_out.txt','a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
base=read_fvecs(f'{SIFT}/sift_base.fvecs', N)
queries=read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
gt=read_ivecs(f'{SIFT}/sift_groundtruth.ivecs', NQ)
conn=psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur=conn.cursor()
def load(tbl):
    cur.execute(f"DROP TABLE IF EXISTS {tbl}"); cur.execute(f"CREATE TABLE {tbl} (id int, e vector(128))")
    buf=io.StringIO()
    for i,v in enumerate(base): buf.write(f"{i}\t[{','.join(map(str,v))}]\n")
    buf.seek(0); cur.copy_expert(f"COPY {tbl} FROM STDIN", buf)
load("sift8"); load("sift5")
emit(f"E1_LOADED n={len(base)} nq={NQ} lists={LISTS} bits={BITS}")
def build(tbl,name,wc):
    cur.execute(f"DROP INDEX IF EXISTS {name}")
    t=time.time(); cur.execute(f"CREATE INDEX {name} ON {tbl} USING theodb_ivfflat (e){wc}"); return time.time()-t
emit(f"E1_BUILD v8rabitq_s={build('sift8','sift8_idx',f' WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1, refine=2, rabitq_bits={BITS})'):.1f}")
emit(f"E1_BUILD v5f32_s={build('sift5','sift5_idx',f' WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)'):.1f}")
for nm in ("sift8_idx","sift5_idx"):
    cur.execute(f"SELECT pg_size_pretty(pg_relation_size('{nm}'))"); emit(f"E1_SIZE {nm}={cur.fetchone()[0]}")
def setcfg(probes, of):
    cur.execute(f"SET theodb_hnsw.over_fetch={of}"); cur.execute(f"SET theodb_ivfflat.probes={probes}"); cur.execute("SET enable_seqscan=off")
def rq(tbl, probes, of, k=10):
    setcfg(probes, of)
    hit=0; best=1e9
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
    setcfg(probes, of)
    tot=0
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
        sp=q8/q5 if q5>0 else 0
        emit(f"E1_RESULT of={of} probes={pr} v8_recall={r8:.4f} v8_qps={q8:.1f} v5_recall={r5:.4f} v5_qps={q5:.1f} qps_ratio={sp:.2f} recall_delta={r8-r5:.4f}")
# buffers evidence at a mid config
for of,pr in [(16,64),(32,128),(64,256)]:
    b8=buffers("sift8",pr,of); b5=buffers("sift5",pr,of)
    emit(f"E1_BUFFERS of={of} probes={pr} v8_bufpq={b8:.1f} v5_bufpq={b5:.1f} ratio={(b5/b8 if b8>0 else 0):.2f}")
emit("E1_DONE")
