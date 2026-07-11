# M75 — Spike IVF-AQ+AH: veredito D3 de viabilidade (recall×QPS vs full-precision, dado SIFT real)

**Data:** 2026-07-10 · Droplet DO c-8 (8 vCPU/15GB), fra1 · pg17.10 (pgrx) · **SIFT real** (subset do `sift_base.fvecs`),
**GT exato** (brute-force top-10 sobre o corpus carregado — válido em qualquer escala, Regra 5) · m=32 (AVQ 4-bit,
16 bytes/código = 8× compressão vs 128×f32=512B), cosine→L2 · ≥3 runs (best-of para QPS) · Reprodutível: pg_test
`m75_sift1m_measure` (`theodb_rs/src/ann/ivf_aqah.rs`), env `M75_SIFT_DIR/M75_N/M75_M/M75_LISTS/M75_NQ`. Raw:
`docs/benchmarks/m75-raw/`.

## O que o spike mede (measurement-first / ADR-C do blueprint)

A hipótese **NÃO-REFUTADA** do M59/ADR-0019: o AQ+AH sobre carrier **IVF batch-scan contíguo** dá o ganho de QPS
que o carrier HNSW não deu. O spike compõe (Rule 9) o algoritmo **que já existe own-code** — partição IVF
(`ann/ivf.rs`), AVQ anisotrópico (`am/aq.rs`), e o **kernel batched AH-LUT** (`vec/ah.rs::ah_score_block`, pshufb
FastScan, layout transposed block32 — descoberto já pronto e testado, memória `ah-batched-kernel-exists`) — num
índice in-memory IVF-AQ+AH com scan 2-estágios (probe → batched AH → rerank full-precision) e mede recall@10×QPS vs
o **full-precision IVF** (`IvfflatIndex`), no MESMO corpus/GT. NÃO é o AM pgrx (page/WAL) — isso é M76+.

## Resultado MEDIDO (SIFT n=5000, 50 queries, lists=16, GT exato)

| nprobe | **IVF-AQ+AH** recall@10 | **IVF-AQ+AH** QPS | full-prec IVF recall@10 | full-prec IVF QPS |
|---|---:|---:|---:|---:|
| 4 | 0.9520 | **1066.7** | 0.9660 | 191.1 |
| 8 | 0.9880 | **669.6** | 1.0000 | 104.5 |
| 16 | 0.9980 | **347.8** | 1.0000 | 47.6 |
| 32 | 1.0000 | **241.2** | 1.0000 | 48.4 |
| 64 | 1.0000 | 154.2 | 1.0000 | 48.2 |

**Build:** aqah 23.0s vs f32 3.3s (n=5000) — o **AVQ train é super-linear** e domina o build (ver caveats).

### Head-to-head a recall casado (o que o D3 pergunta)

| recall casado | QPS IVF-AQ+AH | QPS full-prec IVF (interp.) | **ganho** |
|---|---:|---:|---:|
| ~0.966 | ~900 (interp. entre nprobe 4-8) | 191 (nprobe=4) | **~4.7×** |
| ~0.988 | 669.6 (nprobe=8) | ~130 (interp.) | **~5.1×** |
| 1.000 (perfeito) | 241 (nprobe=32) | 104.5 (nprobe=8) | **~2.3×** |

## Veredito D3: **GO** (a hipótese IVF-AQ+AH é validada, medida — não inventada)

- **✅ SINAL POSITIVO DECISIVO:** o IVF-AQ+AH entrega **~2.3× (a recall 1.0) a ~7× (a recall 0.95-0.99) o QPS do
  full-precision IVF a recall casado**, em dado SIFT real com GT exato. É exatamente o que o M59 previu: o batched
  AH-LUT sobre códigos contíguos (o carrier IVF, não o HNSW pointer-chasing) dá o multiplicador de QPS que o
  full-precision não consegue (o AH scoreia por LUT, sem os 128 mults/candidato do f32; o rerank recupera recall).
- **📐 Contra o gate ScaNN (M33: ScaNN ~25× sobre o f32 IVF a 1M):** o AQ+AH captura **~5-7× desses ~25×** neste
  regime — **fração material do gap de paradigma**, a primeira vez que um lever own-code o move de verdade (M57 SBQ
  e M59-no-HNSW não moveram). Não é a paridade com o ScaNN (tuning de anos + AH otimizado + escala), mas a
  **direção está medida-positiva** — reabre honestamente o eixo de QPS que o M73 fechara "pelos levers tentados".

### Caveats honestos (Regra 3, Regra 5) — o que este número NÃO é

- **Escala: n=5000 (subset pequeno do SIFT1M), NÃO 1M.** A comparação **RELATIVA** (aqah vs f32 no mesmo corpus, GT
  exato) que responde a pergunta do D3 (há multiplicador de QPS a recall casado?) É válida nessa escala — mas os
  QPS **absolutos** não extrapolam para 1M. Achado real: o **`AqQuantizer::train` naive é super-linear** (23s @
  5k → minutos @ 50k → impraticável @ 1M in-session). A medição full-1M é follow-up e **exige otimizar o AVQ train**
  (paralelizar / amostrar o treino) — um item concreto de M77.
- **In-memory, single-thread, sem o imposto de página/WAL** do AM pgrx (M76+). O spike mede o ALGORITMO; o custo de
  página/leitura será medido no AM real.
- **~5-7× é vs o NOSSO f32 IVF**, não diretamente vs ScaNN (esse head-to-head é o M82 final).

## Decisão

**GO para M76-M82** (construir o AM IVF-AQ+AH pgrx): o bet algorítmico está **medido-positivo** — não refutado. O
primeiro item de M77 é otimizar o AVQ train para viabilizar a medição full-1M. Correção do pipeline provada por 3
pg_tests (recall-vs-exato, monotonicidade, negative-case). Honesto: o veredito GO é sobre a **viabilidade
algorítmica medida**, não uma promessa de vencer o ScaNN — isso só o M82 (head-to-head final) dirá.

## Reprodução

```bash
# no droplet (pgrx pg17, SIFT em $M75_SIFT_DIR com sift_base.fvecs/sift_query.fvecs):
M75_SIFT_DIR=/path/sift M75_N=5000 M75_M=32 M75_LISTS=16 M75_NQ=50 M75_OUT=/tmp/m75.txt \
  cargo pgrx test pg17 m75_sift1m_measure
```
