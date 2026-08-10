"""O ganho de 9,4x por configuracao de thread altera o VETOR? Se alterar, nao e ganho — e regressao."""
import json,os,subprocess,sys
TXT=["consulta sobre banco de dados vetorial","PostgreSQL wire compatible database","busca semantica em portugues"]
code='''
import json,sys
from fastembed import TextEmbedding
m=TextEmbedding(model_name="BAAI/bge-small-en-v1.5")
print(json.dumps([list(map(float,v)) for v in m.embed(json.loads(sys.argv[1]))]))
'''
def run(env):
    e={**os.environ,**env}
    out=subprocess.run([sys.executable,"-c",code,json.dumps(TXT)],capture_output=True,text=True,env=e)
    return json.loads(out.stdout)
a=run({"OMP_NUM_THREADS":"1","ORT_NUM_THREADS":"1"})
b=run({})  # sem limite
maxdiff=0.0; ident=0
for va,vb in zip(a,b):
    if va==vb: ident+=1
    maxdiff=max(maxdiff,max(abs(x-y) for x,y in zip(va,vb)))
print(f"vetores byte-identicos: {ident}/{len(a)}")
print(f"maior diferenca absoluta em qualquer dimensao: {maxdiff:.3e}")
import math
cos=lambda u,v: sum(x*y for x,y in zip(u,v))/(math.sqrt(sum(x*x for x in u))*math.sqrt(sum(y*y for y in v)))
print(f"similaridade de cosseno minima entre os pares: {min(cos(u,v) for u,v in zip(a,b)):.10f}")
