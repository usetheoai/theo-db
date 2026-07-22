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
N=int(os.environ.get('M87_N','1000000')); NQ=int(os.environ.get('M87_NQ','100')); LISTS=int(os.environ.get('M87_LISTS','500'))
out=open('/home/theo/m87_out.txt','a')
def emit(s): out.write(s+"\n"); out.flush(); print(s)
base=read_fvecs(f'{SIFT}/sift_base.fvecs', N)
queries=read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
conn=psycopg2.connect(host='localhost', port=PORT, dbname='m87', user='theo'); conn.autocommit=True
cur=conn.cursor()
cur.execute("DROP TABLE IF EXISTS sift"); cur.execute("CREATE TABLE sift (id int, label int, e vector(128))")
buf=io.StringIO()
for i,v in enumerate(base): buf.write(f"{i}\t{i%10}\t[{','.join(map(str,v))}]\n")  # label = id%10 (10% per label)
buf.seek(0); cur.copy_expert("COPY sift FROM STDIN", buf)
emit(f"M87_LOADED n={len(base)} nq={NQ} lists={LISTS}")
t=time.time(); cur.execute(f"CREATE INDEX sift_idx ON sift USING theodb_ivfflat (e) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)")
emit(f"M87_BUILD v5_s={time.time()-t:.1f}")
# EXPLAIN: does the planner pick the index scan under a WHERE filter?
cur.execute("SET enable_seqscan=off; SET theodb_ivfflat.probes=32; SET theodb_hnsw.over_fetch=8")
q0='['+','.join(map(str,queries[0]))+']'
cur.execute(f"EXPLAIN SELECT id FROM sift WHERE label IN (1,2,3) ORDER BY e <-> '{q0}'::vector LIMIT 10")
plan='|'.join(r[0].strip() for r in cur.fetchall()[:3])
emit(f"M87_EXPLAIN {plan}")

def filt_recall_qps(sel_labels, probes, of, k=10):
    where=f"label IN ({','.join(map(str,sel_labels))})"
    cur.execute(f"SET theodb_hnsw.over_fetch={of}"); cur.execute(f"SET theodb_ivfflat.probes={probes}")
    hit=0; best=1e9
    for r in range(2):
        ts=time.time()
        for qi in range(NQ):
            lit='['+','.join(map(str,queries[qi]))+']'
            cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
            cur.execute(f"SELECT id FROM sift WHERE {where} ORDER BY e <-> %s::vector LIMIT {k}",(lit,))
            got=[x[0] for x in cur.fetchall()]
            if r==0:
                # exact filtered GT via seqscan
                cur.execute("SET enable_indexscan=off; SET enable_seqscan=on")
                cur.execute(f"SELECT id FROM sift WHERE {where} ORDER BY e <-> %s::vector LIMIT {k}",(lit,))
                gt=set(x[0] for x in cur.fetchall())
                hit += len(set(got) & gt)
        best=min(best, time.time()-ts)
    return hit/(NQ*k), NQ/best

for sel,labs in [("10pct",[3]),("30pct",[1,2,3])]:
    for pr in [32,64]:
        r,qps=filt_recall_qps(labs,pr,8)
        emit(f"M87_RESULT sel={sel} probes={pr} filtered_recall={r:.4f} qps={qps:.1f}")
emit("M87_DONE")
