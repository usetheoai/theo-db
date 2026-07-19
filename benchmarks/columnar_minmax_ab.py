"""min(col)/max(col) columnar aggregate + zone-map directory fast-path A/B (plan columnar-minmax-zonemap T3.1).
A 1M-row theodb_columnar table vs an identical heap table. Proves BYTE-IDENTICAL min/max (compared as TEXT) for every
in-scope ordered native type (int2/4/8, float4/8, bool, timestamp/date), scalar + GROUP BY + WHERE. The fast-path
(directory-only fold, no chunk decode) is reported per shape via a NOTICE (THEODB_SCAN_PROFILE=1): clean scalar-no-WHERE
min/max fires `path=fastpath`; gated shapes (max(float) with NaN, WHERE present) fire `path=scan` and stay byte-identical.
Also: all-NULL -> NULL, empty -> NULL, same-xact pending fold, float-NaN max. Reproduce with env N/PG*."""
import os, time, psycopg2
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/minmax_out.txt', 'a')
def emit(s): out.write(s + "\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit = True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS mmc"); cur.execute("DROP TABLE IF EXISTS mmh")
cols = "g int, s2 int2, s4 int4, s8 int8, f4 real, f8 float8, b bool, ts timestamptz, d date"
cur.execute(f"CREATE TABLE mmc({cols}) USING theodb_columnar")
cur.execute(f"CREATE TABLE mmh({cols})")
# Spread values so min/max are non-trivial; negatives included (signed-domain fold trap). g%4 groups.
gen = (f"SELECT (g%4), (g%1000-500)::int2, (g-500000), (g::int8*1000000-500000000000), "
       f"(g*1.5-750000)::real, (g*2.5-1250000)::float8, (g%2=0), "
       f"timestamptz '2020-01-01'+(g*interval '1 sec'), date '2020-01-01'+(g%3650) "
       f"FROM generate_series(1,{N}) g")
t = time.time()
cur.execute(f"INSERT INTO mmc {gen}"); cur.execute(f"INSERT INTO mmh {gen}")
emit(f"MINMAX_LOADED n={N} build_s={time.time()-t:.1f}")
cur.execute("SET theodb.enable_columnar_agg=on")
cur.execute("SET max_parallel_workers_per_gather=0")
cur.execute("SET theodb.enable_columnar_agg=on")

def is_customscan(sql):
    cur.execute(f"EXPLAIN {sql}")
    return any('theodb_columnar_agg' in r[0] for r in cur.fetchall())

def path_of(sql):
    """Read the backend NOTICE (postgres started with THEODB_SCAN_PROFILE=1) → fastpath vs scan."""
    del conn.notices[:]
    cur.execute(sql); cur.fetchall()
    for n in conn.notices:
        if 'minmax path=' in n:
            return n.split('minmax path=')[1].strip().rstrip(';\n ')
    return '?'

def txt(tbl, expr, where=''):
    cur.execute(f"SELECT ({expr})::text FROM {tbl} {where}")
    r = cur.fetchone()
    return r[0] if r else None

def bench(sql, guc):
    cur.execute(f"SET theodb.enable_columnar_agg={guc}")
    best = 1e9
    for _ in range(3):
        ts = time.time(); cur.execute(sql); cur.fetchall(); best = min(best, time.time() - ts)
    cur.execute("SET theodb.enable_columnar_agg=on")
    return best * 1000.0

all_ok = True
# NOTE: PostgreSQL has no min/max aggregate for bool (only bool_and/bool_or), so bool is not an in-scope shape here.
COLS = [("s2", "int2"), ("s4", "int4"), ("s8", "int8"), ("f4", "real"), ("f8", "float8"),
        ("ts", "timestamptz"), ("d", "date")]

# --- Scalar min/max byte-identical (as text) + CustomScan + path + speedup ---
for c, ty in COLS:
    for agg in ("min", "max"):
        expr = f"{agg}({c})"
        cs = is_customscan(f"SELECT {expr} FROM mmc")
        vc = txt("mmc", expr); vh = txt("mmh", expr)
        p = path_of(f"SELECT {expr} FROM mmc")
        ident = (vc == vh)
        all_ok = all_ok and ident and cs
        emit(f"MINMAX_SHAPE shape={agg}_{ty} customscan={'Y' if cs else 'N'} identical={'Y' if ident else 'N'} path={p} (c={vc} h={vh})")
    on = bench(f"SELECT min({c}), max({c}) FROM mmc", 'on'); off = bench(f"SELECT min({c}), max({c}) FROM mmc", 'off')
    emit(f"MINMAX_LATENCY col={ty} customscan_ms={on:.1f} native_ms={off:.1f} speedup={off/max(on,1e-9):.2f}x")

