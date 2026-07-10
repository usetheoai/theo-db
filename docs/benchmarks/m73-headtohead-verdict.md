# M73 — Head-to-head MEDIDO vs ScaNN/AlloyDB: o veredito de superioridade vetorial

**Data:** 2026-07-10 · Consolidação measurement-first de frontiers MEDIDOS em SIFT1M real (sift-128-euclidean,
n=1M, k=10) · Veredito rastreável do North Star (`docs/adr/0002`) → ADR-0035.

## Método (por que consolidar, não re-rodar ScaNN)

O M73 pede o veredito head-to-head **depois** de M60+M71+M72. As melhorias de M60 (extendCandidates/recall),
M71 (multi-entry/QPS) e M72 (multi-cliente) foram todas no **carrier HNSW full-precision** — nenhuma tocou o
**paradigma de quantização** que é a vantagem do ScaNN (Gap 2). Portanto o veredito é emitido dos frontiers **já
medidos em SIFT1M real**, sem re-rodar o ScaNN (anti-sunk-cost / D3 — o ScaNN não mudou; re-medir reconfirmaria um
gap de paradigma que quatro medições independentes já estabelecem). Cada linha abaixo tem artefato.

## Frontier medido @ recall ≥ 0.99 (SIFT1M real, o ponto operacional que importa)

| system | fonte (artefato) | recall@10 | QPS | p50 (ms) |
|---|---|---:|---:|---:|
| **ScaNN** (algoritmo do AlloyDB) | M33 `m33-scann-headtohead.md` | 0.9969 | **1920.3** | 0.49 |
| theodb_ivfflat (full-precision) | M33 | 0.9924 | 77.9 | 12.8 |
| theodb_hnsw (full-precision, best) | M45 `m45-pareto-sift1m.md` | 0.9932 | ~43.5 | ~23 |
| pgvector_hnsw | M45 | 0.9956 | 62.8 | — |
| pgvector_ivfflat | M33 | 0.9923 | 71.8 | 13.5 |

**Gap ScaNN @ 0.99: ~25× (vs ivfflat) a ~44× (vs hnsw) sobre QUALQUER índice full-precision permissivo** —
theodb_ivfflat, theodb_hnsw E pgvector estão TODOS na mesma ordem de grandeza atrás. **O gap não é theodb-específico
— é de paradigma** (quantização anisotrópica aprendida + Asymmetric-Hashing LUT SIMD do ScaNN vs full-precision).

## O que M60/M71/M72 mudaram (e o que NÃO mudaram)

- **Mudaram (medido):** recall-navegabilidade a escala (M60: 0.974→0.990 @ 500k), QPS single-client (M71: +29%),
  e o throughput **multi-cliente a 1M×128d** — onde o theodb_hnsw é **competitivo a superior** vs pgvector a recall
  casado no regime clusterizado (M72: +11% QPS @ ~0.91, build 3× mais rápido). **Paridade/superioridade own-code vs
  pgvector: alcançada nesse regime.**
- **NÃO mudaram:** o eixo de quantização (Gap 2). O melhor quantizador permissivo do SOTA (**RaBitQ**, spike D3 1M
  medido) é **competitivo com full-precision (8.2 ms @ 98.4%), não 25× mais rápido** — o ganho dele é memória, não
  QPS (M74/ADR-0036). Logo o gap de QPS do ScaNN **permanece** para qualquer extensão Postgres permissiva.

## Veredito (o que a régua sustenta — Regra 3, Regra 5)

1. **Paridade own-code classe-pgvector: ALCANÇADA E MEDIDA.** Tipo vetorial próprio (M69/M70) + HNSW próprio com
   recall classe-pgvector (M60) + throughput multi-cliente competitivo-a-superior no regime 128d (M72). O TheoDB
   entrega busca vetorial own-code de qualidade equivalente à extensão de referência permissiva.
2. **Superioridade de QPS vetorial sobre o AlloyDB/ScaNN: MEDIDA COMO NÃO-ALCANÇÁVEL** por extensão Postgres
   permissiva (honest-negative). O gap ~25-44× @ recall 0.99 é do algoritmo do ScaNN (AH-LUT anisotrópico, 128d,
   tuning de anos do Google) + o fato de o ScaNN ser uma library in-memory que **não paga o imposto MVCC/WAL/heap**
   que qualquer extensão transacional paga. Confirmado por 4 medições independentes (M33 ivfflat, M45 hnsw, M72
   multi-cliente, RaBitQ spike).
3. **Caveat estrutural (honesto):** ScaNN é uma library ANN in-memory (sem persistência/transações/SQL); o TheoDB
   é um índice PostgreSQL persistente transacional. O eixo QPS-raw compara o ALGORITMO; não torna o ScaNN um banco.
   O valor do TheoDB é busca vetorial **dentro** de um banco transacional aberto, model-agnostic, portável.

## Conclusão → posicionamento permitido (`public-copy.md`)

✅ "paridade de recall classe-pgvector com índice vetorial own-code" · ✅ "throughput multi-cliente competitivo no
regime 128d" · ✅ "eficiência de memória RaBitQ para billion-scale" (M74) · ❌ **jamais** "mais rápido que o AlloyDB
no vetor" (medido como falso).

O reposicionamento formal do North Star (de "superar" para "paridade + memória + AI-native/HTAP/aberto") é a
proposta `docs/adr/0033` — decisão do owner. O M73 entrega a **prova medida de ONDE o TheoDB está** (o que o North
Star exige), não uma vitória inventada.

## Reprodução

```
# ScaNN frontier (inalterado desde M33): pip install scann; python3 benchmarks/run_m33_scann.py --runs 3
# theodb_hnsw frontier: benchmarks/theodb_bench (M45, ef_grid=[40,64,100,200,400], SIFT1M)
# multi-cliente: benchmarks/run_m72_multiclient.py (M72)
```
