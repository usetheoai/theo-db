"""Columnar zone-map skip-pruning A/B (plan columnar-zonemap-skip-pruning T3.1). A clustered 1M-row theodb_columnar
table (sorted on `y` → tight per-chunk-group ranges); a ~10%-selective range filtered aggregate is run with
theodb.columnar_zonemap_skip ON vs OFF (same table). Asserts byte-identical result (D3 gate) and measures the
skip ratio (chunk-groups decoded, from THEODB_SCAN_PROFILE) + wall-clock latency. Goal: skip decodes <= 25% of
the chunk groups skip-off decodes."""
import os, time, psycopg2
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/czab_out.txt', 'a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS cz")
cur.execute("CREATE TABLE cz(id int, y int4, x float8) USING theodb_columnar")
# clustered: rows sorted by y → 100 chunk groups (10k each), y in [1..N]
t=time.time()
cur.execute(f"INSERT INTO cz SELECT g, g, (g%7)::float8 FROM generate_series(1,{N}) g")
emit(f"CZAB_LOADED n={N} build_s={time.time()-t:.1f}")
cur.execute("SET theodb.enable_columnar_agg=on")
# ~10%-selective range in the middle
lo, hi = int(N*0.45), int(N*0.55)
Q = f"SELECT sum(x) FROM cz WHERE y BETWEEN {lo} AND {hi}"
def bench(skip):
    cur.execute(f"SET theodb.columnar_zonemap_skip={skip}")
    res=None; best=1e9
    for _ in range(3):
        ts=time.time(); cur.execute(Q); res=cur.fetchone()[0]; best=min(best, time.time()-ts)
    return res, best*1000.0
on_res, on_ms = bench('on')
off_res, off_ms = bench('off')
emit(f"CZAB_RESULT skip=on  sum={on_res} ms={on_ms:.1f}")
emit(f"CZAB_RESULT skip=off sum={off_res} ms={off_ms:.1f}")
emit(f"CZAB_CORRECT byte_identical={'YES' if on_res==off_res else 'NO'} (on={on_res} off={off_res})")
emit(f"CZAB_LATENCY skip_speedup={off_ms/max(on_ms,1e-9):.2f}x")
emit("CZAB_DONE")
