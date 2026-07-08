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

## Resultado 3 — layout v4 (código separado do f32) sob pressão FORTE (500k×768d, sb=128MB)

O v4 move o f32 do element tuple para uma região raw separada (byte-test prova: `ElementViewV4` não tem
`vec_bytes` → o f32 é estruturalmente inacessível ao walk). Conta de mesa: hot ~63 MB vs f32 1.5 GB. Medição:

| Regime | AQ v4 recall / QPS | f32 recall / QPS | AQ/f32 |
|---|---|---|---|
| in-RAM (16 GB, sb=128MB) | 0.958 / 70 | 0.974 / 71 | 0.99× |
| pressão FORTE (`--memory=700m`) | 0.958 / 2.3 | 0.974 / 2.1 | 1.1× |

**O v4 sozinho NÃO produziu o ganho — paridade, e sob 700m AMBOS colapsam para ~2 QPS (p50 ~450 ms).** Dois
diagnósticos da curva por-`over_fetch` sob pressão:
1. **O rerank é BARATO, não o gargalo:** o AQ atinge recall 0.958 já a `over_fetch=4` (a quantização AH é
   acurada), e o p50 é **flat** em over_fetch ∈ {4,8,16,32} (491→431 ms) — se os `k·over_fetch` reads de f32 frio
   dominassem, o p50 escalaria 8× entre over_fetch 4 e 32. Não escala → o rerank não domina.
2. **O WALK domina e o v4 não o acelerou sob pressão** (AQ 2.3 ≈ f32 2.1). Apesar do byte-test provar que o
   *código* do walk não decodifica o f32, o walk do AQ v4 não ficou mais rápido. **Hipótese mais provável
   (a ressalva de projeto do owner):** as páginas hot-element e raw-f32 estão **interleaved** no `Packed` v4 → ler
   uma página hot puxa a página raw adjacente para o cache → poluição → a separação é derrotada no nível de PÁGINA
   (o byte-test garante a separação no nível de CÓDIGO, mas não no nível de layout físico das páginas). Verificar/
   corrigir a ordenação das páginas (todas hot juntas, depois todas raw) é o próximo passo medido.

## Veredito D3

- **NÃO fecha o gap ~25× vs ScaNN no layout v3.** Honest-negative — o AQ é recall-competitivo (0.958 vs f32 0.974,
  um hair abaixo pela quantização coarse pq_subspaces=8) e QPS-paridade a recall casado, não ≥2×.
- **O eixo anisotrópico + AH está implementado e correto** (a fundação testada, 175 pg_tests). A causa-raiz da
  paridade é **de layout**: o código está co-localizado com o f32, então o working set quente nunca encolheu.
- **O layout v4 (código separado do f32) foi implementado e medido — e NÃO fechou o gap** (Resultado 3): paridade
  sob pressão forte (700m), ambos ~2 QPS. O byte-test prova a separação no nível de CÓDIGO, mas a paridade indica
  que a separação não se traduziu em ganho de I/O — hipótese medível: **interleaving das páginas hot/raw** no
  `Packed` (poluição de cache no nível de página). Este é o próximo passo concreto: garantir páginas hot contíguas
  (todas juntas) antes das raw, e re-medir; se ainda paridade, a conclusão é que o carrier HNSW pointer-chasing não
  materializa o ganho de quantização — o caminho seria o carrier IVF batch-scan (contíguo por design, como ScaNN).
- **Honest-negative rigoroso e medido:** o eixo anisotrópico-PQ+AH está implementado, correto (177 pg_tests) e o
  código está estruturalmente separado (v4), mas **em nenhuma config medida (v3 co-localizado, v4 separado, in-RAM,
  pressão 1.3g/700m) o AQ superou o f32 em QPS a recall casado.** A superioridade que o ScaNN reporta não se
  reproduziu no carrier HNSW do TheoDB. Isto é ciência measurement-first: a hipótese foi testada a fundo e não se
  sustentou; o próximo lever (page-ordering do v4, depois carrier IVF) fica registrado e motivado por medição.
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
