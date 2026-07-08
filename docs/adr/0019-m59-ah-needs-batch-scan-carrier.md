# ADR 0019 — Anisotropic-PQ + AH implementado, mas o ganho exige um carrier de batch-scan (veredito D3 do M59)

**Status:** Accepted · **Date:** 2026-07-08 · **Milestone:** M59 · **Gate:** D3 · **Deciders:** CTO (paulohenriquevn)
**Relacionado:** ADR `0018` (M57 — SBQ não é superior; este ADR é o próximo passo algorítmico), ADR `0015` (own-AM), ADR `0002` (North Star / measurement-first)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m59-anisotropic-ah-blueprint.md` (ADR-D4 previu este risco)
**Evidência:** `docs/benchmarks/m59-anisotropic-ah.md` + `docs/benchmarks/m59-raw/*.json`

## Contexto e problema

O gap de ~25× QPS vs ScaNN (`docs/benchmarks/m33-scann-headtohead.md`) é o eixo P1 do North Star vetorial. O
M57 (ADR-0018) mediu que o SBQ (bit-quantization) NÃO fecha esse gap (fator-constante). O blueprint do M59
identificou o eixo algorítmico real: **quantização anisotrópica (ScaNN score-aware loss) + Asymmetric Hashing
com LUT16 SIMD (`pshufb`)**. O M59 implementou e mediu.

## Decisão

**Reconhecer, por medição, que o eixo anisotrópico-PQ+AH está corretamente implementado mas o ganho de QPS não
se materializa no carrier HNSW — ele exige um carrier de batch-scan contíguo (IVF).** O gap ~25× NÃO é fechado
pelo M59 no HNSW (honest-negative), mas a fundação (codebook anisotrópico + kernel AH SIMD + persistência v3 +
scan wiring) está entregue, testada e é a base do próximo milestone.

## Evidência (500k×768d cosine, box limpa, `theodb:m59fix`)

Paridade AQ vs f32 a recall casado ≥0.957, in-RAM (1.01×) e sob pressão de RAM 1.3 GB (1.03×) — não ≥2×.
A 20k in-RAM o AQ vence o f32 (1.16×, o primeiro índice quantizado do theodb a superar o f32), provando que o
lever AH-LUT é o certo; a vantagem evapora a escala pela localidade de acesso do HNSW.

## Por que (mecanismo medido — generaliza)

1. **HNSW tem localidade de acesso** → o índice f32 não thrasha sob pressão → os códigos AQ de 4 bytes (768×
   menores que o f32) não compram vantagem de I/O. Mesma lição do M57.
2. **O kernel AH batched (o lever de 4.75×) precisa de candidatos CONTÍGUOS** para o `pshufb` (32 lookups/instr).
   A caminhada do HNSW visita nós **um-a-um** (pointer-chasing) → só o `ah_score` single-code roda, que é *mais
   lento* que escalar (Fase 2). **O ganho SIMD do AH é inacessível no carrier HNSW.** ScaNN/FAISS obtêm o 25×
   com um carrier **IVF de batch-scan** (listas contíguas). Este é o risco que o blueprint ADR-D4 previu — agora
   medido, não especulado.

## Consequências

- **North Star P1 (gap 25×): NÃO cumprido pelo M59.** Segue aberto — mas o eixo algorítmico está resolvido; o
  que falta é o carrier.
- **Próximo milestone (M61-class, motivado por medição):** o **carrier IVF de batch-scan** que alimenta o kernel
  AH batched (`ah_score_block`, já implementado e medido a 4.75×) com listas contíguas — o fallback ADR-D4. É
  onde o AH deve materializar o ganho, porque é o carrier que ScaNN/FAISS usam. Alternativa complementar:
  disk-resident/DiskANN (SOAR).
- **A fundação M59 fica no código** (não removida): codebook anisotrópico (`aq.rs`), kernel AH SIMD (`vec/ah.rs`),
  persistência v3 (`hnsw_page.rs`), reloption + scan wiring — tudo testado (175 pg_tests), backward-compat v1/v2
  intacta, opt-in (`WITH (pq_subspaces=M)`, default off). É a base direta do carrier IVF.
- **Sem claim de "superioridade vetorial"** (Regra 5): o artefato honest-negative é o único claim.

## Opções consideradas

1. **Declarar o M59 fechando o gap** — rejeitada: a medição mostra paridade (1.01–1.03×), não ≥2×. Seria claim
   falso (Regra 5).
2. **Remover o AQ (não vale)** — rejeitada: a 20k já supera o f32 (o lever é correto); o AQ é a fundação do
   carrier IVF (próximo milestone). Remover seria descartar a base medida-como-correta.
3. **Honest-negative + reenquadrar para o carrier de batch-scan (esta decisão)** — medir, registrar que o eixo
   está certo mas o carrier HNSW não o alimenta, e mover o esforço para o carrier IVF (onde ScaNN materializa o
   ganho). Alinha com measurement-first + anti-sunk-cost (CLAUDE.md).

## Caveats

Dados gaussian-mixture sintéticos (não SIFT1M) — a direção é mecânica (localidade HNSW + AH batched inacessível
pointer-chasing), mas absolutos podem mover; follow-up SIFT1M. Teto de recall 0.958/0.974 < 0.99 é o gap de grafo
(M60, ortogonal — comparação a recall casado). `pq_subspaces=8` coarse; mais subespaços não mudam o veredito de
carrier.
