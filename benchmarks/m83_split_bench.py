import struct, time, os, io, re
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

N=int(os.environ.get('M83_N','1000000')); NQ=int(os.environ.get('M83_NQ','200'))
out=open('/home/theo/m83_out.txt','a')
def emit(s): out.write(s+"\n"); out.flush(); print(s)

base=read_fvecs(f'{SIFT}/sift_base.fvecs', N)
queries=read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
gt=read_ivecs(f'{SIFT}/sift_groundtruth.ivecs', NQ)
conn=psycopg2.connect(host='localhost', port=PORT, dbname='m83', user='theo'); conn.autocommit=True
cur=conn.cursor()

def load(tbl):
    cur.execute(f"DROP TABLE IF EXISTS {tbl}")
    cur.execute(f"CREATE TABLE {tbl} (id int, e vector(128))")
    buf=io.StringIO()
    for i,v in enumerate(base): buf.write(f"{i}\t[{','.join(map(str,v))}]\n")
    buf.seek(0); cur.copy_expert(f"COPY {tbl} FROM STDIN", buf)
# Two tables, IDENTICAL data (same-data A/B, M46 rigor): v5 storage-separated vs v4 interleaved.
load("sift5"); load("sift4")
emit(f"M83_LOADED n={len(base)} nq={NQ} (sift5, sift4 same data)")

def build(tbl, name, wc):
    cur.execute(f"DROP INDEX IF EXISTS {name}")
    t=time.time(); cur.execute(f"CREATE INDEX {name} ON {tbl} USING theodb_ivfflat (e){wc}"); return time.time()-t
tv5=build("sift5","sift5_idx"," WITH (lists=1000, pq_subspaces=32, aq_threshold=2000, separate_storage=1)")
emit(f"M83_BUILD v5_s={tv5:.1f}")
tv4=build("sift4","sift4_idx"," WITH (lists=1000, pq_subspaces=32, aq_threshold=2000)")
emit(f"M83_BUILD v4_s={tv4:.1f}")
for nm in ("sift5_idx","sift4_idx"):
    cur.execute(f"SELECT pg_size_pretty(pg_relation_size('{nm}'))")
    emit(f"M83_SIZE {nm}={cur.fetchone()[0]}")

BUF_RE=re.compile(r'Buffers: shared hit=(\d+)(?: read=(\d+))?')
def buffers_hit(tbl, probes, k=10, sample=30):
    cur.execute(f"SET theodb_ivfflat.probes={probes}"); cur.execute("SET enable_seqscan=off")
    tot=0; cnt=0
    for qi in range(min(sample, NQ)):
        lit='['+','.join(map(str,queries[qi]))+']'
        cur.execute(f"EXPLAIN (ANALYZE, BUFFERS) SELECT id FROM {tbl} ORDER BY e <-> '{lit}'::vector LIMIT {k}")
        for row in cur.fetchall():
            m=BUF_RE.search(row[0])
            if m: tot += int(m.group(1)) + (int(m.group(2)) if m.group(2) else 0)
        cnt+=1
    return tot/max(cnt,1)

def rq(tbl, probes, k=10):
    cur.execute(f"SET theodb_ivfflat.probes={probes}"); cur.execute("SET enable_seqscan=off")
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

for eng,tbl in (("v5split","sift5"),("v4interleaved","sift4")):
    for pr in [8,16,32,64,128]:
        r,q=rq(tbl,pr); bh=buffers_hit(tbl,pr)
        emit(f"M83_RESULT engine={eng} probes={pr} recall={r:.4f} qps={q:.1f} buffers_per_query={bh:.0f}")
emit("M83_DONE")