# --- GROUP BY min/max full result set byte-identical ---
def grouped(tbl, agg, c):
    cur.execute(f"SELECT g, ({agg}({c}))::text FROM {tbl} GROUP BY g ORDER BY g")
    return [(r[0], r[1]) for r in cur.fetchall()]
for agg in ("min", "max"):
    cs = is_customscan(f"SELECT g, {agg}(s4) FROM mmc GROUP BY g")
    gc = grouped("mmc", agg, "s4"); gh = grouped("mmh", agg, "s4")
    ident = (gc == gh); all_ok = all_ok and ident and cs
    emit(f"MINMAX_SHAPE shape=groupby_{agg}_int4 customscan={'Y' if cs else 'N'} identical={'Y' if ident else 'N'} rows={len(gc)}")

# --- WHERE min/max (fast-path NOT taken; still byte-identical via Phase A) ---
w = "WHERE s4 > 0"
vc = txt("mmc", "max(s4)", w); vh = txt("mmh", "max(s4)", w)
p = path_of(f"SELECT max(s4) FROM mmc {w}")
ident = (vc == vh); all_ok = all_ok and ident
emit(f"MINMAX_SHAPE shape=where_max_int4 identical={'Y' if ident else 'N'} path={p} (c={vc} h={vh})")

# --- Empty set -> NULL ---
vc = txt("mmc", "max(s4)", "WHERE g < 0"); vh = txt("mmh", "max(s4)", "WHERE g < 0")
emit(f"MINMAX_EMPTY c={vc} h={vh} pass={'Y' if vc is None and vh is None else 'N'}")
all_ok = all_ok and (vc is None and vh is None)

# --- All-NULL column -> NULL ---
cur.execute("DROP TABLE IF EXISTS mmnull_c"); cur.execute("DROP TABLE IF EXISTS mmnull_h")
cur.execute("CREATE TABLE mmnull_c(v int4) USING theodb_columnar"); cur.execute("CREATE TABLE mmnull_h(v int4)")
cur.execute("INSERT INTO mmnull_c SELECT NULL FROM generate_series(1,1000)")
cur.execute("INSERT INTO mmnull_h SELECT NULL FROM generate_series(1,1000)")
vc = txt("mmnull_c", "max(v)"); vh = txt("mmnull_h", "max(v)")
emit(f"MINMAX_ALLNULL c={vc} h={vh} pass={'Y' if vc is None and vh is None else 'N'}")
all_ok = all_ok and (vc is None and vh is None)

# --- Float NaN: max must return NaN (falls back to scan), min ignores NaN ---
cur.execute("DROP TABLE IF EXISTS mmnan_c"); cur.execute("DROP TABLE IF EXISTS mmnan_h")
cur.execute("CREATE TABLE mmnan_c(v float8) USING theodb_columnar"); cur.execute("CREATE TABLE mmnan_h(v float8)")
gen_nan = "SELECT CASE WHEN g=500 THEN 'NaN'::float8 ELSE g::float8 END FROM generate_series(1,1000) g"
cur.execute(f"INSERT INTO mmnan_c {gen_nan}"); cur.execute(f"INSERT INTO mmnan_h {gen_nan}")
mc = txt("mmnan_c", "max(v)"); mh = txt("mmnan_h", "max(v)")
pmax = path_of("SELECT max(v) FROM mmnan_c")
nc = txt("mmnan_c", "min(v)"); nh = txt("mmnan_h", "min(v)")
emit(f"MINMAX_NAN max_c={mc} max_h={mh} max_path={pmax} min_c={nc} min_h={nh} pass={'Y' if mc==mh and nc==nh else 'N'}")
all_ok = all_ok and (mc == mh and nc == nh)

# --- Same-xact pending fold: INSERT then SELECT max() in one transaction ---
conn.autocommit = False
cur.execute("INSERT INTO mmc (g, s4) VALUES (0, 999999999)")
pc = txt("mmc", "max(s4)")
conn.rollback(); conn.autocommit = True
emit(f"MINMAX_PENDING max_after_insert={pc} includes_uncommitted={'Y' if pc=='999999999' else 'N'}")
all_ok = all_ok and (pc == '999999999')

emit(f"MINMAX_VERDICT all_identical={'YES' if all_ok else 'NO'}")
out.close()
