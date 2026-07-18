"""E2 FastScan ablation — build theodb_symqg ONCE, sweep recall@10+QPS with the FastScan kernel ON then OFF
(SET theodb.symqg_fastscan) on the SAME index + SAME box. Isolates the kernel's measured effect (the v2-vs-v3
cross-box comparison is confounded by the box change; this is the honest same-index A/B)."""
import struct, os, time, io
import psycopg2
SIFT = os.environ.get('SIFT', '/home/theo'); PORT = int(os.environ.get('PGPORT', '28817'))
DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000')); NQ = int(os.environ.get('NQ', '200')); DEGREE = int(os.environ.get('DEGREE', '32'))


def read_fvecs(path, limit):
    out = []
    with open(path, 'rb') as f:
        while len(out) < limit:
            b = f.read(4)
            if not b:
                break
            d = struct.unpack('<i', b)[0]
            out.append(struct.unpack(f'<{d}f', f.read(d * 4)))
    return out


def read_ivecs(path, limit):
    out = []
    with open(path, 'rb') as f:
        while len(out) < limit:
            b = f.read(4)
            if not b:
                break
            d = struct.unpack('<i', b)[0]
            out.append(struct.unpack(f'<{d}i', f.read(d * 4)))
    return out


out = open('/tmp/fsab_out.txt', 'a')


def emit(s):
    out.write(s + "\n"); out.flush(); print(s, flush=True)


base = read_fvecs(f'{SIFT}/sift_base.fvecs', N)
queries = read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
gt = read_ivecs(f'{SIFT}/sift_groundtruth.ivecs', NQ)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit = True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS sift")
cur.execute("CREATE TABLE sift (id int, e vector(128))")
buf = io.StringIO()
for i, v in enumerate(base):
    buf.write(f"{i}\t[{','.join(map(str, v))}]\n")
buf.seek(0); cur.copy_expert("COPY sift FROM STDIN", buf)
emit(f"FSAB_LOADED n={len(base)} nq={NQ} degree={DEGREE}")
t = time.time()
cur.execute(f"CREATE INDEX sift_symqg ON sift USING theodb_symqg (e) WITH (degree_bound={DEGREE})")
cur.execute("SELECT pg_size_pretty(pg_relation_size('sift_symqg'))")
emit(f"FSAB_BUILD build_s={time.time()-t:.1f} size={cur.fetchone()[0]}")


def recall_qps(k=10):
    hit = 0; best = 1e9
    for r in range(3):
        ts = time.time()
        for qi in range(NQ):
            lit = '[' + ','.join(map(str, queries[qi])) + ']'
            cur.execute("SELECT id FROM sift ORDER BY e <-> %s::vector LIMIT %s", (lit, k))
            got = set(x[0] for x in cur.fetchall())
            if r == 0:
                hit += len(got & set(gt[qi][:k]))
        best = min(best, time.time() - ts)
    return hit / (NQ * k), NQ / best


for mode, guc in [("fastscan", "on"), ("scalar", "off")]:
    cur.execute(f"SET theodb.symqg_fastscan = {guc}")
    cur.execute("SET enable_seqscan=off")
    for ef in [40, 80, 160, 320, 640]:
        cur.execute(f"SET theodb_hnsw.ef_search={ef}")
        rec, qps = recall_qps()
        emit(f"FSAB_RESULT mode={mode} ef={ef} recall={rec:.4f} qps={qps:.1f}")
emit("FSAB_DONE")
