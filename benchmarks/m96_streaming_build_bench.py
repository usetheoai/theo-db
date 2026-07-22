#!/usr/bin/env python3
# M96 — streaming ambuild peak-RSS gate. Build an IVF-AQ v5 index at N vectors and capture the backend's peak RSS.
# Streaming (maintenance_work_mem << corpus) should peak at O(mwm + sample), independent of N — vs the M88 in-RAM
# build that peaked ~4.21x base (OOM at 30M). We measure the streaming peak and compare to the base-dataset size.
#
# Peak RSS is read from the backend's /proc/<pid>/status VmHWM during the build (sampled in a thread), since the
# build runs inside the PG backend, not this python process.
import os, time, threading
import psycopg2

PORT = int(os.environ.get('PORT', '28817')); N = int(os.environ.get('N', '30000000')); DIM = 128
LISTS = int(os.environ.get('LISTS', '1000')); MWM = os.environ.get('MWM', '2GB')
SEED = 42

def emit(s): print(s, flush=True)

conn = psycopg2.connect(host='localhost', port=PORT, dbname='postgres', user='theo'); conn.autocommit = True
cur = conn.cursor()
cur.execute("SELECT pg_backend_pid()"); pid = cur.fetchone()[0]
emit(f"M96 backend_pid={pid} N={N} dim={DIM} lists={LISTS} mwm={MWM}")

# Generate N random-ish vectors deterministically via generate_series (no host RAM — server-side COPY-free INSERT).
cur.execute("DROP TABLE IF EXISTS sb")
cur.execute("CREATE TABLE sb (id int, e vector(%s))" % DIM)
emit("M96_TABLE_CREATED")
# Server-side generation: each row's vector = a cheap deterministic pseudo-random function of id (no client transfer).
t = time.time()
cur.execute(f"""
  INSERT INTO sb
  SELECT g, (SELECT array_agg(((g*31 + s*17) % 997)::real) FROM generate_series(1,{DIM}) s)::vector
  FROM generate_series(1,{N}) g
""")
emit(f"M96_INSERTED {time.time()-t:.0f}s")
cur.execute("ANALYZE sb"); emit("M96_ANALYZED")
base_bytes = N * DIM * 4
emit(f"M96_BASE_BYTES {base_bytes} ({base_bytes/1e9:.1f} GB)")

# Sample the backend's peak RSS (VmHWM resets are not possible; read VmHWM = high-water mark since process start).
peak = {'hwm_kb': 0, 'stop': False}
def sampler():
    while not peak['stop']:
        try:
            with open(f"/proc/{pid}/status") as f:
                for line in f:
                    if line.startswith('VmHWM:'):
                        peak['hwm_kb'] = max(peak['hwm_kb'], int(line.split()[1])); break
        except Exception:
            pass
        time.sleep(0.5)
th = threading.Thread(target=sampler); th.start()

cur.execute(f"SET maintenance_work_mem = '{MWM}'")
t = time.time()
cur.execute(f"CREATE INDEX sb_e ON sb USING theodb_ivfflat (e) WITH (lists={LISTS}, pq_subspaces=32, aq_threshold=2000, separate_storage=1)")
build_s = time.time() - t
peak['stop'] = True; th.join()
hwm_gb = peak['hwm_kb'] / 1e6
emit(f"M96_BUILD {build_s:.0f}s peak_rss={hwm_gb:.2f}GB ratio_vs_base={hwm_gb*1e9/base_bytes:.3f}x")
# Sanity: the streamed index scans.
cur.execute("SET enable_seqscan=off; SET theodb_ivfflat.probes=16")
cur.execute("SELECT count(*) FROM (SELECT id FROM sb ORDER BY e <-> (SELECT e FROM sb WHERE id=1) LIMIT 10) t")
emit(f"M96_SCAN_OK topk={cur.fetchone()[0]}")
cur.execute("DROP TABLE sb"); emit("M96_DONE")
