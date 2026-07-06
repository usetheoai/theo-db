# M50 — Régua SOTA vetorial: `theodb_hnsw` vs pgvector hnsw vs pgvectorscale `diskann`

**Date:** 2026-07-06 · **Milestone:** M50 (GATE do M51) · **Metric:** cosine (`<=>`, requer M49) · **GT:** seqscan exato
**Harness:** `benchmarks/run_m50_sota.py` (reusa `theodb_bench.metrics`, Rule 9) · **Image:** `theodb:m49-p3` (vector + vectorscale + theodb no mesmo processo)
**Verdict:** **theodb_hnsw em recall-parity com pgvector, ~1.6–1.7× atrás em latência (fator-constante) / 29% menos QPS a 8-clientes no alto-recall; `diskann` dominado nesta escala in-memory.** Detalhe + gate do M51 abaixo.

---

## ⚠️ Caveats de honestidade (leia primeiro — Rule 3)

Esta é uma **calibração de escala reduzida**, medida numa box de dev contendida. O que ela **é** e o que ela **não é**:

1. **Escala reduzida por decisão do usuário (2026-07-06).** O DoD do M50 pede um dataset RAG realista dimensionado pela memória da box (default cohere 768d×1M, ~3 GB, ×3 builds). A box tinha **12 containers ativos** e `collect_corpus` materializa o corpus inteiro em RAM sem teto (`am/build.rs`), tornando o run 1M×3-builds inviável aqui. O usuário escolheu **"rodar agora com dataset menor + caveats"**. Escala efetiva: **n=25 000, dim=128, dados gaussianos sintéticos** (distintos — NÃO a degeneração InitPlan da ADR 0012; cada vetor é `rnd.gauss` independente). O run 1M realista fica **gated no streaming build (M55+) ou numa box dedicada/quieta**.
2. **Box NÃO estava quieta.** `load_pre=7.87`; durante os 3 runs o load subiu para **11.03 / 9.32 / 12.64** (`load_post=12.66`) numa box de 12 cores — bem acima do "quieto". Consequência: **os números ABSOLUTOS de latência/QPS carregam ruído de contenção externa.** O que É robusto ao ruído é a **ordenação RELATIVA** entre os três índices — ela é consistente em **1-cliente E multi-cliente E nos 3 runs** (ver abaixo), então o veredito relativo se sustenta apesar da box suja.
3. **Recall é confiável; latência absoluta não.** O `recall_std` sobre 3 runs é **≤ 0.024** (e **≤ 0.007 no ponto de alto-recall ef=400** que ancora o veredito) → as curvas de recall são estáveis onde importa. A latência absoluta oscila com o load; por isso o veredito se apoia em **deltas relativos entre índices medidos no MESMO run/box** (ratios same-run), não em ms absolutos.
4. **Sub-item não medido:** a degradação de latência de scan com pending acumulada (theodb-específico, item 3 do DoD) **não foi medida** neste ciclo — fica registrada como follow-up honesto, não como checkbox fake.

---

## 1. Régua recall × latência (1 cliente) — n=25 000, dim=128, k=10, 50 queries, 3 runs

Melhor-recall em **negrito**. `recall` = média±desvio sobre 3 runs; `p50`/`qps` são 1-cliente (= 1/latência, **não** throughput de banco — ver §2).

Números **gerados diretamente do JSON committado** (`m50-sota-ruler.json § per_spec`) — nenhuma edição à mão (correção de review F1). `p50`/`qps` são médias sobre os 3 runs (ruído de latência absoluta — ver caveats).

| index | knob | recall@10 | p50 (ms) | qps (1c) | build (s) |
|---|---|---|---|---|---|
| **theodb_hnsw** | ef=40 | 0.363 ± 0.024 | 2.70 | 400 | 15.9 |
| **theodb_hnsw** | ef=100 | 0.623 ± 0.011 | 4.77 | 230 | 15.9 |
| **theodb_hnsw** | ef=200 | 0.813 ± 0.011 | 7.56 | 148 | 15.9 |
| **theodb_hnsw** | **ef=400** | **0.941 ± 0.007** | **12.19** | **100** | 15.9 |
| pgvector_hnsw | ef=40 | 0.367 ± 0.010 | 1.59 | 678 | 8.5 |
| pgvector_hnsw | ef=100 | 0.590 ± 0.003 | 2.84 | 385 | 8.5 |
| pgvector_hnsw | ef=200 | 0.796 ± 0.004 | 4.33 | 257 | 8.5 |
| pgvector_hnsw | **ef=400** | **0.935 ± 0.004** | **7.25** | **151** | 8.5 |
| diskann | sls=100 | 0.379 ± 0.005 | 4.86 | 240 | 69.2 |
| diskann | sls=500 | 0.753 ± 0.003 | 15.02 | 69 | 69.2 |
| diskann | sls=1000 | 0.875 ± 0.003 | 26.88 | 39 | 69.2 |
| diskann | **sls=2000** | **0.877 ± 0.003** | **42.80** | 25 | 69.2 |

