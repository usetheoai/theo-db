"""TCP loopback vs Unix domain socket, SEM modelo — so o canal, alternado e pareado."""
import http.client,json,os,socket,statistics,sys,tempfile,threading,time,random
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from socketserver import ThreadingUnixStreamServer
sys.path.insert(0,'benchmarks')
from theodb_bench.significance import paired_significance
random.seed(42); VEC=[random.random() for _ in range(384)]
def mk(nagle):
    class H(BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'; disable_nagle_algorithm=nagle
        def log_message(self,*a): pass
        def do_POST(self):
            self.rfile.read(int(self.headers.get('Content-Length',0)))
            o=json.dumps({"data":[{"embedding":VEC}]}).encode()
            self.send_response(200); self.send_header('Content-Length',str(len(o))); self.end_headers(); self.wfile.write(o)
    return H
class US(ThreadingUnixStreamServer):
    daemon_threads=True
    def get_request(self):
        r,_=super().get_request(); return r,("localhost",0)
class UC(http.client.HTTPConnection):
    def __init__(s,p): super().__init__("localhost"); s._p=p
    def connect(s):
        s.sock=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); s.sock.connect(s._p)
t=ThreadingHTTPServer(("127.0.0.1",8097),mk(True)); threading.Thread(target=t.serve_forever,daemon=True).start()
sp=os.path.join(tempfile.mkdtemp(),"s.sock"); u=US(sp,mk(False)); threading.Thread(target=u.serve_forever,daemon=True).start()
time.sleep(0.4)
ct=http.client.HTTPConnection("127.0.0.1",8097,timeout=10); ct.connect(); ct.sock.setsockopt(socket.IPPROTO_TCP,socket.TCP_NODELAY,1)
cu=UC(sp); cu.connect()
b=json.dumps({"input":"x"}).encode(); h={"Content-Type":"application/json"}
def rt(c):
    x=time.perf_counter(); c.request("POST","/v1/embeddings",b,h); c.getresponse().read(); return (time.perf_counter()-x)*1000
for _ in range(50): rt(ct); rt(cu)
A=[];B=[]
for _ in range(400):          # ALTERNADO
    A.append(rt(ct)); B.append(rt(cu))
s=paired_significance(A,B)
print(f"TCP loopback : {statistics.mean(A):.3f} +- {statistics.stdev(A):.3f} ms  (mediana {statistics.median(A):.3f})")
print(f"Unix socket  : {statistics.mean(B):.3f} +- {statistics.stdev(B):.3f} ms  (mediana {statistics.median(B):.3f})")
print(f"delta pareado: {s['mean_diff']:.3f} ms  ci95=[{s['ci95_low']:.3f},{s['ci95_high']:.3f}]  p={s['p_permutation']:.4f}")
t.shutdown(); u.shutdown()
