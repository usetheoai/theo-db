# TheoDB — Roadmap v5 (Superioridade vetorial P0 — MEDIDA)

> **DRAFT para revisão** (2026-07-09). Sucessor estratégico dos roadmaps v2 (own-code Rust, M17–M60),
> v3 (amplitude de produto, M61–M68) e v4 (independência do pgvector, M69–M70 — **pgvector removido, tipo
> `vector` 100% own-code**). Origem: sign-off do owner (2026-07-09) para fechar o **pilar P0 do North Star**
> que segue parcial: **superioridade vetorial comprovada por benchmark**.
>
> Herda TODAS as travas: engine PostgreSQL mantido (wire-compat), **measurement-first (ADR 0002 — nada é
> "concluído" sem paridade/benchmark medido; nenhuma afirmação de performance sem artefato em
> `wiki/benchmarks/`)**, licenças permissivas (D1 — Apache/MIT/BSD/PostgreSQL), Regra 9 (não reinventar peça
> madura), **honestidade extrema (Regra 3/5 — honest-negative é um resultado válido, não um fracasso)**.

## Contexto (o que está MEDIDO — para não prometer o que a régua não sustenta)

O North Star (`wiki/decisions/0002`) é **igualar ou superar o AlloyDB**. AlloyDB é GCP-managed → o baseline
sancionado é o **ScaNN OSS** (o algoritmo do índice vetorial do AlloyDB, arXiv:1908.10396). Estado medido:

| Eixo | Estado (medido) | Artefato |
|---|---|---|
| **Recall-parity vs pgvector** | ✅ paridade a régua (SIFT1M) | `wiki/benchmarks/m45-pareto-sift1m.*` |
| **Recall≥0.99 no grafo próprio a escala** | ❌ satura ~0.96–0.974 a 100k–500k×768d (gap ~1.5–3pt) | M57 (`m57-raw/`) → **M60 aberto** |
| **Latência do AM (p50)** | ✅ paridade vs pgvector a 1M no ponto recall≥0.99 (M34 ivfflat) | `m32-scale-sift1m.*` |
| **Latência SUPERIOR** | ❌ paridade, não superioridade clara | — |
| **QPS a 1M** | ✅ benchmarked (multi-way) | `m32-scale-sift1m.*` |
| **Head-to-head vs ScaNN (AlloyDB)** | ⚠️ recall PARIDADE mas **~25–37× GAP de QPS** (ScaNN mais rápido) | `wiki/benchmarks/m33-scann-headtohead.*` |

**A verdade dura:** o gap vs ScaNN (~25×) vem da **quantização anisotrópica + Asymmetric Hashing (AH) SIMD**
do ScaNN. Duas apostas nesse eixo já foram MEDIDAS e deram **honest-negative** no carrier: **M57** (SBQ inline
NÃO é ≥2× QPS) e **M59** (anisotrópica + AH SIMD — carrier HNSW não vence). Portanto o v5 **não promete vencer
o ScaNN** — promete **fechar o recall (M60), medir a latência/QPS honestamente, e emitir um veredito de
superioridade rastreável** (que pode ser "paridade own-code portável + trade-off de QPS documentado", se a
régua assim disser). Caveat estrutural do M33: ScaNN é uma **library ANN in-memory** (sem persistência/SQL/txn);
o theodb é um **índice PostgreSQL persistente transacional** — o eixo ALGORITMO é comparável, o eixo PRODUTO não.

## Estratégia do v5

Fechar o P0 em **quatro passos gated por medição**, do pré-requisito ao veredito. Cada milestone tem um gate
executável e **aceita honest-negative como conclusão** (Regra 3): o valor é a régua e o veredito, não um número
prometido. O pgvector permanece como **oráculo de controle** nos benchmarks (recall-parity gate), mesmo tendo
sido removido da distribuição (M70) — instalável só no ambiente de benchmark.

## Milestones

### M60 — [ ] Qualidade de recall do HNSW próprio — recall≥0.99 a escala (o pré-requisito do P0)

> **Já no ROADMAP.md** (spun-off do M57). É a FUNDAÇÃO do v5 — sem recall≥0.99 no grafo próprio, nenhum claim de
> superioridade se sustenta. Discover linha-a-linha theodb_hnsw vs pgvector no MESMO grafo; isolar a origem do
> gap (distribuição de níveis `ml`, entry-point da descida greedy, ou heurística de vizinhos). **Risco ALTO** (3
> levers já refutados por medição; pode exigir vários ciclos). DoD e diagnóstico completos no bloco M60 do
> `ROADMAP.md`. **Dependência de tudo abaixo.**

### M71 — [ ] Latência-superior do AM (scan hot-path v2) — o "priorizar latência do AM" do P0

**Objective:** hoje a latência de query do theodb_hnsw é **paridade** com o pgvector, não superioridade. Empurrar
o hot-path do scan (partial-read page-native M35 → v2: prefetch, layout de página, dispatch SIMD do cosine/IP no
scan — reuso do M58) para **p50 medidamente ≤ pgvector** a recall≥0.99 (produção), num same-graph micro-bench
(criterion, M46/M47) + um benchmark e2e a 1M. Honest-negative aceito (se a régua disser paridade, o veredito é
paridade — não inflar).

**Definition of done:**
- [ ] Discover (R0): o que o pgvector faz no hot-path do scan HNSW que o theodb não faz (mesmo grafo) — prefetch/página/SIMD.
- [ ] Fix com **p50 do theodb_hnsw ≤ pgvector a recall≥0.99** num same-graph micro-bench + e2e a 1M, sem regressão de recall → `wiki/benchmarks/m71-scan-latency.md` + `benchmarks/artifacts/m71-scan-latency.json`.
- [ ] Veredito honesto (superior / paridade / honest-negative) com mean±std ≥3 runs.

