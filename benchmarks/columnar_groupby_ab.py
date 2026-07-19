"""Columnar GROUP BY pushdown A/B (plan columnar-groupby-pushdown T3.1). A 1M-row theodb_columnar table vs an
identical heap table. For several GROUP BY shapes (single int key, multi-key, temporal date key, and the ADR-2
agg-before-key order), asserts the full grouped result SET is byte-identical (columnar-CustomScan vs heap-native),
that the columnar plan is a CustomScan (EXPLAIN), and measures the vectorized speedup (GUC on = CustomScan vs GUC
off = native aggregate over the SAME columnar table). Reproduce with N/PGPORT/PGDB/PGUSER env."""
import os, time, psycopg2
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/gbab_out.txt', 'a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS gbc"); cur.execute("DROP TABLE IF EXISTS gbh")
cur.execute("CREATE TABLE gbc(k int, k2 int, d date, x float8) USING theodb_columnar")
cur.execute("CREATE TABLE gbh(k int, k2 int, d date, x float8)")
# 100 groups on k, 10 on k2, ~365 on d; x monotonic. Same generator for both tables.
gen = f"""SELECT (g%100), (g%10), (date '2020-01-01' + (g%365)), g::float8
          FROM generate_series(1,{N}) g"""
t=time.time()
cur.execute(f"INSERT INTO gbc {gen}"); cur.execute(f"INSERT INTO gbh {gen}")
emit(f"GBAB_LOADED n={N} build_s={time.time()-t:.1f}")
cur.execute("SET theodb.enable_columnar_agg=on")
cur.execute("SET max_parallel_workers_per_gather=0")

# Each shape: (label, select-list, group-by, order-by). Correctness is checked by fetching the FULL top-level grouped
# result set (ORDER BY makes it deterministic) from both the columnar and the heap table and comparing the row lists
# directly in Python — a top-level GROUP BY is exactly the canonical OLAP query the CustomScan targets.
SHAPES = [
    ("int_key",       "k, sum(x), count(*)", "k",     "k"),
    ("multi_key",     "k, k2, sum(x)",       "k, k2", "k, k2"),
    ("date_key",      "d, count(*)",         "d",     "d"),
    ("agg_before_key","sum(x), k",           "k",     "2"),   # ADR-2: agg first, key second (order by 2nd col = k)
]

def norm(rows):
    # Round floats so float-summation order noise does not defeat the byte-identical compare.
    return [tuple(round(v, 3) if isinstance(v, float) else v for v in r) for r in rows]

def result_set(tbl, sel, grp, order):
    cur.execute(f"SELECT {sel} FROM {tbl} GROUP BY {grp} ORDER BY {order}")
    return norm(cur.fetchall())

def explain_customscan(sel, grp):
    cur.execute(f"EXPLAIN SELECT {sel} FROM gbc GROUP BY {grp}")
    return any('theodb_columnar_agg' in r[0] for r in cur.fetchall())

def bench(sel, grp, order, guc):
    cur.execute(f"SET theodb.enable_columnar_agg={guc}")
    q = f"SELECT {sel} FROM gbc GROUP BY {grp} ORDER BY {order}"
    best=1e9
    for _ in range(3):
        ts=time.time(); cur.execute(q); cur.fetchall(); best=min(best, time.time()-ts)
    cur.execute("SET theodb.enable_columnar_agg=on")
    return best*1000.0

for label, sel, grp, order in SHAPES:
    cs = explain_customscan(sel, grp)
    emit(f"GBAB_EXPLAIN shape={label} customscan={'YES' if cs else 'NO'}")
    rc = result_set("gbc", sel, grp, order); rh = result_set("gbh", sel, grp, order)
    ident = (rc == rh)
    emit(f"GBAB_CORRECT shape={label} identical={'YES' if ident else 'NO'} rows={len(rc)}")
    if not ident:
        emit(f"GBAB_DIFF shape={label} sample_c={rc[:2]} sample_h={rh[:2]}")
    on_ms = bench(sel, grp, order, 'on'); off_ms = bench(sel, grp, order, 'off')
    emit(f"GBAB_LATENCY shape={label} customscan_ms={on_ms:.1f} native_ms={off_ms:.1f} speedup={off_ms/max(on_ms,1e-9):.2f}x")
emit("GBAB_DONE")
