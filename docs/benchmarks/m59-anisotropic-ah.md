# M59 (P1) — Quantização anisotrópica + Asymmetric Hashing SIMD: recall×QPS (veredito D3)

**Veredito: HONEST-NEGATIVE com achado mecânico preciso.** O índice anisotrópico-PQ + AH (`WITH
(pq_subspaces=M, pq_bits=4, aq_threshold=η×1000)`) foi **implementado e validado por correção** (Fases 1-4,
175 pg_tests GREEN, backward-compat v1/v2 intacta), mas o ganho de QPS que fecharia o gap ~25× vs ScaNN
**NÃO se materializa no carrier HNSW**: a escala (500k×768d) o AQ está em **paridade** com o f32 (1.01–1.03×),
in-RAM e sob pressão de RAM. A causa-raiz é medida e mecânica — o mesmo padrão do SBQ (M57), agora entendido
com precisão. Consequência: o lever anisotrópico está correto e é a base; o ganho exige um **carrier de
batch-scan contíguo (IVF)**, não a caminhada pointer-chasing do HNSW. ADR `docs/adr/0019`.

## Método

- **Harness:** `benchmarks/run_m59_aq.py` (recall×QPS a escala, AQ vs SBQ vs f32 vs pgvector) +
  `benchmarks/run_m59_pressure.py` (split `--phase build|measure` para constranger RAM ENTRE build e medição,
  o padrão M57). Ambos reusam o harness M57 (`_make_dataset` gaussian-mixture, `_queries`, `_ground_truth`,
  `_measure`, `_conn` com keepalives) — Rule 9. GT = seqscan exato cosine.
- **Ambiente:** droplet DigitalOcean c-8 (8 vCPU / 16 GB), box **limpa** (`load_per_run < 1.5`, lição m46),
  imagem `theodb:m59fix` (com o fix do codebook multi-página), `--network host --shm-size=8g`,
  `shared_buffers=1GB`. Dados gaussian-mixture (256 centros), cosine, dim=768.
- **Config AQ:** `pq_subspaces=8` (sub_dim 96), `pq_bits=4` (LUT16), `aq_threshold=4100` (η=4.1 — o ponto
  operacional T=0.2 do ScaNN, Guo et al. 2020 Theorem 3.4). Códigos: `⌈8/2⌉ = 4 bytes/vetor` (vs f32
  768×4 = 3072 bytes → **768× menores**). `over_fetch` é o knob de recall-recovery varrido.

## Resultado 1 — recall×QPS in-RAM (a vantagem existe a escala pequena, evapora a escala)

| n | AQ recall / QPS | SBQ recall / QPS | f32 recall / QPS | pgvector recall / QPS | AQ/f32 |
|---|---|---|---|---|---|
| 20k | **0.995 / 1494** | 0.995 / 1298 | 0.995 / 1286 | 1.0 / 1259 | **1.16×** |
| 100k | 0.978 / 1017 | 0.978 / 1041 | 0.978 / 1047 | 0.996 / 484 | 0.97× |

- A **20k in-RAM o AQ VENCE o f32 (1.16×)** — o **primeiro** índice quantizado do theodb a superar o f32 (o
  SBQ do M57 era 0.35–0.77×, mais lento). Confirma que o AH-LUT é o lever certo (não bit-quantization).
- A **100k a vantagem evapora** (AQ ≈ f32, paridade) — o grafo satura em recall 0.978 (o teto do HNSW do
  theodb, gap M60/ortogonal; pgvector chega a 0.996) e o scoring deixa de ser o gargalo.

## Resultado 2 — AQ vs f32 sob pressão de RAM (o discriminador D3, 500k×768d)

recall casado ≥0.957 (o teto comum do grafo a esta escala), QPS 1-cliente:

| Regime | AQ QPS | f32 QPS | **AQ/f32** |
|---|---|---|---|
| in-RAM (16 GB) | 204 | 201 | **1.01×** |
| pressão (`--memory=1.3g`, < índice f32 ~1.5 GB) | 207 | 202 | **1.03×** |

