"""Columnar zone-map skip-pruning A/B for TEMPORAL columns (timestamptz + date). Extends columnar_zonemap_ab.py
(which covers int/float) to the most common analytical filter: a time-range on a time-series column. A clustered
1M-row theodb_columnar table, monotonic `ts` (→ tight per-chunk-group ranges), a ~10%-selective time-range
filtered aggregate run with theodb.columnar_zonemap_skip ON vs OFF (same table). Asserts byte-identical result
(D3 gate) and measures the skip ratio + wall-clock latency, for BOTH the `timestamptz` and the `date` column.
Goal: skip decodes <= 25% of the chunk groups skip-off decodes, byte-identical."""
import os, time, psycopg2
from datetime import datetime, timedelta, timezone
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/cztab_out.txt', 'a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS czt")
cur.execute("CREATE TABLE czt(id int, ts timestamptz, d date, x float8) USING theodb_columnar")
# clustered / time-series: monotonic ts at 1-minute steps from 2020-01-01 → sorted → 100 tight chunk groups.
t=time.time()
cur.execute(f"""INSERT INTO czt
  SELECT g,
         timestamptz '2020-01-01 00:00:00+00' + (g * interval '1 minute'),
         (timestamptz '2020-01-01 00:00:00+00' + (g * interval '1 minute'))::date,
         (g%7)::float8
  FROM generate_series(1,{N}) g""")
emit(f"CZTAB_LOADED n={N} build_s={time.time()-t:.1f}")
cur.execute("SET theodb.enable_columnar_agg=on")

def bench(col, lo_expr, hi_expr, skip):
    q = f"SELECT sum(x) FROM czt WHERE {col} BETWEEN {lo_expr} AND {hi_expr}"
    cur.execute(f"SET theodb.columnar_zonemap_skip={skip}")
    res=None; best=1e9
    for _ in range(3):
        ts=time.time(); cur.execute(q); res=cur.fetchone()[0]; best=min(best, time.time()-ts)
    return res, best*1000.0

# ~10%-selective time-range in the middle (minutes 45%..55% of N). Pass LITERAL timestamp/date constants (not
# `base + interval` arithmetic) so the qual RHS is a folded T_Const → extract_zone_predicate can push it (a
# non-Const RHS would decline to the native plan and the skip would silently never engage — false negative).
base = datetime(2020, 1, 1, tzinfo=timezone.utc)
lo_dt = base + timedelta(minutes=int(N*0.45)); hi_dt = base + timedelta(minutes=int(N*0.55))
lo_ts = f"timestamptz '{lo_dt.strftime('%Y-%m-%d %H:%M:%S+00')}'"
hi_ts = f"timestamptz '{hi_dt.strftime('%Y-%m-%d %H:%M:%S+00')}'"
lo_d  = f"date '{lo_dt.strftime('%Y-%m-%d')}'"
hi_d  = f"date '{hi_dt.strftime('%Y-%m-%d')}'"

for label, col, lo, hi in [("timestamptz", "ts", lo_ts, hi_ts), ("date", "d", lo_d, hi_d)]:
    # M99 gotcha: without the CustomScan engaged the WHERE runs in the native plan → "identical" would be trivial.
    cur.execute("SET theodb.columnar_zonemap_skip=on")
    cur.execute(f"EXPLAIN SELECT sum(x) FROM czt WHERE {col} BETWEEN {lo} AND {hi}")
    plan = "\n".join(r[0] for r in cur.fetchall())
    emit(f"CZTAB_EXPLAIN col={label} customscan={'YES' if 'theodb_columnar_agg' in plan else 'NO'}")
    on_res, on_ms = bench(col, lo, hi, 'on')
    off_res, off_ms = bench(col, lo, hi, 'off')
    ok = 'YES' if on_res==off_res else 'NO'
    emit(f"CZTAB_RESULT col={label} skip=on  sum={on_res} ms={on_ms:.1f}")
    emit(f"CZTAB_RESULT col={label} skip=off sum={off_res} ms={off_ms:.1f}")
    emit(f"CZTAB_CORRECT col={label} byte_identical={ok} (on={on_res} off={off_res})")
    emit(f"CZTAB_LATENCY col={label} skip_speedup={off_ms/max(on_ms,1e-9):.2f}x")
emit("CZTAB_DONE")