**Leitura:**
- **theodb_hnsw ↔ pgvector: paridade de recall, gap de latência de fator-constante.** Nas duas curvas o recall casa knob-a-knob (0.363/0.623/0.813/0.941 vs 0.367/0.590/0.796/0.935 — paridade dentro do ruído). Na latência, pgvector é mais rápido **em TODA a curva** por um multiplicador **aproximadamente constante ~1.6–1.7×**: ef=40 → 2.70 vs 1.59 ms (1.70×); ef=400 → 12.19 vs 7.25 ms (1.68× média; **1.64× ± 0.35 medido same-run** nos 3 runs, imune ao ruído da box). Um gap que é um multiplicador ~constante ao longo de todos os ef é a **assinatura de um custo por-candidato fixo** (scoring f32 full-precision por candidato), não algorítmico e não recall — exatamente o eixo que o GOTO P0 do CTO aponta como o teto ainda não vencido, e exatamente o eixo que o lever do M51 (SBQ-inline) ataca.
- **diskann dominado nesta escala (por dois eixos DISTINTOS — ver §4).** Menor recall-teto (0.877), maior latência (28–43 ms) e **build 8× mais lento** (69 s vs 8.5 s). Isso é **esperado e honesto**: o `diskann` (StreamingDiskANN + SBQ) é desenhado para **billion-scale disk-resident**. **Eixo QPS:** a 25k o f32 cabe trivialmente em RAM, então a compressão SBQ ~16–32× **não tem onde ganhar QPS** (o ganho só materializa sob pressão de memória) e o traversal disk-oriented + search-lists profundas só custam latência. **Eixo recall (separado):** o teto 0.877 é **propriedade de carrier** (poda na distância SBQ sem rerank f32 completo no topo), **não** artefato da escala/pressão de memória — por `m40-ceiling-probe.md`, num pipeline com rerank f32 o recall é carrier-limited, não quantizer-limited. Detalhe completo em §4.

## 2. Primeiro QPS multi-cliente de banco (item 3 do DoD) — theodb vs pgvector, ponto de alto-recall (ef=400, recall ~0.94)

"QPS a 1 cliente é 1/latência, não throughput de banco" (DoD). Sweep de conexões concorrentes, 5 s cada, cada cliente com sua conexão:

| index | conns | QPS (banco) | p95 (ms) |
|---|---|---|---|
| theodb_hnsw | 8 | 680 | 16.6 |
| theodb_hnsw | 16 | 791 | 34.1 |
| pgvector_hnsw | 8 | **962** | 11.9 |
| pgvector_hnsw | 16 | 921 | 33.1 |

**Leitura:** a 8 clientes, pgvector faz **962 vs 680 qps** (theodb **29% atrás**, no MESMO recall ~0.94) — consistente com o gap de latência 1-cliente. A 16 clientes ambos saturam (~790–920 qps) na box de 12 cores sob load externo 11–12, e o gap **estreita para ~14%** (791 vs 921). **O sinal relativo (pgvector à frente no throughput de alto-recall) é o mesmo em 1-cliente e multi-cliente e nos 3 runs** → a *direção* é robusta ao ruído da box (a *magnitude* varia 14–29% com a saturação).

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

**Onde está o teto da classe atual.** O `theodb_hnsw` **igualou o recall** do pgvector (a baseline SOTA permissiva) em toda a curva cosine — o pilar de recall do North Star está em paridade. O que **sobra** é o eixo de **latência/throughput**: pgvector é ~1.6–1.7× mais rápido (1-cliente, fator-constante ao longo de toda a curva; **1.64× ± 0.35 same-run**) e faz 29% mais QPS a 8-clientes (estreitando para ~14% a 16c) no MESMO recall ~0.94. O gargalo é **custo por-candidato no scan** (multiplicador ~constante → scoring f32 full-precision por candidato), não algorítmico e não recall — o mesmo diagnóstico do GOTO P0.

