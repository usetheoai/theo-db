# M50 — Régua SOTA vetorial: `theodb_hnsw` vs pgvector hnsw vs pgvectorscale `diskann`

**Date:** 2026-07-06 · **Milestone:** M50 (GATE do M51) · **Metric:** cosine (`<=>`, requer M49) · **GT:** seqscan exato
**Harness:** `benchmarks/run_m50_sota.py` (reusa `theodb_bench.metrics`, Rule 9) · **Image:** `theodb:m49-p3` (vector + vectorscale + theodb no mesmo processo)
**Verdict:** **theodb_hnsw em recall-parity com pgvector, ~40% atrás em throughput no ponto de alto-recall; `diskann` dominado nesta escala in-memory.** Detalhe + gate do M51 abaixo.

---

## ⚠️ Caveats de honestidade (leia primeiro — Rule 3)

Esta é uma **calibração de escala reduzida**, medida numa box de dev contendida. O que ela **é** e o que ela **não é**:

1. **Escala reduzida por decisão do usuário (2026-07-06).** O DoD do M50 pede um dataset RAG realista dimensionado pela memória da box (default cohere 768d×1M, ~3 GB, ×3 builds). A box tinha **12 containers ativos** e `collect_corpus` materializa o corpus inteiro em RAM sem teto (`am/build.rs`), tornando o run 1M×3-builds inviável aqui. O usuário escolheu **"rodar agora com dataset menor + caveats"**. Escala efetiva: **n=25 000, dim=128, dados gaussianos sintéticos** (distintos — NÃO a degeneração InitPlan da ADR 0012; cada vetor é `rnd.gauss` independente). O run 1M realista fica **gated no streaming build (M55+) ou numa box dedicada/quieta**.
2. **Box NÃO estava quieta.** `load_pre=7.87`; durante os 3 runs o load subiu para **11.03 / 9.32 / 12.64** (`load_post=12.66`) numa box de 12 cores — bem acima do "quieto". Consequência: **os números ABSOLUTOS de latência/QPS carregam ruído de contenção externa.** O que É robusto ao ruído é a **ordenação RELATIVA** entre os três índices — ela é consistente em **1-cliente E multi-cliente E nos 3 runs** (ver abaixo), então o veredito relativo se sustenta apesar da box suja.
3. **Recall é confiável; latência absoluta não.** O `recall_std` sobre 3 runs é minúsculo (~0.006–0.011) → as curvas de recall são estáveis. A latência absoluta oscila com o load; por isso o veredito se apoia em **deltas relativos entre índices medidos no MESMO run/box**, não em ms absolutos.
4. **Sub-item não medido:** a degradação de latência de scan com pending acumulada (theodb-específico, item 3 do DoD) **não foi medida** neste ciclo — fica registrada como follow-up honesto, não como checkbox fake.

---

## 1. Régua recall × latência (1 cliente) — n=25 000, dim=128, k=10, 50 queries, 3 runs

Melhor-recall em **negrito**. `recall` = média±desvio sobre 3 runs; `p50`/`qps` são 1-cliente (= 1/latência, **não** throughput de banco — ver §2).

| index | knob | recall@10 | p50 (ms) | qps (1c) | build (s) |
|---|---|---|---|---|---|
| **theodb_hnsw** | ef=40 | 0.367 ± 0.009 | 2.93 | 343 | 15.9 |
| **theodb_hnsw** | ef=100 | 0.598 ± 0.010 | 4.98 | 201 | 15.9 |
| **theodb_hnsw** | ef=200 | 0.808 ± 0.006 | 8.02 | 125 | 15.9 |
| **theodb_hnsw** | **ef=400** | **0.941 ± 0.006** | **12.19** | **72** | 15.9 |
| pgvector_hnsw | ef=40 | 0.369 ± 0.009 | 2.88 | 373 | 8.5 |
| pgvector_hnsw | ef=100 | 0.605 ± 0.007 | 4.51 | 225 | 8.5 |
| pgvector_hnsw | ef=200 | 0.796 ± 0.002 | 6.91 | 149 | 8.5 |
| pgvector_hnsw | **ef=400** | **0.935 ± 0.011** | **7.25** | **107** | 8.5 |
| diskann | sls=100 | 0.383 ± 0.001 | 7.70 | 130 | 69.2 |
| diskann | sls=500 | 0.755 ± 0.009 | 19.96 | 50 | 69.2 |
| diskann | sls=1000 | 0.874 ± 0.002 | 28.14 | 36 | 69.2 |
| diskann | **sls=2000** | **0.877 ± 0.002** | **42.80** | 21 | 69.2 |

