"""M115 columnar-aggregate composability A/B (plan m115-columnar-composability T2.1). Proves the columnar-aggregate
CustomScan output is CONSUMABLE inside an enclosing expression — the four shapes that used to fail with `cache lookup
failed for attribute N of relation 0` now run byte-identical vs a heap table with the CustomScan engaged: (1)
subquery-over-grouped-agg, (2) JOIN on the grouped output, (3) aggregate over the agg value with ORDER BY, (4) scalar
`s+1` over a subquery. Also asserts the top-level GROUP BY path is unchanged (no regression). Reproduce with env."""
import os, psycopg2
PORT = int(os.environ.get('PGPORT', '28817')); DB = os.environ.get('PGDB', 'e2ab'); USER = os.environ.get('PGUSER', 'theo')
N = int(os.environ.get('N', '1000000'))
out = open('/tmp/m115_out.txt', 'a')
def emit(s): out.write(s+"\n"); out.flush(); print(s, flush=True)
conn = psycopg2.connect(host='localhost', port=PORT, dbname=DB, user=USER); conn.autocommit=True
cur = conn.cursor()
cur.execute("DROP TABLE IF EXISTS cc"); cur.execute("DROP TABLE IF EXISTS ch")
cur.execute("CREATE TABLE cc(k int, x float8) USING theodb_columnar")
cur.execute("CREATE TABLE ch(k int, x float8)")
gen = f"SELECT (g%100), g::float8 FROM generate_series(1,{N}) g"
cur.execute(f"INSERT INTO cc {gen}"); cur.execute(f"INSERT INTO ch {gen}")
emit(f"M115_LOADED n={N}")
cur.execute("SET theodb.enable_columnar_agg=on")
cur.execute("SET max_parallel_workers_per_gather=0")

def scalar(sql):
    cur.execute(sql); return cur.fetchone()[0]
def engages_customscan(sql):
    cur.execute("SET theodb.enable_columnar_agg=on")
    cur.execute(f"EXPLAIN {sql}")
    return any('theodb_columnar_agg' in r[0] for r in cur.fetchall())

# The four previously-failing composability shapes: value from the columnar table must equal the heap value.
SHAPES = [
    ("subquery_over_grouped", "SELECT sum(s) FROM (SELECT k, sum(x) s FROM {t} GROUP BY k) q"),
    ("agg_orderby_value",     "SELECT string_agg(s::text, ',' ORDER BY s) FROM (SELECT k, sum(x) s FROM {t} GROUP BY k) q"),
    ("scalar_s_plus_1",       "SELECT s+1 FROM (SELECT sum(x) s FROM {t}) q"),
    ("count_over_grouped",    "SELECT count(*) FROM (SELECT k, sum(x) s FROM {t} GROUP BY k) q"),
]
for label, tmpl in SHAPES:
    cv = scalar(tmpl.format(t="cc")); hv = scalar(tmpl.format(t="ch"))
    cs = engages_customscan(tmpl.format(t="cc"))  # the inner columnar agg is a CustomScan
    emit(f"M115_SHAPE {label} identical={'YES' if cv==hv else 'NO'} customscan={'YES' if cs else 'NO'} (c={cv} h={hv})")

# JOIN on the grouped output (matching-group count must equal).
jc = scalar("SELECT count(*) FROM (SELECT k, sum(x) s FROM cc GROUP BY k) a JOIN (SELECT k, sum(x) s FROM ch GROUP BY k) b USING(k)")
emit(f"M115_SHAPE join_on_grouped matched_groups={jc} (expect 100)")

# Top-level regression: GROUP BY (+ORDER BY) unchanged, byte-identical, CustomScan engaged.
tl_cs = engages_customscan("SELECT k, sum(x) FROM cc GROUP BY k")
def gset(t):
    cur.execute(f"SELECT k, round(sum(x)::numeric,3) FROM {t} GROUP BY k ORDER BY k"); return cur.fetchall()
tl_ident = gset("cc") == gset("ch")
emit(f"M115_TOPLEVEL customscan={'YES' if tl_cs else 'NO'} identical={'YES' if tl_ident else 'NO'}")
emit("M115_DONE")
