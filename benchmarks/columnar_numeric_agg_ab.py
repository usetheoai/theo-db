"""Numeric-output aggregates A/B (plan numeric-output-aggregates T3.1). A 1M-row theodb_columnar table vs an
identical heap table. Proves BYTE-IDENTICAL results — compared as TEXT so any scale/rounding drift fails — for
sum(int8)->numeric and avg(int2/4/8)->numeric with the CustomScan engaged, across magnitudes that exercise PG's
DATA-DEPENDENT avg scale (16 sig-digits for small sums, shrinking as the sum grows to scale 0) AND a sum exceeding
i64 (the Decimal128 i128 exactness path — an Int64 sum would silently wrap). GROUP BY + scalar + empty-group NULL.
Measures the speedup for the shipped shapes. Reproduce with env N/PG*."""
import os, time, psycopg2
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/numagg_out.txt', 'a')
def emit(s): out.write(s + "\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit = True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS numc"); cur.execute("DROP TABLE IF EXISTS numh")
# g%4 groups; s2/s4 small ints (avg scale ~16), s8 ~1e6..1e12 (avg scale shrinks), big=1e13 whose sum over N rows
# EXCEEDS i64 max (9.2e18) — a wrapping Int64 sum would go negative here, so an identical result PROVES the exact
# Decimal128/i128 path is load-bearing. All columns min/max-able native ints.
cols = "g int, s2 int2, s4 int4, s8 int8, big int8"
cur.execute(f"CREATE TABLE numc({cols}) USING theodb_columnar")
cur.execute(f"CREATE TABLE numh({cols})")
gen = f"SELECT (g%4), (g%100)::int2, g, (g::int8*1000000), 10000000000000::int8 FROM generate_series(1,{N}) g"
t = time.time()
cur.execute(f"INSERT INTO numc {gen}"); cur.execute(f"INSERT INTO numh {gen}")
emit(f"NUMAGG_LOADED n={N} build_s={time.time()-t:.1f}")
cur.execute("SET theodb.enable_columnar_agg=on")
cur.execute("SET max_parallel_workers_per_gather=0")

def is_customscan(sql):
    cur.execute(f"EXPLAIN {sql}")
    return any('theodb_columnar_agg' in r[0] for r in cur.fetchall())

def scalar_text(tbl, expr):
    cur.execute(f"SELECT ({expr})::text FROM {tbl}")
    return cur.fetchone()[0]

def bench(sql, guc):
    cur.execute(f"SET theodb.enable_columnar_agg={guc}")
    best = 1e9
    for _ in range(3):
        ts = time.time(); cur.execute(sql); cur.fetchall(); best = min(best, time.time() - ts)
    cur.execute("SET theodb.enable_columnar_agg=on")
    return best * 1000.0

all_identical = True
# --- Scalar numeric-output shapes: byte-identical (as TEXT) + CustomScan + speedup ---
for label, expr in [
    ("sum_int8", "sum(s8)"),        # exact scale-0 numeric, within i64
    ("sum_int8_over_i64", "sum(big)"),  # 1e13 * 1M rows = 1e19 > i64 max (9.2e18): proves the Decimal128 i128 path
    ("avg_int2_scale16", "avg(s2)"),    # small sum -> avg scale 16
    ("avg_int4_shrink", "avg(s4)"),     # ~5e11 sum -> avg scale shrinks
    ("avg_int8_shrink", "avg(s8)"),     # large sum -> avg scale 0-ish
]:
    cs = is_customscan(f"SELECT {expr} FROM numc")
    vc = scalar_text("numc", expr); vh = scalar_text("numh", expr)
    ident = (vc == vh)
    all_identical = all_identical and ident and cs
    emit(f"NUMAGG_SHAPE shape={label} customscan={'YES' if cs else 'NO'} identical={'YES' if ident else 'NO'} (c={vc} h={vh})")
    on = bench(f"SELECT {expr} FROM numc", 'on'); off = bench(f"SELECT {expr} FROM numc", 'off')
    emit(f"NUMAGG_LATENCY shape={label} customscan_ms={on:.1f} native_ms={off:.1f} speedup={off/max(on,1e-9):.2f}x")

# --- GROUP BY: full grouped numeric result set byte-identical (as text) ---
def grouped_text(tbl, expr):
    cur.execute(f"SELECT g, ({expr})::text FROM {tbl} GROUP BY g ORDER BY g")
    return [(r[0], r[1]) for r in cur.fetchall()]
for label, expr in [("groupby_sum_int8", "sum(s8)"), ("groupby_avg_int4", "avg(s4)")]:
    cs = is_customscan(f"SELECT g, {expr} FROM numc GROUP BY g")
    gc = grouped_text("numc", expr); gh = grouped_text("numh", expr)
    ident = (gc == gh)
    all_identical = all_identical and ident and cs
    emit(f"NUMAGG_SHAPE shape={label} customscan={'YES' if cs else 'NO'} identical={'YES' if ident else 'NO'} rows={len(gc)}")

# --- Empty group -> NULL (zero-count guard), matching PG's finalfn ---
cur.execute("SELECT (avg(s4))::text FROM numc WHERE g < 0")
en = cur.fetchone()[0]
emit(f"NUMAGG_EMPTY avg_over_empty={'NULL' if en is None else en} pass={'YES' if en is None else 'NO'}")
all_identical = all_identical and (en is None)

emit(f"NUMAGG_VERDICT all_identical_and_customscan={'YES' if all_identical else 'NO'}")
out.close()
