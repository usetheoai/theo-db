# ADR-0040 — M90: inline label filter (Approach A, scan-key/label-in-index) — veredito MEDIDO `GO`

- **Status:** Accepted (2026-07-12)
- **Contexto:** M90 (linhagem pós-M89 "filtered vector search inline/adaptive") — fecha a metade **inline** do gap de
  filtered vector search vs o AlloyDB. O M87 entregou o post-filter (classe pgvector-relaxed_order); o M90 empurra o
  filtro PARA DENTRO da travessia do IVF-AQ. DISCOVER (`knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md`,
  deep research Staff-DB lendo o pgvectorscale real) **corrigiu a arquitetura**: o inline parsimony-correto é
  **Approach A (scan-key/label-in-index)**, NÃO o Custom Scan Provider (Approach B), que é YAGNI aqui → M91.
- **Relação:** consome o M87 (o scan iterativo) e o M89 (build escalável). Artefato: `docs/benchmarks/m90-inline-filter.{md,json}`.

## Decisão

O inline label filter é MEDIDO como **`GO (inline > post)`** (DO c-8 Xeon 8280, 500k, ~1% seletividade, probes=32,
k=10, 100 queries — sign-off council-benchmark):

1. **v7 INLINE: recall@10 = 1.0000, QPS = 208.8.**
2. **v5 M87 POST-filter: recall@10 = 0.5180, QPS = 10.5.**
3. **delta = +0.4820 de recall + ~20× QPS.** A ~1% de seletividade — o regime onde o post-filter fome (quase todos
   os candidatos do rerank-pool falham o filtro) — o inline pula os não-matching na Stage-1 antes de custar um slot,
   então o rerank enche de candidatos MATCHING (recall completo) e dispensa o iterative re-search caro.

**251 pg_tests GREEN** (250 + `v7_inline_label_filter_scans_correct`), zero regressão; vetor-only e v5/v6 sem-label
byte-idênticos (v7 é opt-in na 2ª coluna).

## Como (Approach A — código próprio, Regra 9)

1. **opclass:** `amcanmulticol=true` + opclass DEFAULT `theodb_ivfflat_label_ops` (`OPERATOR 1 &&` em `smallint[]`)
   com `theodb_smallint_array_overlap` próprio → o planner empurra `lbl && '{…}'` como Index Cond.
2. **formato v7:** a code-blob por-lista vira `[ids][labels_fixed][codes]` (`LABEL_K=8` slots + count por vetor,
   co-localizado → Stage-1 lê o label sem random-read extra). Writer streaming (reusa o flush per-lista do M89).
   Contabilidade de páginas idêntica ao v5.
3. **scan:** `amrescan` parseia o ScanKey `&&` → o query label set + `xs_recheck=true`; `scan_ivf_aq_split_v7` lê o
   label co-localizado na Stage-1 e pula os não-overlapping antes do rerank.

## Boundary honesto (o que o M90 NÃO faz)

- Só a coluna de label declarada + `&&`, label `smallint[]`. `WHERE price < 100` numa coluna heap comum ainda
  post-filtra — o inline arbitrary-`WHERE` (Custom Scan Provider + bitmap, o Approach B do AlloyDB) é o **M91**.
- Format bump v7 + REINDEX para usar labels; índices sem-label inalterados (sem REINDEX).
- **NÃO é claim de QPS-superior** vs ScaNN/AlloyDB (teto de paradigma M73/M82 permanece) — é claim de
  **recall-estável-sob-filtro-de-label-seletivo** (com um bônus grande de QPS), medido.
- Dados sintéticos com clusters bem-separados (recall significativo, ao contrário do M88 tie-dense); a comparação
  inline-vs-post é same-data → válida independentemente; single run.

## Desvio do plano/milestone (Regra 3 — a DISCOVER corrigiu)

O texto original do M90 (roadmap-feature) dizia "Custom Scan Provider". A DISCOVER (lendo o pgvectorscale real,
permissivo) mostrou que ele usa o **scan-key/label** (Approach A) — muito menor risco e suficiente para o DoD (um
filtro de label seletivo). O Custom Scan (Approach B, arbitrary-WHERE) foi movido para o M91. O milestone foi
re-escopado no roadmap; este ADR registra a decisão measurement-first.

## Alternativas consideradas

- **Custom Scan Provider agora (o texto original)** — rejeitado p/ o DoD: YAGNI (arbitrary-WHERE não está no DoD),
  máquina pesada de planner/executor → M91.
- **Manter só o M87 post-filter** — rejeitado: MEDIDO como recall 0.52 a ~1% (fome no regime seletivo).
- **Variable-length labels** — deferido: `LABEL_K=8` fixo cobre a maioria dos filtros de tag/categoria; VLA é
  follow-up (documentado).

## Relação com ADRs anteriores

- Consome M87 (post-filter) + M89 (build). Estende a linhagem inline/adaptive. Não altera `0002`/`0033`.
