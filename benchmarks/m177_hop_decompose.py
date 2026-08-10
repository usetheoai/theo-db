"""Decompõe o hop: transporte puro vs serialização do vetor. Sem modelo — só o canal."""
import json, statistics, threading, time, urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import random
random.seed(42)
VEC384=[random.random() for _ in range(384)]      # payload que um embedding devolve
VEC1024=[random.random() for _ in range(1024)]    # e5-large
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_POST(self):
        n=int(self.headers.get('Content-Length',0)); body=json.loads(self.rfile.read(n))
        vec = VEC1024 if body.get('dim')==1024 else (VEC384 if body.get('dim')==384 else [])
        out=json.dumps({"data":[{"embedding":vec}]}).encode()
        self.send_response(200); self.send_header('Content-Type','application/json')
        self.send_header('Content-Length',str(len(out))); self.end_headers(); self.wfile.write(out)
srv=ThreadingHTTPServer(('127.0.0.1',8091),H)
threading.Thread(target=srv.serve_forever,daemon=True).start(); time.sleep(0.4)
def rt(dim):
    p=json.dumps({"input":"uma consulta do usuario","dim":dim}).encode()
    r=urllib.request.Request("http://127.0.0.1:8091/v1/embeddings",data=p,headers={'Content-Type':'application/json'})
    t=time.perf_counter()
    with urllib.request.urlopen(r,timeout=10) as resp: json.loads(resp.read())
    return (time.perf_counter()-t)*1000
for dim,label in ((0,'sem vetor (transporte puro)'),(384,'com vetor 384d'),(1024,'com vetor 1024d')):
    for _ in range(20): rt(dim)
    s=[rt(dim) for _ in range(200)]
    print(f"{label:<32} {statistics.mean(s):6.3f} ± {statistics.stdev(s):5.3f} ms   (mediana {statistics.median(s):.3f})")
srv.shutdown()