**Dependencies:** M60 (recall≥0.99 — medir latência abaixo do recall de produção). **Risco (MÉDIO-ALTO):** ganhos de hot-path costumam ser fator-constante; a régua pode dar paridade.

### M72 — [ ] QPS a 1M+ multi-cliente — throughput sob concorrência real

**Objective:** o M32/M34 mediram p50 single-client. Faltam **QPS a 1M sob N clientes concorrentes** (o regime
real de produção) — theodb_hnsw/ivfflat vs pgvector, mesmo hardware/dataset. Provar (ou refutar honestamente)
que o throughput multi-cliente é competitivo, incluindo o efeito do lock/buffer do índice sob carga.

**Definition of done:**
- [ ] Harness multi-cliente (N conexões, QPS agregado, p50/p95/p99) a 1M×128d (SIFT1M) — theodb vs pgvector, ≥3 runs, mean±std → `wiki/benchmarks/m72-qps-multiclient.md` + `benchmarks/artifacts/m72-qps-multiclient.json`.
- [ ] Veredito honesto de QPS multi-cliente (competitivo / gap medido) com a origem do gap identificada.

**Dependencies:** M60, M71. **Risco (MÉDIO):** contenção de buffer/lock sob concorrência; o gap pode ser estrutural (índice persistente vs library in-memory).

### M73 — [ ] Head-to-head MEDIDO vs ScaNN/AlloyDB (re-run pós-M60/M71/M72) — o VEREDITO de superioridade

**Objective:** re-rodar o head-to-head do M33 (SIFT1M, mesmo hardware/query-set) **depois** de M60 (recall) +
M71 (latência) + M72 (QPS), e emitir o **veredito de superioridade vetorial rastreável** do North Star. Honesto:
o resultado pode ser (a) fechou/reduziu o gap vs ScaNN, (b) paridade own-code + trade-off de QPS documentado, ou
(c) honest-negative. Em qualquer caso, o v5 entrega a **prova medida de ONDE o TheoDB está** vs o SOTA — o que o
North Star exige (não uma vitória inventada).

**Definition of done:**
- [ ] Re-run M33 (ScaNN OSS proxy do AlloyDB; caveat library-vs-database documentado) a recall≥0.99, ≥3 runs → `wiki/benchmarks/m73-headtohead-verdict.md` + `benchmarks/artifacts/m73-headtohead-verdict.json`.
- [ ] **ADR de veredito do North Star vetorial:** superior / paridade+trade-off / honest-negative, com a evidência e a decisão de posicionamento (o que o produto pode/NÃO pode claim, per `public-copy.md`).
- [ ] Atualizar `goto-p0-vector-superiority` (memória) + o CLAUDE.md North Star com o estado MEDIDO final.

**Dependencies:** M60, M71, M72. **Risco (ALTO):** o gap ScaNN (~25×) é anisotrópico+AH; M57/M59 já foram honest-negative nesse eixo — o veredito honesto pode ser "paridade own-code, não superioridade de QPS pura".

### M74 — [ ] (CONDICIONAL) Quantização SOTA no índice — só se M73 identificar um lever viável não-refutado

**Objective:** SÓ arranca se o M73 (ou os discover de M71/M72) apontar um caminho de quantização **não** já
refutado por M57 (SBQ) / M59 (anisotrópica+AH no carrier HNSW) — ex.: uma formulação anisotrópica diferente, AH
SIMD num carrier IVFFlat (não HNSW), ou RaBitQ/rerank a outra régua. Measurement-first + gate de trigger: **não
implementar sem um blueprint com evidência de que o lever é viável** (anti-sunk-cost, D3). Pode terminar como
"nenhum lever viável — o veredito M73 é final".

**Definition of done:**
- [ ] Discover-gate: blueprint com evidência (paper + medição de viabilidade) de um lever de quantização não-refutado → decisão implementar/não-implementar.
- [ ] SE implementar: recall≥0.99 + ganho de QPS MEDIDO vs o baseline M73, sem regressão → `wiki/benchmarks/m74-quant-sota.md` + `benchmarks/artifacts/m74-quant-sota.json`.
- [ ] SE não: ADR honesto "nenhum lever viável pós-M57/M59; o veredito M73 é o estado final do pilar".

**Dependencies:** M73 (o veredito + a origem do gap). **Risco (ALTO):** dois levers já refutados; este é condicional por design.

## Sequência

```
M60 (recall≥0.99) ──▶ M71 (latência) ──▶ M72 (QPS multi-cliente) ──▶ M73 (veredito head-to-head)
                                                                          │
                                                                          └──▶ M74 (quant SOTA, CONDICIONAL)
```

M60 é o pré-requisito de tudo (medir superioridade abaixo do recall de produção). M71→M72→M73 são o núcleo do
veredito. M74 só existe se M73 abrir um caminho não-refutado.

## O que o v5 NÃO é (honestidade)

- **NÃO é uma promessa de vencer o ScaNN.** Dois levers (M57 SBQ, M59 anisotrópica+AH) já deram honest-negative.
  O v5 entrega o **veredito medido** — que pode ser paridade own-code + trade-off documentado.
- **NÃO reabre HA / replicação / control-plane** — deploy/plataforma, fora do escopo deste repo (CLAUDE.md).
- **NÃO faz claim de performance sem artefato** em `wiki/benchmarks/` (Regra 5 / `public-copy.md`).

## Relação com os roadmaps anteriores

- v1 (M0–16), v2 (M17–M60), v3 (M61–M68), v4 (M69–M70): entregues (exceto M60, que é a fundação do v5).
- Detalhe estratégico do v3 em `ROADMAP-v3.md`. Este v5 fecha o pilar P0 que o v3 conscientemente diferiu.
