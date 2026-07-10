---
slug: m71-scan-latency
milestone: M71
date: 2026-07-10
verdict: SHIPPABLE
cycle: discover
---

# Blueprint M71 — Latência-superior do AM: fechar o gap iso-recall vs pgvector (scan hot-path v2)

Discover do gap de LATÊNCIA do `theodb_hnsw` a **iso-recall** (mesmo ponto de recall) vs pgvector, 500k×768d.
Dual-source (theodb file:line ↔ pgvector file:line) + SOTA web (R0). Grounded na medição do M60 (mesmo corpus/droplet).

## O problema medido (rigor iso-recall — não QPS-sweep cru)

A comparação honesta é **iso-recall** (council-benchmark): latência no MESMO ponto de recall, não QPS no mesmo `ef`.

| recall-alvo ~0.97 | ef necessário | p50 |
|---|---|---|
| pgvector | ef≈150–200 | ~6–7 ms |
| theodb f32 (baseline) | ef=1000 | ~15.6 ms |
| theodb f32 (+ multi-entry build, M60 fix#2) | ef=1000 | ~12.1 ms (+29% QPS) |

**O gap real: theodb precisa de ~5× o `ef` para o mesmo recall** → ~5× candidatos expandidos → ~2× a latência a
iso-recall. Não é penalidade de storage (page-reads são iguais — pgvector também separa página de vetor/vizinhos,
`hnsw.h:180`). Decompõe em qualidade-de-grafo (quanto `ef` por recall) + custo-por-candidato.

## Causa-raiz (ranqueada — file:line dos dois lados + SOTA)

### #1 (MAIOR alavanca) — Qualidade do grafo: 5× de `ef` por recall

theodb satura recall devagar em `ef` (0.898→0.954→0.958→0.974 em ef 200→1000); pgvector satura rápido
(0.878→0.964 em ef 40→100). Prova de que é o GRAFO (não o loop de scan): o M60 fix#2 (multi-entry `ep←W` no
build) cortou p50 15.6→12.1ms **sem mudar `ef`** → uma mudança de BUILD melhorou candidatos-por-recall. O scan em
si é idêntico ao pgvector (`scan_core.rs:138-164` ≡ `hnswutils.c:886-974`, mesmo beam greedy). **Alavanca de
BUILD:** auditar a heurística de seleção de vizinhos (diversity/RobustPrune, HNSW paper Alg.4) + `ef_construction`;
o multi-entry já é a direção certa (aplicá-lo é o 1º passo do M71 — é win de latência a recall igual).

### #2 (theodb pode SUPERAR pgvector) — Kernel de distância com early-out por limiar

theodb computa a distância 768d COMPLETA para TODO candidato (`scan_core.rs:149` `load()` incondicional; o bound
`nd < worst` só é checado DEPOIS, `:151-152`). pgvector pula a materialização do elemento quando não melhora o heap
(`hnswutils.c:928` passa `&f->distance` como `maxDistance`; `:562` só carrega se `*distance < *maxDistance`). Num
beam de ef=1000 (~16k candidatos/query) a maioria NÃO melhora o worst-of-ef — theodb paga a distância cheia neles.
**Fix (SOTA-grounded):** kernel `l2_sq_from_bytes_bounded(query, raw, threshold)` que acumula e **retorna cedo** ao
ultrapassar `worst` — em 768d, um candidato distante estoura o limiar nos primeiros ~128 dims e pula os 640
restantes. Passar `worst` de `scan_core.rs:151` para o `score`/`load`. **PANORAMA (arXiv:2510.00566)** valida
exatamente isso (acumula L2 incremental, poda por lower-bound vs threshold corrente, SIMD + cache-aware); Faiss
learned-termination (SIGMOD 2020) idem. É onde o theodb pode **bater** o pgvector (mais agressivo que o skip de
element-load do pgvector, e aplicável mesmo lendo a página).

### #3 (multiplicador — corta ns/candidato) — SIMD sub-ótimo em 768d

- **L2 de acumulador único** (`vec.rs:144-152`, 1 `acc` na cadeia fmadd) → latency-bound. 4 acumuladores
  independentes cortam ~2-3× (padrão Faiss FastScan / KScaNN arXiv:2511.03298). `cosine_terms` (`:178-186`) já tem
  3 acumuladores — parcialmente coberto.