**Leitura:**
- **theodb_hnsw ↔ pgvector: paridade de recall, gap de latência.** Nas duas curvas o recall casa knob-a-knob (0.37/0.60/0.81/0.94 vs 0.37/0.61/0.80/0.94). No ponto de alto-recall (~0.94), theodb faz **p50 12.2 ms vs 7.25 ms** do pgvector → **theodb ~1.7× mais lento** ali. Nos knobs baixos (ef≤200) a diferença é pequena. Ou seja: o gap que sobra é **latência/fator-constante no scan de alto-ef**, não recall — exatamente o que o GOTO P0 do CTO aponta como o teto ainda não vencido.
- **diskann dominado nesta escala.** Menor recall-teto (0.877), maior latência (28–43 ms) e **build 8× mais lento** (69 s vs 8.5 s). Isso é **esperado e honesto**: o `diskann` (StreamingDiskANN + SBQ) é desenhado para **billion-scale disk-resident**, onde a compressão SBQ ~16–32× faz o grafo caber em RAM. A 25k tudo cabe em memória, então o SBQ só **custa recall** e o overhead de traversal disk-oriented só **custa latência** — sem o regime de pressão de memória, o design dele não tem onde ganhar.

## 2. Primeiro QPS multi-cliente de banco (item 3 do DoD) — theodb vs pgvector, ponto de alto-recall (ef=400, recall ~0.94)

"QPS a 1 cliente é 1/latência, não throughput de banco" (DoD). Sweep de conexões concorrentes, 5 s cada, cada cliente com sua conexão:

| index | conns | QPS (banco) | p95 (ms) |
|---|---|---|---|
| theodb_hnsw | 8 | 680 | 16.6 |
| theodb_hnsw | 16 | 791 | 34.1 |
| pgvector_hnsw | 8 | **962** | 11.9 |
| pgvector_hnsw | 16 | 921 | 33.1 |

**Leitura:** a 8 clientes, pgvector faz **962 vs 680 qps** (theodb ~40% atrás, no MESMO recall ~0.94) — consistente com o gap de latência 1-cliente. A 16 clientes ambos saturam (~800–920 qps) na box de 12 cores sob load externo 11–12, e o gap se estreita. **O sinal relativo (pgvector à frente no throughput de alto-recall) é o mesmo em 1-cliente e multi-cliente** → robusto ao ruído da box.

## 3. Metodologia / reprodução

```bash
# box quieta idealmente (aqui rodou contendida — ver caveats)
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres \
  PGOPTIONS='-c statement_timeout=300000' \
  python3 benchmarks/run_m50_sota.py --n 25000 --dim 128 --nq 50 --runs 3 --out m50.json
```
Imagem `theodb:m49-p3` (as 3 extensões no mesmo processo/buffer manager). GT = seqscan exato por query (cosine). Isolamento: cada índice é `CREATE`/`DROP` isolado por spec. Load registrado por run (`os.getloadavg`). Raw completo em `m50-sota-ruler.json § raw` + `§ multiclient`.

---

## 4. VEREDITO ESCRITO — **o gate formal do M51** (item 6 do DoD)

**Onde está o teto da classe atual.** O `theodb_hnsw` **igualou o recall** do pgvector (a baseline SOTA permissiva) em toda a curva cosine — o pilar de recall do North Star está em paridade. O que **sobra** é o eixo de **latência/throughput no ponto de alto-recall**: ~1.7× mais lento (1-cliente) e ~40% menos QPS (8-cliente) que o pgvector a recall ~0.94. O gargalo é **custo por-candidato no scan de alto-ef** (fator-constante), não algorítmico e não recall — o mesmo diagnóstico do GOTO P0.

**O lever do M51 (SBQ inline) continua sendo a aposta certa?** — **SIM, direcionalmente, MAS com o gate re-escopado.**