**Paridade em ambos os regimes.** A tese ≥2× (códigos AQ de 4 bytes cacheiam enquanto o f32 de 3072 bytes
spilla) **NÃO se materializou** — nem sob pressão a 1.3 GB (onde o índice f32 excede a RAM).

## Por que o ganho não materializou (mecanismo — medido, preciso)

1. **HNSW tem localidade de acesso** (a lição do M57, confirmada para o AQ): uma query toca ~`ef·log N` nós; as
   páginas quentes ficam cacheadas mesmo com o índice f32 excedendo a RAM → o f32 **não thrasha** sob pressão →
   os códigos pequenos do AQ não compram vantagem de I/O.
2. **O kernel AH batched (o lever de 4.75× — `docs/benchmarks/`, Fase 2) precisa de candidatos CONTÍGUOS** para
   alimentar o `_mm256_shuffle_epi8` (32 lookups/instrução). Mas a caminhada do HNSW visita nós **um-a-um**
   (pointer-chasing dos vizinhos), então na prática só o `ah_score` **single-code** roda — e esse, medido na
   Fase 2, é *mais lento* que um table-lookup escalar (paga `_mm_extract_epi8`/subespaço). O ganho SIMD do AH
   fica **inacessível no carrier HNSW**. Isto é **exatamente o risco ADR-D4 do blueprint**, agora medido.

O ScaNN/FAISS obtêm o 25× porque usam um **carrier IVF de batch-scan**: listas invertidas contíguas onde o
`pshufb` pontua 32+ códigos por instrução. O eixo algorítmico (anisotropic-PQ + AH) está correto e implementado;
o **carrier** é a peça que falta.

## Veredito D3

- **NÃO fecha o gap ~25× vs ScaNN no carrier HNSW.** Honest-negative — o AQ é recall-competitivo (0.958 vs f32
  0.974, um hair abaixo pela quantização coarse pq_subspaces=8) e QPS-paridade a recall casado, não ≥2×.
- **O eixo anisotrópico + AH está implementado e correto** (a fundação medida): a 20k in-RAM já supera o f32
  (1.16×), provando o lever. O que falta é o **carrier de batch-scan contíguo (IVF)** — o próximo milestone
  (M61-class, o fallback ADR-D4), agora **motivado por medição**, não especulação.
- Decisão registrada em `docs/adr/0019-m59-ah-needs-batch-scan-carrier.md`.

## Caveats honestos

1. **Dados gaussian-mixture sintéticos** (não SIFT1M) — a direção (paridade no HNSW por localidade + AH batched
   inacessível pointer-chasing) é mecânica, não dependente do dataset. Follow-up: SIFT1M real.
2. **Teto de recall 0.978/0.958 < 0.99** a escala — o gap de qualidade do grafo HNSW do theodb (M60,
   ortogonal ao AQ; afeta f32 e AQ igualmente). A comparação é a recall CASADO, então o veredito do AQ é
   robusto ao teto.
3. **`pq_subspaces=8` coarse** (sub_dim 96) — mais subespaços dariam recall maior mas códigos maiores; não muda
   o veredito de carrier (o gargalo é a caminhada, não a granularidade).
4. **1-cliente** — QPS multi-cliente pode mudar absolutos, não a razão de paridade (mesmo carrier).

## Reprodução

```
# build (16 GB): PQ_SUBSPACES=8 python3 run_m59_pressure.py --phase build --n 500000 --dim 768 --make --state S.json
# in-RAM:        PQ_SUBSPACES=8 python3 run_m59_pressure.py --phase measure --state S.json --mem-note inram
# pressão:       docker update --memory=1300m --memory-swap=1300m pgm59 && sync && echo 3>/proc/sys/vm/drop_caches
#                PQ_SUBSPACES=8 python3 run_m59_pressure.py --phase measure --state S.json --mem-note pressure_1.3g
```

Dados brutos: `docs/benchmarks/m59-raw/{m59_smoke,m59_100k,m59p_inram,m59p_pressure}.json`.
