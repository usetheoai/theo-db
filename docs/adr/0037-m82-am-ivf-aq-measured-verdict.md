# ADR-0037 — M82: veredito MEDIDO do pg_scann (IVF-AQ+AH como Access Method) no caminho real do Postgres

- **Status:** Accepted (2026-07-11)
- **Contexto:** M82 (Roadmap v6, fase 7 — track pg_scann M75→M82) — o head-to-head **final MEDIDO** do algoritmo
  ScaNN (IVF + AVQ + Asymmetric-Hashing batched-LUT + rerank) shipado como **Access Method próprio** do PostgreSQL
  (`theodb_ivfflat` v4), rodado end-to-end **dentro do Postgres** a 1M (SIFT1M), contra a baseline f32-IVF own-code
  na MESMA tabela (rigor A/B same-data do M46) e os pontos de referência do M33.
- **Natureza:** registra um **veredito medido** (onde o pg_scann-as-AM está vs o SOTA), não uma mudança de mandato.
  O mandato LOCKED permanece `docs/adr/0002`; o reposicionamento é `docs/adr/0033` (decisão do owner).
- **Relação:** confirma e **estende** o veredito M73 (`docs/adr/0035`) com uma medição no caminho AM (não só
  in-memory). Fecha o track pg_scann.

## Decisão

O pilar vetorial do North Star (superioridade de QPS vetorial sobre o ScaNN/AlloyDB) é medido como
**HONEST_NEGATIVE_FINAL** também pelo caminho pg_scann-as-Access-Method. Especificamente:

1. O índice **v4 IVF-AQ+AH é funcionalmente correto**: recall@10 **byte-idêntico** ao f32-IVF exato em todos os
   níveis de probe (AH pruning + rerank exato é **lossless** nestes settings).
2. O índice v4 **não entrega ganho de QPS medível** sobre o f32-IVF no AM (diferenças dentro do ruído best-of-3).
3. A recall 0.985 o v4 mede **78.5 QPS** — classe do pgvector f32-IVF do M33 (78 QPS @ 0.99), **~24× abaixo do
   ScaNN** (1920 QPS @ 0.99).

Artefato: `docs/benchmarks/m82-pgscann-headtohead.{md,json}` (SIFT1M full, GT oficial válido a 1M, DO 8 vCPU).

## Causa-raiz (por que os 5-7× in-memory do M75 sumiram)

O spike M75 mediu ~5-7× QPS in-memory para IVF-AQ+AH, com o caveat explícito *"in-memory single-thread, no pgrx
page/WAL tax (M76+)"*. O M82 confirma que o caveat era load-bearing.

No layout v4 atual os códigos AQ estão **interleaved** com os vetores f32 nas mesmas páginas por-lista
(`[ids][f32][codes]`). Ler os códigos para pontuar por AH **também pagina os vetores f32**, então o scan paga o
**I/O f32 completo** de qualquer jeito. O AH LUT só economiza o **compute** da distância exata — e compute **não é
o gargalo**. O scan do AM é **I/O + centroid-probe bound**, exatamente os "system-level overheads" documentados por
**arXiv:2603.23710 (SIGMOD 2026)**. O ganho in-memory não sobrevive ao AM baseado em página.

## Alternativas consideradas

- **(A) Reportar o achado honesto e fechar o track (ESCOLHIDA).** Measurement-first, anti-sunk-cost: o track
  entregou o lifecycle v4 correto e testado; a performance medida é null no AM; o valor é a prova medida final +
  a semente honesta do próximo lever. Alinha com Regra 3 (honestidade), Regra 5 (perf é claim medido) e a postura
  honest-negative-aceito do Roadmap v5 (`[[roadmap-v5-vector-superiority]]`).
- **(B) Redesenhar o layout separando códigos e f32 em páginas distintas (FastScan/ScaNN storage) e re-medir.**
  Rejeitada para M82: é redesign de storage além do escopo benchmark+verdict da fase 7, e não há evidência prévia
  de que o gap de paradigma (não pagar MVCC/WAL + LUT anisotrópico em loop apertado) seja fechável por extensão PG
  permissiva — o M73 já mediu que não é. Registrada como **semente de próxima descoberta** (não shipada, não é
  claim), não como trabalho de M82.
- **(C) Forçar um número de superioridade reduzindo escala/cherry-picking probes.** Rejeitada — violaria Regra 5 e
  o council-benchmark ("mediu ou está supondo?").

## Consequências

- **Posicionamento permitido** (`.claude/rules/public-copy.md` + ADR-0035): "paridade recall + memória
  billion-scale + AI-native/HTAP/aberto"; **jamais** "mais rápido que o AlloyDB no vetor". O v4 IVF-AQ agrega
  **compressão de memória** (16 bytes/vec vs 512 bytes f32 — 32× nos códigos, usados como candidate-filter lossless)
  sem custo de recall, o que é um benefício real de **footprint**, não de QPS.
- **`amcostestimate` é v4-aware** (`theodb_rs/src/am/cost.rs` — lê `read_ivf_aq_meta` para o `dir.len()`/nlists no
  ratio `probes/lists`), então o planner trata o índice v4 como um IVF (custo ∝ nprobe), coerente com o medido.
- **Track pg_scann fechado.** M75→M82 entregaram o AM v4 IVF-AQ completo (build/scan/page/WAL/VACUUM/pending-fold/
  cost, lossless), com o veredito de performance medido e honesto. O lever de separação de storage fica como semente.
- **North Star:** o mandato LOCKED (ADR-0002) permanece até assinatura do owner; a evidência acumulada (M33, M73,
  M82) sustenta a proposta de reposicionamento `docs/adr/0033`.

## Referências

- `docs/benchmarks/m82-pgscann-headtohead.{md,json}` — a medição M82 (este ADR).
- `docs/benchmarks/m75-ivf-aqah-spike.{md,json}` — o spike in-memory (5-7×, GO condicional) que M82 fecha.
- `docs/adr/0035-m73-northstar-vector-verdict.md` — veredito M73 que M82 confirma/estende.
- `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` — mandato LOCKED.
- `docs/adr/0033-north-star-reposition-proposal.md` — proposta de reposicionamento (owner).
- arXiv:2603.23710 (SIGMOD 2026) — system-level overheads: índices cluster-based (ScaNN) podem superar grafos em
  Postgres real; e, simetricamente, o overhead de sistema mascara ganhos de compute in-memory.
