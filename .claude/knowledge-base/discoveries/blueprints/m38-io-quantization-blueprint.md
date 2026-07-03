# Blueprint: M38 — quantização de I/O no scan (o gate measurement-first FALSIFICOU a abordagem SBQ)

> **Discovery verdict:** ⚠️ **ABORDAGEM SBQ FALSIFICADA para o gate de recall — o milestone precisa de decisão.** A
> medição em dados SIFT reais mostra que o SBQ (quantização escalar, a abordagem primária do plano M38) atinge no
> máximo **recall 0.947 (bits=4, over_fetch=40) vs 1.0 do baseline f32** — e o SBQ-1bit fica em ~0.77. O gate do
> DoD ("recall preservado ≥ baseline") NÃO é atingível com SBQ. Este é o gatilho de escalada que o próprio DoD do
> M38 antecipou. Método: measurement-first (`theodb.sbq_knn` real vs seqscan exato, SIFT 120k×128).

**Slug:** `m38-io-quantization` · **Owner:** paulohenriquevn · **Created:** 2026-07-03

## Context

M38 foi escopado (split do M36) para cortar o gargalo `reads` (~44–51% do custo de scan, medido no M36)
persistindo códigos SBQ menores nas páginas de lista (16 B/vetor vs 512 B f32 → ~32× menos bytes lidos), rankeando
por Hamming, com rerank f32 do top over_fetch. O DoD gateou em **recall preservado ≥ baseline**, com uma cláusula
de escalada explícita: *"Se SBQ-1bit regredir < baseline mesmo com over_fetch, escalar bits ou PQ/ADC via ADR."*

## A medição (o achado que reformula o milestone)

`theodb.sbq_knn` (a impl SBQ real do M22) vs o seqscan exato, **SIFT 120k×128 (embeddings reais, estruturados)**,
100 queries, lists=120, probes=60 (o baseline f32 atinge recall **1.0** nesse ponto — o candidato-set contém todos
os vizinhos verdadeiros, então o teto de recall é 1.0):

| config | recall@10 |
|---|---|
| **f32 baseline** (theodb_ivfflat probes=60) | **1.0000** |
| SBQ bits=1, over_fetch=10 / 20 / 40 | 0.554 / 0.669 / **0.774** |
| SBQ bits=2, over_fetch=10 / 20 / 40 | 0.652 / 0.748 / **0.854** |
| SBQ bits=4, over_fetch=10 / 20 / 40 | 0.784 / 0.883 / **0.947** |

**Conclusão (dados, não suposição):** o SBQ **não preserva recall ao baseline** em dados reais. A melhor config
(bits=4/of=40 = 64 B/vetor, só 8× menor que f32) atinge 0.947 — ainda abaixo de 1.0. O SBQ-1bit (a proposta
primária, 16 B/vetor) fica em ~0.77. Em dados aleatórios uniformes (pior caso p/ quantização) é ainda pior
(SBQ-1bit 0.22–0.44). A premissa "SBQ corta I/O a recall preservado" está falsificada.

## Por quê (ancorado no código + na teoria)

O `sbq::knn` (`theodb_rs/src/sbq.rs:114`) gera candidatos pelo carrier IVFFlat f32 (`candidate_positions`, probes)
— então o candidato-set JÁ contém os vizinhos verdadeiros (recall-ceiling 1.0). A perda acontece no **ranking
Hamming** (`sbq.rs:158-159`): a quantização escalar por-dimensão perde informação de ranking demais, empurrando os
vizinhos verdadeiros pra fora da janela `k·over_fetch` antes do rerank f32. O rerank só recupera o que o Hamming
mantém — e o Hamming-1bit não mantém o suficiente. É teoria conhecida: **quantização escalar (SBQ) preserva muito
menos informação de ranking por byte que quantização de produto (PQ)** — por isso ScaNN/FAISS usam PQ + distância
assimétrica via LUT, não quantização escalar. (pgvectorscale usa SBQ, mas com o grafo StreamingDiskANN dando um
candidato-set muito melhor que probes IVFFlat — a qualidade do candidato compensa.)

