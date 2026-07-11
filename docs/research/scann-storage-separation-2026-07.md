# Deep Research — Storage-Separated ScaNN-fidelity IVF-AQ Access Method (o caminho para a classe AlloyDB in-Postgres)

**Data:** 2026-07-11 · **Autor:** deep research web-grounded (R0), 4 frentes paralelas · **Status:** input para o Roadmap v7 (M83+) · **Verdito:** GO CONDICIONAL a um spike D3 (M83)

> Dossiê de pesquisa que responde à semente aberta do ADR-0037 (M82): *o índice v4 IVF-AQ deu zero ganho de QPS porque os códigos estão INTERLEAVED com os f32 nas mesmas páginas — ler os códigos pagina os f32 e paga o I/O completo.* A alavanca é um **layout de página que separa códigos↔vetores brutos**. Todo claim externo tem URL resolvível; todo claim interno tem `file:line`.

---

## 0. A reformulação honesta do alvo (Regra 3 — ler primeiro)

O "gap de ~24× vs ScaNN" (M33/M82) é a **comparação errada de sistema**: ScaNN 1920 QPS é uma **biblioteca in-memory sem imposto MVCC/WAL/heap/buffer-manager**. A referência correta e alcançável dentro do Postgres é o **próprio teto publicado do AlloyDB: "até 4× mais rápido que o pgvector HNSW"** ([Google Cloud, ScaNN for AlloyDB GA](https://cloud.google.com/blog/products/databases/scann-for-alloydb-index-is-ga)).

Evidência decisiva (arXiv:2603.23710, SIGMOD 2026, [HTML](https://arxiv.org/html/2603.23710)):
- **Overhead de sistema = 84.4% dos ciclos de CPU do ScaNN dentro do Postgres** (single-thread); *"a vasta maioria do tempo de query... é gasta em overheads de baixo nível como acesso a página e manipulação de dados."*
- Quantização no pgvector dá **0.75×–1.04× QPS** (nenhum ganho consistente) — *"PGVector-HNSW permanece dominado por acessos aleatórios a página."* **Isto é EXATAMENTE o null do M82** — re-derivamos independentemente um achado SOTA publicado, forte sinal de que a nossa medição é sã, não bug.

**Decomposição honesta do gap (Agente 3):**

| Bucket | Mecanismo | Recuperável | Confiança | Fonte |
|---|---|---|---|---|
| **A — Layout/I/O** | separar códigos↔f32 → scan só-códigos (~32× menos I/O na poda) | **~4–6×** sobre o AM I/O-bound atual | MÉDIA (spike in-memory M75 deu 5-7×; realização em página é não-testada) | M75 spike; VectorChord 0.4 (o rewrite que existe *por causa* deste gargalo) |
| **B — SIMD/AH-LUT** | pshufb block32 in-register | **já em mãos** (kernel existe, `vec/ah.rs:204`); resíduo = anos de tuning AVX-512 anisotrópico | MÉDIA-BAIXA no último incremento | FastScan; AVQ Guo 2020 |
| **C — Paradigma/imposto de sistema** | MVCC/WAL/heap/buffer/per-tuple | **~4–6× IRRECUPERÁVEL** para extensão PG permissiva | ALTA | arXiv:2603.23710 (84.4%); teto AlloyDB (~4×) |

**Alvo honesto:** um AM com storage separado pode plausivelmente recuperar **~4–6×**, aterrissando em **~315–470 QPS @ 0.99** — **classe "AlloyDB-ScaNN in-Postgres"**, ainda **~4–6× abaixo do ScaNN-biblioteca** (estruturalmente inalcançável como extensão transacional). **"Igual ao AlloyDB" é ACHIEVABLE-e-gated; "vencer o ScaNN" não é.**

---

## 1. A alavanca — os 4 SOTA todos separam códigos de vetores brutos

Convergência total (Agente 1), com o nosso v4 sendo o **outlier** que interleava:

- **FAISS FastScan** (`IndexIVFFastScan`, `bbs`): códigos em blocos de 32 transpostos, **separados dos originais**; refinamento lê originais de um índice distinto (`Refine(SQ8)`/`Rflat`). É a origem do nosso próprio kernel block32. [FastScan wiki](https://github.com/facebookresearch/faiss/wiki/Fast-accumulation-of-PQ-and-AQ-codes-(FastScan)) · [André et al. VLDB'15, hal-01239055](https://inria.hal.science/hal-01239055).
- **AlloyDB ScaNN**: scoring sobre códigos comprimidos → rescore de um pool pequeno de vetores brutos (`scann.pre_reordering_num_neighbors`). Quantizadores: SQ8 (default), **AH (Preview, "até 4× comprimido vs SQ8", melhor performance)**, FLAT. [Create ScaNN index](https://cloud.google.com/alloydb/docs/ai/create-scann-index) · [perf overview](https://cloud.google.com/alloydb/omni/containers/15.7.1/docs/ai/scann-vector-query-perf-overview).
- **VectorChord** (vchordrq, RaBitQ IVF — **AGPL, estudar-não-copiar** [[vectorchord-agpl-study-only]]): tupla de códigos block32 com um **ponteiro** (`prefetch`/`head`) para uma cadeia de páginas **separada** de vetores brutos (`VectorTuple`). O rewrite 0.4 existe *exatamente* para parar de ler f32 um-a-um — [confirma que a separação é a alavanca e que o I/O sobre f32 era o gargalo](https://blog.vectorchord.ai/vectorchord-04-faster-postgresql-vector-search-with-advanced-io-and-prefiltering) — **a mesma parede do M82**.
- **pgvectorscale** (StreamingDiskANN, SBQ, permissivo): o nó guarda só `bq_vector` (códigos SBQ) + `heap_item_pointer`; o vetor full é lido **do heap** só no resort (`get_full_distance_for_resort`). [README](https://github.com/timescale/pgvectorscale/blob/main/README.md).

**Layout v5 proposto (concreto):** por lista, duas cadeias de páginas — **CODE range** (`[ids i64×n][AQ codes block32]` ~24 B/vec) + **VECTOR range** (`[f32 dim×4 ×n]` ordinal-addressed, 512 B/vec); dir entry cresce de 12→20 B/lista (dois cursores). Scan em 2 fases: Fase 1 lê **só** as CODE pages, AH-poda para `over_fetch` sobreviventes; Fase 2 random-read do f32 **só** dos sobreviventes. `write_ivf_aq` atual: `page.rs:874-890` (o blob `[ids][f32][codes]`); scan atual: `scan.rs:315` (`read_ivf_list_bytes` lê tudo).

**Redução de I/O (dims nossas: f32=512 B/vec, códigos AQ m=32 = 16 B/vec):** Fase 1 lê **~22× menos bytes/lista** (536→24 B/vec; 95.5% menos), **~5–18× menos page-reads/query**, crescendo com `nprobe`.

---

## 2. Quantizador — MANTER AVQ4+AH (não adicionar SQ8 na poda)

Agente 2, decisivo por Regra 9: **já temos os dois lados do caminho ScaNN-grade** (`aq.rs` AVQ 4-bit + `vec/ah.rs` AH-LUT16 FastScan). O delta é **layout, não codec**.

- **Recall-per-byte** (FAISS codec bench SIFT1M): PQ/AVQ > SQ em códigos compactos. Aos nossos **16 B/vec**, PQ dá recall@1 útil; SQ8 nem existe a 16 B (é 128 B = 1 B/dim). SQ8 é um codec de **rerank** (alta fidelidade, código grande — `Refine(SQ8)`), não de **poda**. [FAISS codec bench](https://github.com/facebookresearch/faiss/wiki/Vector-codec-benchmarks).
- **Throughput SIMD**: AH-LUT16 pshufb ~5× > ADC in-memory, ~1M QPS sem rerank ([FastScan wiki](https://github.com/facebookresearch/faiss/wiki/Fast-accumulation-of-PQ-and-AQ-codes-(FastScan)); [Faiss lib arXiv:2401.08281](https://arxiv.org/html/2401.08281v2)). Nosso kernel já é isto (`vec/ah.rs:204,290`).
- **AlloyDB** move-se *para* AVQ+AH (tier de performance), mantendo SQ8 como default ergonômico — evidência de que AVQ+AH é o codec de scan SOTA.
- **Caveat honesto:** recall@1 asymmetric a 16 B ≈ 0.23 — os códigos de poda são **candidate-filter**, nunca resposta final; exigem rerank exato (que o M82 mediu **lossless** a m=32). É o two-tier que já temos.
- **SQ8**: só como tier de rerank **opcional futuro** (lê 128 B/sobrevivente vs 512 B f32) se o rerank exato virar gargalo — **YAGNI** por ora. **SOAR** ([arXiv:2404.00774](https://arxiv.org/abs/2404.00774)): lever de recall/probe **ortogonal** (fase posterior; ~2–3× menos pontos escaneados a 90% recall, +7.7–17.3% memória).

---

## 3. O spike D3 (M83) — MEDE antes de construir; DEVE ser real-AM

Agente 4, lição do M82 embutida: **o spike NÃO pode ser in-memory** — o M75 já mediu a separação in-memory (5-7×) e o M82 viu evaporar no AM. Um segundo spike in-memory seria *measurement theatre* (Regra 3). O valor é o **modelo de I/O**, que só existe no AM.

**Delta mínimo (real-AM, ~185 LoC, atrás de reloption `separate_storage=on` — v3/v4/236 pg_tests intactos):**

| Mudança | Âncora | LoC | O quê |
|---|---|---|---|
| Write split | `page.rs:874-890` → `write_ivf_aq_split` (v5) | ~80 | duas regiões por lista; dir 12→20 B |
| Read meta v5 | `page.rs:943-992` | ~40 | parse dir 20 B, dois cursores |
| Scan 2-fase | `scan.rs:279-367` → `scan_ivf_aq_split` | ~50 | Fase 1 só-códigos (`ah_score_block` verbatim); Fase 2 random-read f32 só dos sobreviventes (`with_page_item` já existe) |
| Dispatch + reloption | `scan.rs:185` | ~15 | `WITH (pq_subspaces=M, separate_storage=on)` |
| Harness | `benchmarks/m83_split_bench.py` | novo | A/B same-data (M46): v5+v4+v3 na MESMA tabela 1M; sweep probes; recall×QPS best-of-3 + `pages_read` via profiler |

**GATE D3 (recall 0.985; M82: v4=78.5 QPS):**
- **GO** (constrói M84+): v5 ≥ **3× v4** (≥~235 QPS) **E** `pages_read` confirma a queda de I/O da Fase 2.
- **HONEST-PARTIAL** (1.3–3×): separação ajuda mas o *centroid-probe bind* (a outra metade do ADR-0037) domina → M84 mais estreito.
- **HONEST-NEGATIVE-FINAL** (<1.3×): fecha a track, estende o veredito M73/M82 uma terceira vez (anti-sunk-cost).

**⚠️ Caveat #1 (load-bearing) — confound de page-cache:** a 1M×128d o f32 (512 MB) **cabe nos 16 GB de RAM**; "páginas separadas" pode economizar só *faults lógicos/memcpy*, não *I/O de disco*. O spike DEVE reportar **cold-cache E warm-cache**; a vantagem real do ScaNN é a **escala bilhão** onde o f32 NÃO cabe — que 1M só *projeta*, não prova (daí M88). Ignorar isto é repetir a armadilha "ganho in-memory que não sobreviveu" que o M82 já pagou.

---

## 4. Roadmap v7 (M83→M88) — serial, gate-driven; honest-negative é terminal válido em cada etapa

- **M83 — D3 spike:** medir a separação de storage (real-AM). GATE: ≥3× v4 + pages_read confirma → GO; senão fecha honesto.
- **M84 — layout v5 produção** *(gated M83 GO)*: WAL-safe, VACUUM/fold das 2 regiões, `amcostestimate` v5-aware. GATE: crash-safe + QPS ≥ spike.
- **M85 — refine SQ8/PQ-maior** *(gated M84)*: rerank lê 128 B não 512 B. GATE: QPS↑ a recall casado, perda ≤ ε.
- **M86 — SOAR spill** *(gated M85)*: multi-cluster assignment → menos probes p/ mesmo recall (ataca o centroid-probe bind). GATE: recall-a-probes-fixo↑.
- **M87 — filtered ANN + planner** *(gated M86)*: `amcostestimate` escolhe v5 corretamente em WHERE seletivo.
- **M88 — head-to-head bilhão-scale + North Star re-measure** *(gated M87)*: a medição terminal onde f32 NÃO cabe em RAM (regime real da vantagem). ADR estendendo/revisando 0037.

---

## Fontes

**Externas (resolvíveis):** [arXiv:2603.23710 SIGMOD 2026](https://arxiv.org/html/2603.23710) · [FAISS FastScan wiki](https://github.com/facebookresearch/faiss/wiki/Fast-accumulation-of-PQ-and-AQ-codes-(FastScan)) · [FAISS codec bench](https://github.com/facebookresearch/faiss/wiki/Vector-codec-benchmarks) · [The Faiss Library arXiv:2401.08281](https://arxiv.org/html/2401.08281v2) · [André et al. VLDB'15 hal-01239055](https://inria.hal.science/hal-01239055) · [AlloyDB ScaNN GA](https://cloud.google.com/blog/products/databases/scann-for-alloydb-index-is-ga) · [AlloyDB vs pgvector HNSW](https://cloud.google.com/blog/products/databases/how-scann-for-alloydb-vector-search-compares-to-pgvector-hnsw) · [Create ScaNN index](https://cloud.google.com/alloydb/docs/ai/create-scann-index) · [Understanding ScaNN in AlloyDB](https://cloud.google.com/blog/products/databases/understanding-the-scann-index-in-alloydb) · [VectorChord 0.4](https://blog.vectorchord.ai/vectorchord-04-faster-postgresql-vector-search-with-advanced-io-and-prefiltering) · [VectorChord 1.0](https://blog.vectorchord.ai/vectorchord-10-developer-first-vector-search-on-postgres-100x-faster-indexing-than-pgvector) · [pgvectorscale README](https://github.com/timescale/pgvectorscale/blob/main/README.md) · [AVQ Guo 2020 arXiv:1908.10396](https://arxiv.org/abs/1908.10396) · [SOAR arXiv:2404.00774](https://arxiv.org/abs/2404.00774)

**Internas:** `docs/adr/0037-m82-am-ivf-aq-measured-verdict.md` · `docs/benchmarks/m82-pgscann-headtohead.md` · `docs/benchmarks/m33-scann-headtohead.md` · `docs/benchmarks/m73-headtohead-verdict.md` · `docs/adr/0035-m73-northstar-vector-verdict.md` · `docs/benchmarks/m75-ivf-aqah-spike.md` · `theodb_rs/src/am/page.rs:874-890` · `theodb_rs/src/am/scan.rs:279-367` · `theodb_rs/src/vec/ah.rs:204,290` · `theodb_rs/src/am/aq.rs`

**Honestidade:** o whitepaper AlloyDB (PDF) não extraiu limpo — os fatos de quantizador estão ancorados nas docs machine-readable, não no PDF. O ganho recuperável (~4–6×) é MÉDIA-confiança e **contingente ao spike M83**; o confound de page-cache a 1M é o maior risco de um GO falso.