**O lever do M51 (SBQ inline) continua sendo a aposta certa?** — **SIM, direcionalmente, MAS com o gate re-escopado.**

- **Direção correta:** o gap remanescente é o custo por-candidato do scan, e o mecanismo do SBQ-inline — códigos compactos DENTRO do índice + scoring barato (Hamming/popcount) no hot path + rerank f32 só no top — ataca **exatamente** esse eixo. É o lever que muda o asymptote (menos bytes lidos por candidato, mais grafo em cache), não mais um ajuste de fator-constante.
- **Ressalva dura, medida aqui — separando os DOIS eixos (correção de review):** o `diskann` — que **É** SBQ-inline (a implementação SOTA permissiva do pgvectorscale) — ficou **dominado nesta escala de 25k** por **duas razões distintas** que não devem ser fundidas:
  - **Eixo QPS/throughput:** o ganho de QPS da compressão SBQ **só materializa sob pressão de memória** (quando o corpus f32 estoura o cache/RAM e a compressão ~16–32× faz o grafo caber). A 25k, f32 cabe trivialmente (~12,8 MB) → a compressão não tem onde ganhar e o overhead de traversal disk-oriented + search-lists profundas só custa latência.
  - **Eixo recall (separado):** o teto de recall do `diskann` (0.877) **não é artefato de pressão de memória** — é uma propriedade de **carrier**: o StreamingDiskANN poda na distância SBQ sem um rerank f32 completo no topo. Por `m40-ceiling-probe.md`, num pipeline com rerank f32 o recall é **carrier-limited, não quantizer-limited** (com budget de carrier adequado, SBQ+rerank chega a ~1.0).
  - **Corolário para o M51:** a meta de QPS do M51 (`fronteira ≥2× a recall ≥0.99 vs pgvector`) **NÃO é mensurável nesta escala reduzida** — medida a 25k, o M51 vai *parecer* uma regressão **no eixo de throughput** do mesmo jeito que o `diskann` parece aqui. **Mas a analogia vale SÓ no eixo de throughput:** no eixo de recall o M51 é desenhado com rerank f32 on-page (DoD M51), então **um recall < 0.99 a 25k seria uma falha REAL do M51, não um artefato esperado**. O número do gate de QPS do M51 **tem que vir de um run em escala com pressão de memória** (cohere 768d×1M ou subset 1536d), não de 25k.
- **Dependência de recall corretamente identificada (correção de review — base M40, não M39):** o alvo `recall ≥0.99` do M51 **depende do passo de rerank exato f32 no top** — **não** porque "SBQ escalar topa o recall" (o `m40-ceiling-probe.md` **falsificou** isso: o teto 0.77 do M39 era um artefato de carrier/probes baixos, não do quantizador; a posição fechada do time — objetivo do M51, ROADMAP — é que **quantização não move recall neste pipeline, ela existe para baratear o scoring**), mas porque o **rank Hamming sobre os códigos comprimidos precisa ser corrigido pelo rerank f32** para que o recall fique **carrier-limited (recuperável via ef/over_fetch)** em vez de quantizer-limited. Ref: `m40-ceiling-probe.md` + `theodb_rs/src/sbq.rs:6-7` (o pipeline carrier→Hamming→rerank-f32 já existe).

**Decisão de gate:** **M51 AUTORIZADO** (a aposta SBQ-inline é a próxima certa), com **três condições de medição herdadas deste veredito**, para não repetir o erro de medir num asymptote errado:
1. O gate de QPS do M51 (`≥2× a recall ≥0.99`) **deve ser medido em escala com pressão de memória** (≥250k @1536d ou 1M @768d) — a régua de 25k aqui **não pode** validar nem refutar esse número.
2. O gate de recall do M51 (`≥0.99`) **exige o rerank f32 no top** provado por teste — porque o rank Hamming sozinho é carrier-degradado (M40), não porque o SBQ tenha um teto de recall.
3. O número de QPS do gate **deve ser medido em box quieta (load-guard pré-flight)** — o QPS absoluto DESTE run está poluído por contenção externa (caveat §2) e não pode servir de baseline para o `≥2×`.

Nenhuma evidência aqui **falsifica** o M51; a evidência **re-escopa a régua** onde ele deve ser medido. Isso é o gate cumprindo seu papel: impedir que o próximo ciclo mire um asymptote desconhecido — no eixo de QPS **e** no eixo de recall.

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
