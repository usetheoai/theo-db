"""Conexao nova por chamada (o que minreq faz) vs conexao reutilizada. Sem modelo — so o canal."""
import http.client, json, statistics, threading, time, urllib.request, random
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
random.seed(42); VEC=[random.random() for _ in range(384)]
class H(BaseHTTPRequestHandler):
    disable_nagle_algorithm=True
    protocol_version='HTTP/1.1'
    def log_message(self,*a): pass
    def do_POST(self):
        n=int(self.headers.get('Content-Length',0)); self.rfile.read(n)
        out=json.dumps({"data":[{"embedding":VEC}]}).encode()
        self.send_response(200); self.send_header('Content-Type','application/json')
        self.send_header('Content-Length',str(len(out))); self.end_headers(); self.wfile.write(out)
srv=ThreadingHTTPServer(('127.0.0.1',8092),H)
threading.Thread(target=srv.serve_forever,daemon=True).start(); time.sleep(0.4)
body=json.dumps({"input":"consulta"}).encode()
def novo():   # o que minreq faz hoje: conexao nova, fecha no fim
    t=time.perf_counter()
    c=http.client.HTTPConnection("127.0.0.1",8092,timeout=5)
    c.request("POST","/v1/embeddings",body,{"Content-Type":"application/json"})
    c.getresponse().read(); c.close()
    return (time.perf_counter()-t)*1000
conn=http.client.HTTPConnection("127.0.0.1",8092,timeout=5)
conn.connect(); conn.sock.setsockopt(__import__("socket").IPPROTO_TCP,__import__("socket").TCP_NODELAY,1)
def reuso():  # conexao mantida aberta
    t=time.perf_counter()
    conn.request("POST","/v1/embeddings",body,{"Content-Type":"application/json"})
    conn.getresponse().read()
    return (time.perf_counter()-t)*1000
for f,label in ((novo,'conexao NOVA por chamada (minreq hoje)'),(reuso,'conexao REUTILIZADA (keep-alive)')):
    for _ in range(30): f()
    s=[f() for _ in range(300)]
    print(f"{label:<40} {statistics.mean(s):6.3f} +- {statistics.stdev(s):5.3f} ms  (mediana {statistics.median(s):.3f})")
srv.shutdown()
