"""E2 T5.1 — in-PG A/B: theodb_symqg vs theodb_hnsw on SIFT1M. The GOAL's gate: symqg QPS >= 1.5x theodb_hnsw
at matched recall@10 >= 0.95 (settles the per-hop random-page-read tax the off-PG spike could not). Builds both
indexes on the same table, sweeps ef_search (both AMs read the theodb_hnsw.ef_search GUC), measures recall@10 vs
the official groundtruth + QPS (best-of-3) + index size. Mirror of benchmarks/e1_rabitq_bench.py."""
import struct, os, time
import psycopg2
SIFT = os.environ.get('SIFT', '/home/theo'); PORT = int(os.environ.get('PGPORT', '28817'))
DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000')); NQ = int(os.environ.get('NQ', '200'))
DEGREE = int(os.environ.get('DEGREE', '32'))


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


out = open('/tmp/e2ab_out.txt', 'a')


def emit(s):
    out.write(s + "\n"); out.flush(); print(s, flush=True)


base = read_fvecs(f'{SIFT}/sift_base.fvecs', N)
queries = read_fvecs(f'{SIFT}/sift_query.fvecs', NQ)
gt = read_ivecs(f'{SIFT}/sift_groundtruth.ivecs', NQ)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit = True
cur = conn.cursor()
import io
cur.execute("DROP TABLE IF EXISTS sift")
cur.execute("CREATE TABLE sift (id int, e vector(128))")
buf = io.StringIO()
for i, v in enumerate(base):
    buf.write(f"{i}\t[{','.join(map(str, v))}]\n")
buf.seek(0); cur.copy_expert("COPY sift FROM STDIN", buf)
emit(f"E2AB_LOADED n={len(base)} nq={NQ} degree={DEGREE}")


def build(am, name, wc=""):
    cur.execute(f"DROP INDEX IF EXISTS {name}")
    t = time.time()
    cur.execute(f"CREATE INDEX {name} ON sift USING {am} (e){wc}")
    secs = time.time() - t
    cur.execute(f"SELECT pg_size_pretty(pg_relation_size('{name}'))")
    return secs, cur.fetchone()[0]


def recall_qps(ef, k=10):
    cur.execute(f"SET theodb_hnsw.ef_search={ef}")
    cur.execute("SET enable_seqscan=off")
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


for am, name, wc in [("theodb_symqg", "sift_symqg", f" WITH (degree_bound={DEGREE})"),
                     ("theodb_hnsw", "sift_hnsw", "")]:
    secs, size = build(am, name, wc)
    emit(f"E2AB_BUILD am={am} build_s={secs:.1f} size={size}")
    for ef in [40, 80, 160, 320, 640]:
        rec, qps = recall_qps(ef)
        emit(f"E2AB_RESULT am={am} ef={ef} recall={rec:.4f} qps={qps:.1f}")
    cur.execute(f"DROP INDEX {name}")  # isolate the next AM's scan
emit("E2AB_DONE")
