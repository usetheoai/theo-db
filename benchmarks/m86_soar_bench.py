import struct, time, os, io
import psycopg2
SIFT="/home/theo/sift"; PORT=28817
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
N=int(os.environ.get('M86_N','1000000')); NQ=int(os.environ.get('M86_NQ','200')); LISTS=int(os.environ.get('M86_LISTS','500'))
out=open('/home/theo/m86_out.txt','a')
def emit(s): out.write(s+"\n"); out.flush(); print(s)
base=read_fvecs(f'{SIFT}/sift_base.fvecs', N)
queries=read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
gt=read_ivecs(f'{SIFT}/sift_groundtruth.ivecs', NQ)
conn=psycopg2.connect(host='localhost', port=PORT, dbname='m86', user='theo'); conn.autocommit=True
cur=conn.cursor()
def load(tbl):
    cur.execute(f"DROP TABLE IF EXISTS {tbl}"); cur.execute(f"CREATE TABLE {tbl} (id int, e vector(128))")
    buf=io.StringIO()
    for i,v in enumerate(base): buf.write(f"{i}\t[{','.join(map(str,v))}]\n")
    buf.seek(0); cur.copy_expert(f"COPY {tbl} FROM STDIN", buf)
load("siftsoar"); load("siftbase")
emit(f"M86_LOADED n={len(base)} nq={NQ} lists={LISTS}")
def build(tbl,name,wc):
    cur.execute(f"DROP INDEX IF EXISTS {name}")
    t=time.time(); cur.execute(f"CREATE INDEX {name} ON {tbl} USING theodb_ivfflat (e){wc}"); return time.time()-t
emit(f"M86_BUILD soar_s={build('siftsoar','siftsoar_idx',f' WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1, soar_lambda=1000)'):.1f}")
emit(f"M86_BUILD base_s={build('siftbase','siftbase_idx',f' WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)'):.1f}")
for nm in ("siftsoar_idx","siftbase_idx"):
    cur.execute(f"SELECT pg_size_pretty(pg_relation_size('{nm}'))"); emit(f"M86_SIZE {nm}={cur.fetchone()[0]}")
def rq(tbl, probes, of, k=10):
    cur.execute(f"SET theodb_hnsw.over_fetch={of}"); cur.execute(f"SET theodb_ivfflat.probes={probes}"); cur.execute("SET enable_seqscan=off")
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
# GATE: recall-at-FIXED-probes (does SOAR reach higher recall at the same probes?) + QPS at matched recall
for of in [8,16]:
    for pr in [4,8,16,32,64]:
        rS,qS=rq("siftsoar",pr,of); rB,qB=rq("siftbase",pr,of)
        emit(f"M86_RESULT of={of} probes={pr} soar_recall={rS:.4f} soar_qps={qS:.1f} base_recall={rB:.4f} base_qps={qB:.1f} recall_gain={rS-rB:+.4f} qps_ratio={qS/qB if qB>0 else 0:.2f}")
emit("M86_DONE")
