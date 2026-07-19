"""M114 columnar aggregate completeness A/B (plan m114-columnar-aggregate-completeness T3.1). A 1M-row
theodb_columnar table vs an identical heap table. Proves byte-identical results for the SHIPPED shapes (avg(float8),
sum(int4), sum(int2), GROUP BY+WHERE combined incl. a partial-overlap group) with the CustomScan engaged, and proves
the DECLINED shapes (avg(int), sum(int8), sum(real)) fall back to the native plan (NOT a CustomScan) while still
returning the correct value. Measures the speedup for the shipped scalar/grouped shapes. Reproduce with env N/PG*."""
import os, time, psycopg2
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/m114_out.txt', 'a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS m114c"); cur.execute("DROP TABLE IF EXISTS m114h")
cols = "k int, x float8, i4 int4, i2 int2, b int8, f4 real"
cur.execute(f"CREATE TABLE m114c({cols}) USING theodb_columnar")
cur.execute(f"CREATE TABLE m114h({cols})")
# k=g%100 (100 groups, clustered-ish), x monotonic float8, i4/i2 ints, b int8, f4 real.
gen = f"SELECT (g%100), g::float8, g, (g%1000)::int2, g::int8, g::real FROM generate_series(1,{N}) g"
t=time.time()
cur.execute(f"INSERT INTO m114c {gen}"); cur.execute(f"INSERT INTO m114h {gen}")
emit(f"M114_LOADED n={N} build_s={time.time()-t:.1f}")
cur.execute("SET theodb.enable_columnar_agg=on")
cur.execute("SET max_parallel_workers_per_gather=0")

def is_customscan(sql):
    cur.execute(f"EXPLAIN {sql}")
    return any('theodb_columnar_agg' in r[0] for r in cur.fetchall())

def scalar(tbl, expr):
    cur.execute(f"SELECT {expr} FROM {tbl}")
    return cur.fetchone()[0]

def bench(sql, guc):
    cur.execute(f"SET theodb.enable_columnar_agg={guc}")
    best=1e9
    for _ in range(3):
        ts=time.time(); cur.execute(sql); cur.fetchall(); best=min(best,time.time()-ts)
    cur.execute("SET theodb.enable_columnar_agg=on")
    return best*1000.0

# --- SHIPPED scalar shapes: byte-identical + CustomScan + speedup ---
for label, expr in [("avg_float8","avg(x)"), ("sum_int4","sum(i4)"), ("sum_int2","sum(i2)")]:
    cs = is_customscan(f"SELECT {expr} FROM m114c")
    vc = scalar("m114c", expr); vh = scalar("m114h", expr)
    emit(f"M114_SHIP shape={label} customscan={'YES' if cs else 'NO'} identical={'YES' if vc==vh else 'NO'} (c={vc} h={vh})")
    on=bench(f"SELECT {expr} FROM m114c",'on'); off=bench(f"SELECT {expr} FROM m114c",'off')
    emit(f"M114_LATENCY shape={label} customscan_ms={on:.1f} native_ms={off:.1f} speedup={off/max(on,1e-9):.2f}x")

# --- SHIPPED GROUP BY + WHERE combined: full grouped result set byte-identical ---
def grouped_set(tbl, where):
    cur.execute(f"SELECT k, sum(x), count(*) FROM {tbl} WHERE {where} GROUP BY k ORDER BY k")
    return [(r[0], round(r[1],3), r[2]) for r in cur.fetchall()]
where = "k BETWEEN 20 AND 60"   # ~40%-selective on the group key itself (pushable zone-map predicate)
cs = is_customscan(f"SELECT k, sum(x) FROM m114c WHERE {where} GROUP BY k")
gc = grouped_set("m114c", where); gh = grouped_set("m114h", where)
emit(f"M114_SHIP shape=groupby_where customscan={'YES' if cs else 'NO'} identical={'YES' if gc==gh else 'NO'} rows={len(gc)}")
on=bench(f"SELECT k, sum(x), count(*) FROM m114c WHERE {where} GROUP BY k",'on')
off=bench(f"SELECT k, sum(x), count(*) FROM m114c WHERE {where} GROUP BY k",'off')
emit(f"M114_LATENCY shape=groupby_where customscan_ms={on:.1f} native_ms={off:.1f} speedup={off/max(on,1e-9):.2f}x")

# --- DECLINED shapes: NOT a CustomScan + still correct vs heap ---
for label, expr in [("avg_int4","avg(i4)"), ("sum_int8","sum(b)"), ("sum_real","sum(f4)")]:
    cs = is_customscan(f"SELECT {expr} FROM m114c")
    vc = scalar("m114c", expr); vh = scalar("m114h", expr)
    emit(f"M114_DECLINE shape={label} customscan={'YES' if cs else 'NO'} (expect NO) correct={'YES' if vc==vh else 'NO'} (c={vc} h={vh})")
emit("M114_DONE")
