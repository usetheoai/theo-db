"""COLD per-query A/B: drop the OS page cache before EVERY query so each search faults its
Stage-2 pages fresh from disk. This is the out-of-RAM proxy the 15GB box otherwise hides
(a 528MB index fits OS cache, so a once-per-pass drop only chills the first queries). v5
faults the big f32 refine region; v8 faults the ¼-size RaBitQ region (or nothing — codes
already scored in Stage-1). Run AS ROOT (drops caches; connects as theo via trust auth)."""
import struct, os, time
import psycopg2
SIFT=os.environ.get('SIFT','/home/theo'); PORT=int(os.environ.get('PGPORT','28817'))
DB=os.environ.get('PGDB','e1'); USER=os.environ.get('PGUSER','theo')
NQ=int(os.environ.get('NQ','100'))
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
out=open('/tmp/e1_cold.txt','a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
def drop_caches():
    os.system("sync; echo 3 > /proc/sys/vm/drop_caches")
def cfg(cur, pr, of):
    cur.execute(f"SET theodb_hnsw.over_fetch={of}"); cur.execute(f"SET theodb_ivfflat.probes={pr}"); cur.execute("SET enable_seqscan=off")
def cold(tbl, pr, of, k=10):
    # fresh connection each index so PG's own shared_buffers start empty too
    conn=psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
    cur=conn.cursor(); cfg(cur, pr, of)
    hit=0; lat=[]
    for qi in range(NQ):
        drop_caches()
        lit='['+','.join(map(str,queries[qi]))+']'
        t=time.time()
        cur.execute(f"SELECT id FROM {tbl} ORDER BY e <-> %s::vector LIMIT %s",(lit,k))
        got=set(x[0] for x in cur.fetchall()); lat.append(time.time()-t)
        hit+=len(got & set(gt[qi][:k]))
    conn.close()
    lat.sort(); n=len(lat)
    return hit/(NQ*k), (sum(lat)/n)*1000, lat[n//2]*1000, lat[min(n-1,int(n*0.95))]*1000
for of,pr in [(16,64),(32,128),(64,256)]:
    r8,m8,p50_8,p95_8=cold("sift8",pr,of); r5,m5,p50_5,p95_5=cold("sift5",pr,of)
    sp=m5/m8 if m8>0 else 0
    emit(f"COLD_RESULT of={of} probes={pr} v8_recall={r8:.4f} v8_mean_ms={m8:.1f} v8_p50={p50_8:.1f} v8_p95={p95_8:.1f} v5_recall={r5:.4f} v5_mean_ms={m5:.1f} v5_p50={p50_5:.1f} v5_p95={p95_5:.1f} latency_speedup={sp:.2f}")
emit("COLD_DONE")
