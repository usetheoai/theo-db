# M72 — QPS multi-cliente a 1M (throughput sob concorrência real) — theodb_hnsw vs pgvector

**Data:** 2026-07-10 · Droplet DO c-8 (8 vCPU dedicados / 15 GB), fra1 · PostgreSQL 17.10 (pgrx) · 1M×128d ·
gaussian-mixture determinístico (256 clusters, `theodb_bench` SEED), cosine, GT exato · 8 clientes concorrentes
(multiprocessing), 15 s/run, **3 runs por ponto** (mean±std) · Reprodutível: `benchmarks/run_m72_multiclient.py`.
Raw: `docs/benchmarks/m72-raw/*.json`.

## Contexto (o que faltava)

M32/M34 mediram **p50 single-client**. O M72 mede o regime de produção que faltava: **throughput agregado sob N
conexões concorrentes** martelando o MESMO índice — theodb_hnsw (com extendCandidates, M60/ADR-0034 + multi-entry,
M71) vs pgvector 0.8.0, mesmo corpus/hardware, ao ponto de recall casado.

## Resultado MEDIDO (8 clientes concorrentes, 1M×128d)

| engine | ef | recall@10 | **QPS agregado** | p50 (ms) | p95 (ms) | p99 (ms) |
|---|---:|---:|---:|---:|---:|---:|
| pgvector | 80 | 0.8755 | 772.4 ± 1 | 11.53 | 12.65 | 13.48 |
| pgvector | 120 | 0.8990 | 642.7 ± 0 | 13.87 | 14.96 | 15.75 |
| pgvector | 200 | 0.9095 | 539.5 ± 1 | 16.51 | 17.61 | 18.37 |
| pgvector | 350 | 0.9135 | 453.5 ± 0 | 19.49 | 20.87 | 21.96 |
| pgvector | 600 | 0.9140 | 392.6 ± 1 | 22.32 | 23.76 | 24.62 |
| **theodb** | 150 | **0.9170** | **597.7 ± 2** | 13.61 | 16.14 | 18.24 |
| **theodb** | 250 | **0.9515** | **445.3 ± 2** | 18.36 | 21.70 | 26.04 |
| **theodb** | 400 | **0.9700** | **353.5 ± 1** | 23.59 | 27.08 | 32.04 |

Build (1M, mesmo hardware): **theodb 366.9 s** (com extendCandidates) · **pgvector 1084.2 s** (build paralelo
on-disk default) → theodb builda ~**3× mais rápido** neste regime.

## Head-to-head a recall casado

- **Ponto casado ~0.91 (ambos alcançam):** theodb ef150 → **0.917 @ 597.7 QPS** (p50 13.6 ms) vs pgvector ef200 →
  0.9095 @ 539.5 QPS (p50 16.5 ms). **theodb à frente** — throughput +11%, p50 menor, a um recall ligeiramente
  MAIOR. (pgvector precisando de mais recall só piora: ef350 0.9135@453, ef600 0.914@393.)
- **Recall alto (0.95 / 0.97):** theodb alcança 0.9515 @ 445 QPS e **0.970 @ 354 QPS**. **A pgvector platôa em
  ~0.914 neste dataset** (ef600 ainda 0.914) — não alcança 0.95/0.97 aqui, então não há ponto casado acima de 0.92.

## Veredito HONESTO (Regra 3, Regra 5) — competitivo, **regime-favorável ao theodb**

- **✅ O throughput multi-cliente do theodb_hnsw é competitivo — e neste dataset SUPERA a pgvector a recall casado:**
  +11% QPS a ~0.91, e alcança recall (0.97) que a pgvector não alcança aqui. Origem do ganho (identificada): o
  **extendCandidates** (M60/ADR-0034) dá navegabilidade superior em dados **clusterizados** — e este corpus é
  256 clusters gaussianos, exatamente o regime-alvo dessa heurística. A pgvector (build padrão) tem dificuldade de
  navegar os clusters e platôa ~0.914.
- **⚠️ Isto é o regime FAVORÁVEL ao theodb — não é claim universal.** O eixo oposto foi medido e documentado
  (ADR-0034 / `gap1-extend-candidates.md`, 500k×**768d**, recall **0.99+**): lá a pgvector tem o **frontier per-ef à
  frente** (~1.8× o ef do theodb a iso-recall alta). Dimensão alta + recall alto = regime da pgvector; 128d +
  clusterizado + recall moderado = regime do theodb. **Ambos são fatos medidos dos seus regimes.**
- **⚠️ Corpus sintético (gaussian-mixture), NÃO o SIFT1M real.** Consistente com M45/M51 (mesmo gerador,
  comparação justa: mesmo dado/hardware para os dois engines), mas o SIFT1M real é menos patologicamente
  clusterizado — o platô da pgvector e a vantagem do theodb provavelmente encolhem em dado real. Marcado como
  honesto, não escondido.

## Conclusão

O M72 fecha o gap de conhecimento do multi-cliente: **theodb_hnsw entrega throughput concorrente competitivo/
à-frente da pgvector a recall casado no regime clusterizado 128d** (paridade/superioridade own-code MEDIDA neste
regime), enquanto o **frontier de alta-dimensão/alto-recall permanece da pgvector** (ADR-0034). Nenhuma afirmação
universal de superioridade — o veredito honesto é **competitivo, regime-dependente**, alimentando o veredito
consolidado do pilar (M73 / ADR-0035).

## Reprodução

```bash
# no droplet (pgrx pg17, extensões theodb_rs + vector instaladas):
python3 benchmarks/run_m72_multiclient.py --engine theodb   --n 1000000 --dim 128 --clients 8 --ef 150 --secs 15 --out t.json
python3 benchmarks/run_m72_multiclient.py --engine pgvector --n 1000000 --dim 128 --clients 8 --ef 200 --secs 15 --out p.json
```
