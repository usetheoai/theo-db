"""Ha ganho em LOTE? Se embedar 8 textos numa chamada for muito mais barato que 8 chamadas,
entao dynamic batching e uma oportunidade real — e a tecnica padrao de servidor de inferencia."""
import json,statistics,sys,time
from fastembed import TextEmbedding
m=TextEmbedding(model_name="BAAI/bge-small-en-v1.5")
T=["consulta numero %d sobre banco de dados vetorial e busca semantica"%i for i in range(64)]
list(m.embed(T[:2]))
print(f"{'batch':>6} {'total ms':>10} {'ms/texto':>10} {'ganho/texto':>12} {'textos/s':>10}")
base=None
for b in (1,2,4,8,16,32,64):
    txt=T[:b]
    for _ in range(3): list(m.embed(txt))
    s=[]
    for _ in range(12):
        t=time.perf_counter(); list(m.embed(txt)); s.append((time.perf_counter()-t)*1000)
    tot=statistics.median(s); per=tot/b
    if base is None: base=per
    print(f"{b:>6} {tot:>10.2f} {per:>10.3f} {base/per:>11.2f}x {1000/per:>10.1f}")
