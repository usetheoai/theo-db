"""M177 — mede o footgun do ADR 0007: cada theodb.embed() segura um backend PostgreSQL inteiro?

O ADR 0007 registrou em 2026-06 que a chamada e sincrona dentro do backend e que "maquina de fila e
complexidade essencial apenas depois de um gargalo medido". As medicoes anteriores deste milestone
mediram o SERVIDOR de embeddings; esta mede o BANCO — quantos backends ficam presos, e o que isso faz
com max_connections.

Metodo: N clientes psql concorrentes chamando theodb.embed(), enquanto um observador conta
pg_stat_activity por state/wait_event. Um backend preso em ClientRead nao e o mesmo que um preso
executando: o primeiro esta ocioso, o segundo consumiu uma vaga de max_connections POR TODA a latencia
do modelo — que e o custo que o ADR nomeia.
"""
from __future__ import annotations
import argparse, json, statistics, subprocess, threading, time

def psql(sql, db="postgres"):
    return subprocess.run(["docker","exec","theodb-adr0007","psql","-U","postgres","-tAc",sql],
                          capture_output=True, text=True, timeout=180).stdout.strip()

def worker(n, out, lock):
    for _ in range(n):
        t = time.perf_counter()
        r = subprocess.run(["docker","exec","theodb-adr0007","psql","-U","postgres","-tAc",
                            "SELECT length(theodb.embed('consulta sobre banco vetorial')::text)"],
                           capture_output=True, text=True, timeout=180)
        dt = (time.perf_counter()-t)*1000
        with lock:
            out.append((dt, "ok" if r.returncode == 0 else "err"))

def observe(stop, samples):
    while not stop.is_set():
        r = psql("SELECT count(*) FILTER (WHERE state='active'), "
                 "count(*) FILTER (WHERE wait_event_type IS NOT NULL AND state='active'), "
                 "count(*) FROM pg_stat_activity WHERE backend_type='client backend'")
        if r:
            try: samples.append([int(x) for x in r.split("|")])
            except ValueError: pass
        time.sleep(0.25)

def main():
    ap = argparse.ArgumentParser(); ap.add_argument("--clients", default="1,4,8,16")
    ap.add_argument("--per-client", type=int, default=4); ap.add_argument("--json", default=None)
    a = ap.parse_args()
    maxc = int(psql("SHOW max_connections"))
    rows = []
    for c in [int(x) for x in a.clients.split(",")]:
        lat, lock, samples, stop = [], threading.Lock(), [], threading.Event()
        obs = threading.Thread(target=observe, args=(stop, samples), daemon=True); obs.start()
        t0 = time.perf_counter()
        ths = [threading.Thread(target=worker, args=(a.per_client, lat, lock)) for _ in range(c)]
        for t in ths: t.start()
        for t in ths: t.join()
        wall = time.perf_counter()-t0; stop.set(); time.sleep(0.4)
        ok = sorted(d for d, k in lat if k == "ok")
        peak_active = max((s[0] for s in samples), default=0)
        rows.append({"clients": c, "max_connections": maxc,
                     "requests": len(lat), "errors": sum(1 for _, k in lat if k == "err"),
                     "wall_s": round(wall,1), "rps": round(len(ok)/wall,2),
                     "p50_ms": round(ok[len(ok)//2],1) if ok else None,
                     "p99_ms": round(ok[min(int(len(ok)*.99),len(ok)-1)],1) if ok else None,
                     "backends_ativos_pico": peak_active,
                     "pct_de_max_connections": round(100*peak_active/maxc,1)})
        print(json.dumps(rows[-1]), flush=True)
    if a.json: json.dump({"levels": rows}, open(a.json,"w"), indent=2)

if __name__ == "__main__": raise SystemExit(main())
