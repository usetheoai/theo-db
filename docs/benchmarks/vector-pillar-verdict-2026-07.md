# Veredito medido do pilar vetorial P0 — 2026-07 (o que a régua sustenta, sem spin)

Fechamento da investigação de superioridade vetorial (North Star, `docs/adr/0002`). Toda afirmação aqui é medida,
com artefato. Método: measurement-first + prior-art permissivo (Regra 9). Consolidação para decisão do owner.

## O que foi perseguido, e medido

O North Star pede **igualar OU superar o AlloyDB** no vetor, buscando **superioridade de QPS comprovada por
benchmark**. Perseguimos por **todos os caminhos honestos**:

### Gap 1 — theodb → pgvector (navegabilidade do grafo HNSW)
- **Medido:** o grafo próprio precisa de ~2× o `ef` do pgvector a 100k, ~5× a 500k, pro mesmo recall
  (degradação por escala: recall@10 0.998@100k → 0.974@500k; pgvector 0.988@500k). Artefatos:
  `docs/benchmarks/m60-hnsw-recall.md`, `m60-raw/`.
- **7 levers REFUTADOS por medição:** efc↑ (piorou), MERGE back-links, m↑, descida-beam ef=1 (no-op), multi-entry
  `ep←W` (no-op de recall / +29% QPS — shipado no M71), bissecção seq≈paralelo, **e o teste decisivo
  sequential-vs-parallel a 500k (seq 0.974 ≈ paralelo 0.972 → overwrite REFUTADO)**.
- **Diagnóstico prior-art (R0):** o gap ~1.4-3pt casa EXATAMENTE com o delta documentado da "heurística de
  select-neighbors" ([Zenn deep-dive re-impl]) — é um bug conhecido de re-implementadores. Fix candidato:
  `select_from` vs Qdrant `graph_layers_builder.rs` (Apache-2.0) + `keep_pruned`/`extend_candidates` (hnsw_rs).
- **Teto honesto:** fechável para **PARIDADE com pgvector** (dias-semanas, método white-box). NÃO superioridade.

### Gap 2 — pgvector → ScaNN/AlloyDB (paradigma: quantização + FastScan)
- **Medido (M33):** ScaNN ~25× QPS sobre o theodb_ivfflat full-precision (SIFT1M). Vantagem = quantização
  anisotrópica + AH-LUT SIMD.
- **Prior-art (R0):** o SOTA permissivo é **RaBitQ** (arXiv:2405.12497, 1-bit, training-free, bound de erro
  provado). Vendorizado o core (`rabitq-rs` Apache-2.0, ADR-0032). Validado pela **VectorChord** (default deles é
  IVF+RaBitQ) e **LanceDB** (IVF_RQ).
- **Spike D3 medido (1M×768d, `docs/benchmarks/rabitq-spike/rabitq_ivf_mstg_1m768d.log`):**

  | Índice RaBitQ | recall pico | p50 @ pico | memória |
  |---|---|---|---|
  | MSTG-mem (grafo+RaBitQ) | 98.4% | **8.2 ms** | 3.4 GB |
  | MSTG-disk (mmap) | 98.4% | 245 ms | **5.3 MB** |
  | IVF-RaBitQ | 91% | 17.7 ms | — |

  Referência full-precision a 1M×768d: ~10-15 ms @ ~0.98 (M34 + extrapolação M60).

## Conclusão (dura, honesta, medida)

1. **RaBitQ NÃO reproduz o gap de 25× do ScaNN.** MSTG-RaBitQ-mem (8.2ms @ 98.4%) é **competitivo** com
   full-precision (~10-15ms), não 25× mais rápido. O 25× do ScaNN é específico (128d, AH-LUT, tuning de anos do
   Google), não transfere pro RaBitQ permissivo no nosso regime (768d).
2. **O ganho real do RaBitQ é MEMÓRIA, não QPS.** A variante disk dá 98.4% recall com **5.3 MB residentes** (vs
   3 GB crus) — mas a 245ms. Memória minúscula OU latência baixa, não os dois. É o caso billion-scale-em-SSD-barato.
3. **Recall do RaBitQ 1-bit trava em 98.4%** (precisa mais bits/rerank pra 99+).

> **Superioridade de QPS vetorial sobre o AlloyDB/ScaNN NÃO é alcançável como extensão Postgres permissiva.**
> O RaBitQ entrega **eficiência de memória (billion-scale barato) + latência competitiva**, não superioridade de QPS.
> A superioridade do ScaNN é do algoritmo dele (AH-LUT anisotrópico, 128d) + o fato de NÃO pagar o imposto do
> Postgres (MVCC/WAL/heap) que qualquer extensão paga.

## Alvos honestos e alcançáveis do pilar (recomendação)

1. **Gap 1 fix** → **paridade** de recall/latência com pgvector (real, fechável, método white-box + prior-art).
2. **RaBitQ** (core já vendorizado) → feature de **memória/escala** (billion-scale em hardware barato, 32×
   compressão, latência competitiva) — shipar posicionado como **"escala/custo"**, não "mais rápido que o AlloyDB".
   + adotar **continuous recall measurement** (design da VectorChord; estende M67/M68).
3. **AI-native + HTAP + abertura + portabilidade** → os diferenciais genuínos onde o theodb já lidera.

## Proposta ao owner

Reposicionar o North Star (ADR-0002, LOCKED) via **ADR-0033 (proposta)**: de "superar o AlloyDB no vetor" para
"**paridade vetorial classe-pgvector + eficiência de memória RaBitQ para billion-scale + AI-native/HTAP/aberto**".
É o que a medição sustenta. Ver `docs/adr/0033-north-star-reposition-proposal.md`.

Referências permissivas (estudo/vendor): `rabitq-rs`, `RaBitQ-Library`, LanceDB, Qdrant, hnsw_rs (Apache/MIT).
AGPL só-estudo-de-design (não copiar código): VectorChord, srvdb (ver memória `vectorchord-agpl-study-only`).