- **Direção correta:** o gap remanescente é latência de scan, e o mecanismo do SBQ-inline — códigos compactos DENTRO do índice + scoring barato (Hamming/popcount) no hot path + rerank f32 só no top — ataca **exatamente** esse eixo. É o lever que muda o asymptote (menos bytes lidos por candidato, mais grafo em cache), não mais um ajuste de fator-constante.
- **Ressalva dura, medida aqui:** o `diskann` — que **É** SBQ-inline (a implementação SOTA permissiva do pgvectorscale) — ficou **dominado nesta escala de 25k**. A razão é estrutural: o ganho de QPS do SBQ **só materializa quando o corpus f32 estoura o cache/RAM** (regime de pressão de memória). A 25k, f32 cabe, então SBQ só custa recall. **Corolário para o M51:** a meta de sucesso do M51 (`fronteira ≥2× a recall ≥0.99 vs pgvector`) **NÃO é mensurável nesta escala reduzida** — medida a 25k, o M51 vai *parecer* uma regressão do mesmo jeito que o `diskann` parece aqui. O número do gate do M51 **tem que vir de um run em escala com pressão de memória** (cohere 768d×1M ou subset 1536d), não de 25k.
- **Risco de recall já mapeado (M39):** a quantização escalar SBQ topa em recall 0.77–0.95 em SIFT real (ADR/M39 linha 367) — o alvo `recall ≥0.99` do M51 **depende do passo de rerank exato f32 no top** (que o DoD do M51 já inclui). Sem o rerank, o gate de recall do M51 falha.

**Decisão de gate:** **M51 AUTORIZADO** (a aposta SBQ-inline é a próxima certa), com **duas condições de medição herdadas deste veredito**, para não repetir o erro de medir num asymptote errado:
1. O gate de sucesso do M51 (`≥2× QPS a recall ≥0.99`) **deve ser medido em escala com pressão de memória** (≥250k @1536d ou 1M @768d) — a régua de 25k aqui **não pode** validar nem refutar esse número.
2. O gate de recall do M51 (`≥0.99`) **exige o rerank f32 no top** provado por teste, dado o teto conhecido do SBQ escalar (M39).

Nenhuma evidência aqui **falsifica** o M51; a evidência **re-escopa a régua** onde ele deve ser medido. Isso é o gate cumprindo seu papel: impedir que o próximo ciclo mire um asymptote desconhecido.

## 5. DoD do M50 — status honesto

| # | Item do DoD | Status |
|---|---|---|
| 1 | Pareto + spec `diskann`, seed 42, isolamento → posição SOTA-em-Postgres | ✅ feito (§1) |
| 2 | +1 dataset realista dimensionado pela memória (cohere 768d×1M ou subset) | ⚠️ **escala reduzida** (25k×128 gaussiano) por decisão do usuário; 1M realista gated no streaming build (M55+)/box dedicada — caveat explícito §caveats |
| 3 | 1º QPS-de-banco multi-cliente (8/16 conns, p50/p95) | ✅ feito (§2); ⚠️ sub-item de degradação-com-pending **não medido** (follow-up honesto) |
| 4 | Protocolo de box quieta (load-guard, ≥3 runs, effect>variância) | ⚠️ **3 runs ✅ + load registrado ✅**, mas box NÃO quieta (load 7.9→12.6); effect>variância vale p/ recall (std ~0.01) e p/ o delta relativo, NÃO p/ latência absoluta — caveat §caveats |
| 5 | Higiene de artefatos G8 (JSON m41/m43, banner m32, superseded M31, reconcile M30) | ✅ feito (4/4 — ver CHANGELOG `[Unreleased]`) |
| 6 | Veredito escrito = gate formal do M51 | ✅ feito (§4) |

**Bottom line honesto:** os itens 1, 3 (headline), 5, 6 estão **cumpridos com evidência**; os itens 2 e 4 estão **cumpridos em escala reduzida** por restrição de infra que o usuário aceitou explicitamente (box contendida) — documentados como caveat, não como checkbox fake. O veredito relativo (paridade de recall, gap de latência, diskann dominado a esta escala, M51 autorizado-mas-re-escopado) é **robusto ao ruído** porque consistente em 1-cliente + multi-cliente + 3 runs.