- **Norma da query recomputada por candidato** no cosseno (`cosine_terms:183-185` faz `anq += q*q` para TODO
  candidato) — `‖q‖²` é CONSTANTE no scan. ~256 ops/candidato desperdiçadas × ~16k candidatos/query. **Fix:** içar
  `‖q‖²` para fora do loop (`hnsw_page.rs:1563`), kernel vira `(dot, nr)`. Cosseno é a métrica medida no M60 →
  direto no caminho quente. **Honesto:** #3 corta o p50 ABSOLUTO (ajuda em todo `ef`) mas NÃO muda o `ef`-por-recall
  → multiplica #1/#2, não substitui.

### #4 (refuta mis-framing) — page-reads são IGUAIS; p50 é sublinear em `ef`

Ambos separam página de vetor/vizinhos → ~mesmos page-reads. A maioria dos reads a `ef` alto acerta o buffer-cache
do PG → o candidato marginal é compute-bound. **Logo #2/#3 (compute/candidato) têm MAIS alavanca de p50 do que o
`ef` cru sugere.** (A única página extra do theodb é o rerank AQ v4 `hnsw_page.rs:1622-1631` — NÃO tomada no f32 v1
medido.)

### #5 (baixa confiança — medir antes) — overhead de heap/dedup a ef=1000

`BinaryHeap` + `HashSet<u64>` SipHash (`scan_core.rs:111-126`) vs `pairingheap` + `tidhash` do pgvector. ~16k
entradas no visited a ef=1000. Swap `ahash`/`FxHashSet` é experimento de 1 linha SE o profiler apontar. Honesto:
pequeno vs uma distância 768d.

## Coverage corners

### Corner 1 — Integration tests / medição iso-recall (o gate)
Harness iso-recall a 500k×768d (reuso `run_m60_recall.py` + controle `run_m60_pgvector_control.py`): reportar
**`ef` necessário para recall 0.97/0.98 e o p50 nesse ponto**, theodb vs pgvector, ≥3 runs mean±std. Gate M71:
**p50 do theodb ≤ pgvector a recall-matched**. Instrumentar `candidates_seen` (já existe `scan_core.rs:169`) +
"dims computadas vs totais" (novo counter, `THEODB_SCAN_PROFILE`) para provar o ganho do kernel bounded.

### Corner 2 — Dependencies
Nenhuma nova. Reuso de `vec.rs` (SIMD próprio), `scan_core.rs`, o harness de benchmark. Parsimony rung 4.

### Corner 3 — Tools
Micro-bench criterion `vec.rs:492` (estender p/ multi-accumulator + norm-hoist + bounded a dim=768, ns/candidato).
Benchmark e2e a 500k×768d em droplet (o gate iso-recall). pgvector como oráculo de controle (DB separado).

### Corner 4 — Techniques (SOTA / R0 web)
- **PANORAMA** (arXiv:2510.00566) — early-termination por bound incremental + threshold; SIMD/cache-aware. Valida #2.
- **Faiss FastScan** (wiki) + **KScaNN** (arXiv:2511.03298) — múltiplos acumuladores SIMD. Valida #3.
- **Faiss learned adaptive early termination** (SIGMOD 2020) — early-stop por candidatos. Contexto de #2.
- pgvector `HnswSearchLayer` `maxDistance` (`hnswutils.c:928,562`) — o skip de element-load (o que theodb supera com bounded kernel).

## ADRs do blueprint

**ADR M71-1 — A latência iso-recall é atacada por (a) qualidade de grafo [ef-por-recall] + (b) custo/candidato.**
Ordem: (1) aplicar o multi-entry build (win medido +29%, direção certa de grafo); (2) kernel de distância bounded
(theodb pode superar pgvector; PANORAMA); (3) SIMD multi-accumulator + norm-hoist (multiplicador). Alternativas
rejeitadas: mexer no beam de scan (idêntico ao pgvector — não é o gap); page-layout (page-reads já iguais — #4).

**ADR M71-2 — Iso-recall, não QPS-sweep.** Todo claim de latência é p50 a recall-matched (council-benchmark), ≥3
runs, mean±std, em droplet. Honest-negative aceito (se ficar em paridade, o veredito é paridade).

## Acceptance / halt
- Todo claim: theodb file:line + pgvector file:line + SOTA. Gate = p50 iso-recall ≤ pgvector a 500k×768d (droplet).
- Honest-negative aceito. Ceiling: paridade é alcançável (sem penalidade estrutural); superioridade é plausível via
  o bounded kernel + norm-hoist (edge de custo/candidato que o kernel C do pgvector não tem).

## Próximos passos do CYCLE M71
plan (to-plan) → implement (multi-entry build + bounded kernel + norm-hoist + multi-accumulator) → benchmark
iso-recall em droplet → review → release. **Implement + benchmark exigem droplet** (esta box não tem pgrx — provado
no M60).