E o over_fetch alto que seria necessário para chegar perto do baseline ergue o custo do rerank f32 (re-lê f32 do
over_fetch) — erodindo o ganho de I/O que era o ponto todo.

## As opções honestas (a decisão do milestone)

1. **Escalar para PQ (Product Quantization) — o answer técnico correto, implementação grande.** Codebooks
   aprendidos por subespaço (k-means por sub-vetor), distância assimétrica query-f32-vs-DB-código via LUT (ADC —
   Asymmetric Distance Computation). É o que ScaNN/FAISS fazem; preserva muito mais recall por byte que SBQ. Mas é
   um milestone substancial (novo quantizer + LUT SIMD + persistência + gate de recall a 1M), não um slice.
2. **Re-escopar M38 para um lever de `reads` que NÃO custa recall.** Ex.: layout de página melhor (packing mais
   denso, menos overhead por candidato) ou poda de candidatos (menos candidatos varridos via centroides melhores)
   — ataca `reads` sem quantização lossy. Ganho menor mas recall-zero-risco.
3. **Entregar SBQ como um operating-point OPCIONAL (recall/QPS tradeoff explícito), não substituindo o exato.** Um
   `theodb_ivfflat` com pré-filtro SBQ = mais rápido a recall ~0.95, ADITIVO ao path exato (o usuário escolhe via
   GUC). Honesto, mas NÃO cumpre o gate "recall preservado ≥ baseline" do DoD como escrito.

## Coverage Corner 1 — Integration Tests
Recall-vs-baseline gate (o que falsificou a abordagem): `theodb.sbq_knn` vs seqscan exato em SIFT real. Qualquer
approach escolhido é gated pelo mesmo teste de recall a 1M.

## Coverage Corner 2 — Dependencies
Nenhuma nova (SBQ e PQ são std-only + `sbq.rs`/novo quantizer). PQ reusa k-means do `ann/ivf.rs`.

## Coverage Corner 3 — Tools
`theodb.sbq_knn` (a impl SBQ existente, usada para medir), `benchmarks/theodb_bench/` (recall/QPS),
`THEODB_SCAN_PROFILE` (reads).

## Coverage Corner 4 — Techniques
SBQ (medido, insuficiente p/ o gate). PQ/ADC (o SOTA — codebooks por subespaço, LUT assimétrica; ScaNN
arXiv:1908.10396, FAISS). O trade-off recall-por-byte: escalar vs produto.

## ADRs

### ADR-1 (proposto) — SBQ não passa o gate de recall; PQ é o answer técnico, mas é decisão de escopo
**Decisão:** a abordagem SBQ do plano M38 está falsificada pela medição (recall 0.77–0.95 < 1.0 baseline). Escalar
para PQ (opção 1) é o correto tecnicamente mas é um milestone grande. A escolha entre PQ (opção 1), re-escopo p/
lever recall-zero-risco (opção 2), ou SBQ-como-operating-point-opcional (opção 3) é uma decisão de escopo que
precisa do humano (convenção + Regra 1). **Rejeitado:** construir SBQ que regride recall para nominalmente cumprir
o M38 (workaround — viola "SEM WORKAROUNDS" + "100% FUNCIONAL").

## Recommendations
1. **Surface o achado ao humano** (measurement-first funcionou — o SBQ não passa o gate; PQ é grande).
2. Decidir: PQ (grande, correto) vs re-escopo (lever recall-zero-risco) vs SBQ-opcional (tradeoff documentado).
3. Qualquer path é gated pelo recall-vs-baseline a 1M + o profiler `reads`.

## Top 3 risks
- **R1:** construir SBQ mesmo assim → regressão de recall silenciosa (o gate pega, mas o esforço é desperdiçado). → surface antes.
- **R2:** PQ é PhD-level (codebooks + LUT SIMD) — risco de escopo. → medir o recall de um protótipo PQ antes de comprometer.
- **R3:** a medição é em 120k; a 1M as proporções podem mudar, mas o gap de recall SBQ (escalar perde ranking) é estrutural, não de escala.
