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

- A **20k in-RAM o AQ mede 1.16× o f32** — **consistente com o lever AH-LUT (não bit-quantization), mas dentro do
  ruído de 1 run / 20 queries** (Δp50 ~0.1 ms sub-ms, `load_per_run≈4`; NÃO é uma "vitória" decision-grade — o
  `analysis-golden-rule.md § 3` pede ≥3 runs mean±std). Ainda assim é o 1º sinal de um quantizado do theodb ≥ f32
  (o SBQ do M57 era 0.35–0.77×, mais lento). O veredito D3 **não depende deste ponto** — o discriminador é a
  paridade a 500k (abaixo), robusta.
- A **100k a paridade (AQ ≈ f32)** já aparece — o grafo satura em recall 0.978 (o teto do HNSW do theodb, gap
  M60/ortogonal; pgvector chega a 0.996) e o scoring deixa de ser o gargalo.

## Resultado 2 — AQ vs f32 sob pressão de RAM (o discriminador D3, 500k×768d)

recall casado ≥0.957 (o teto comum do grafo a esta escala), QPS 1-cliente. **1 run** (não decision-grade
estatístico — o padrão é a paridade, não o ponto exato). O f32 alcança um recall MAIOR (0.974 vs AQ 0.958) se
aceitar ~26% menos QPS — o AQ satura em 0.958 (quantização coarse); a comparação abaixo é ao recall CASADO:

| Regime | AQ recall / QPS | f32 recall / QPS | **AQ/f32 (recall casado)** |
|---|---|---|---|
| in-RAM (16 GB) | 0.958 / 204 | 0.974 / 201 (casado: 202) | **1.01×** |
| pressão (`--memory=1.3g`, < índice f32 ~1.5 GB) | 0.958 / 207 | 0.974 / 202 | **1.03×** |

**Paridade em ambos os regimes.** A tese ≥2× (códigos AQ de 4 bytes cacheiam enquanto o f32 de 3072 bytes
spilla) **NÃO se materializou** — nem sob pressão a 1.3 GB (onde o índice f32 excede a RAM).

## Por que o ganho não materializou (mecanismo — validado por matemática)

O gargalo NÃO é o scoring: o AH single-code é ~100× mais barato que o cosine f32 (3.8 µs/candidato no f32 SIMD vs
dezenas de ns no LUT). Se o scoring dominasse, o AQ seria ~100× mais rápido. Está em paridade → o gargalo é o
**page-read / working set** do walk. E a causa-raiz é o **LAYOUT**:

1. **O layout v3 co-localiza o código AQ com o vetor f32 no mesmo element tuple:**
   `element tuple = [header][f32: dim×4 = 3072 B][código AQ: ⌈m/2⌉ = 4 B]`. Para ler o código de 4 B de um nó, o
   walk **pagina o tuple inteiro de ~3 KB**. Logo o working set quente do AQ é o MESMO do f32:

   | | working set quente (acesso aleatório do walk) @500k×768 |
   |---|---|
   | f32 index | 500k × 3072 B ≈ **1.5 GB** |
   | **AQ v3 (hoje)** | 500k × 3076 B ≈ **1.5 GB** — código JUNTO do f32 |

   Os "códigos 768× menores" são **irrelevantes ao I/O** porque estão guardados *ao lado* do f32, não *no lugar*
   dele. Sob pressão de RAM o AQ thrasha idêntico → **paridade**. A conta bate exatamente com a medição.
2. **O que ScaNN/FAISS fazem (e o v3 não fez):** guardam **apenas os códigos** numa estrutura compacta e contígua
   no hot-path; os f32 (rerank) ficam **separados**, tocados só no reordenamento final. Working set quente = `m
   bytes/vetor`, não `dim×4` → cacheável sob pressão. **Esta é a peça que falta — de LAYOUT, não de carrier.**
3. **Secundário — batching SIMD:** o kernel AH batched (`ah_score_block`, 4.75× no micro-bench
   `theodb_rs/src/vec/ah_tests.rs::ah_simd_per_candidate_speedup`) precisa de códigos **contíguos** para o `pshufb`;
   a caminhada HNSW é nó-a-nó, então hoje só o `ah_score` single-code roda. Mas o scoring já não é o gargalo — o
   batching só importa DEPOIS de separar o layout (ganho de segunda ordem).

## Veredito D3

- **NÃO fecha o gap ~25× vs ScaNN no layout v3.** Honest-negative — o AQ é recall-competitivo (0.958 vs f32 0.974,
  um hair abaixo pela quantização coarse pq_subspaces=8) e QPS-paridade a recall casado, não ≥2×.
- **O eixo anisotrópico + AH está implementado e correto** (a fundação testada, 175 pg_tests). A causa-raiz da
  paridade é **de layout**: o código está co-localizado com o f32, então o working set quente nunca encolheu.
- **Próximo passo (o fix real): layout v4 — código separado do f32** (element tuple = só o código; f32 numa região
  rerank-only). Conta @500k: hot ~50 MB (códigos 2 MB + grafo 48 MB) vs 1.5 GB → cache-resident sob pressão → o
  ganho materializa. **Ressalva:** o grafo TAMBÉM precisa ficar separado/compacto (teste de mesa verifica quais
  bytes o `score_candidate` toca). Medir a **2M** (f32 ≈ 6 GB ≫ RAM), não só `--memory=800m`.
- Decisão registrada em `docs/adr/0019-m59-ah-needs-code-vector-separation.md`.

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
