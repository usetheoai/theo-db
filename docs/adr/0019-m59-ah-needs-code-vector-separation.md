# ADR 0019 — Anisotropic-PQ + AH implementado, mas o ganho exige SEPARAR códigos dos vetores f32 no layout (veredito D3 do M59)

**Status:** Accepted · **Date:** 2026-07-08 · **Milestone:** M59 · **Gate:** D3 · **Deciders:** CTO (paulohenriquevn)
**Relacionado:** ADR `0018` (M57 — SBQ não é superior; este ADR é o próximo passo algorítmico), ADR `0015` (own-AM), ADR `0002` (North Star / measurement-first)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m59-anisotropic-ah-blueprint.md`
**Evidência:** `docs/benchmarks/m59-anisotropic-ah.md` + `docs/benchmarks/m59-raw/*.json`

## Contexto e problema

O gap de ~25× QPS vs ScaNN (`docs/benchmarks/m33-scann-headtohead.md`) é o eixo P1 do North Star vetorial. O
M57 (ADR-0018) mediu que o SBQ (bit-quantization) NÃO fecha esse gap (fator-constante). O blueprint do M59
identificou o eixo algorítmico real: **quantização anisotrópica (ScaNN score-aware loss) + Asymmetric Hashing
com LUT16 SIMD (`pshufb`)**. O M59 implementou e mediu.

## Decisão

**Reconhecer, por medição + análise, que o eixo anisotrópico-PQ+AH está corretamente implementado, mas o layout
de persistência v3 CO-LOCALIZA o código AQ com o vetor f32 no mesmo element tuple — então o working set quente do
walk HNSW NÃO encolheu, e a paridade com o f32 é consequência disso.** O gap ~25× NÃO é fechado pelo M59 v3
(honest-negative), e a correção primária é de **layout** (separar códigos dos vetores), não a troca de carrier.

## Evidência (500k×768d cosine, box limpa, `theodb:m59fix`)

Paridade AQ vs f32 a recall casado ≥0.957, in-RAM (1.01×) e sob pressão de RAM 1.3 GB (1.03×) — não ≥2×.
A 20k in-RAM o AQ mede 1.16× (dentro do ruído de 1 run/20q, mas consistente com o lever: a 20k o índice inteiro
cabe em cache por acidente de escala — o AQ leva a melhor onde nada thrasha).

## Por que (mecanismo — validado por matemática, não hand-waving)

O gargalo NÃO é o scoring (o AH single-code é ~100× mais barato que o cosine f32 — 3.8 µs/candidato no f32 SIMD
vs dezenas de ns no LUT). O gargalo é o **page-read / working set** do walk. E a causa-raiz é o layout:

1. **O layout v3 co-localiza o código AQ com o f32 no mesmo element tuple:**
   `element tuple = [header][vetor f32: dim×4 = 3072 B][código AQ: ⌈m/2⌉ = 4 B]`.
   Para ler o código de 4 B de um nó, o walk **pagina o tuple inteiro de ~3 KB**. Logo:

   | | working set quente (acesso aleatório do walk) @500k×768 |
   |---|---|
   | f32 index | 500k × 3072 B ≈ **1.5 GB** |
   | **AQ v3 (hoje)** | 500k × 3076 B ≈ **1.5 GB** — código JUNTO do f32 |

   O índice AQ tem o **mesmo tamanho** do f32. Os "códigos 768× menores" são irrelevantes ao I/O porque estão
   guardados *ao lado* do f32, não *no lugar* dele. Sob pressão de RAM o AQ thrasha idêntico ao f32 → paridade.
   A conta bate exatamente com a medição.

2. **O que ScaNN/FAISS fazem (e o M59 v3 não fez):** guardam **apenas os códigos** numa estrutura compacta e
   contígua no hot-path; os vetores f32 (para rerank) ficam numa região **separada**, tocada só no reordenamento
   final do top-k. O working set quente vira `m bytes/vetor`, não `dim×4` → cacheável sob pressão.

3. **Secundário — batching SIMD:** o kernel AH batched (`ah_score_block`, medido a 4.75× no micro-bench
   `vec/ah_tests.rs`) precisa de códigos **contíguos** para o `pshufb`; a caminhada HNSW é nó-a-nó (pointer-chasing),
   então hoje só o `ah_score` single-code roda. Isso limita o *throughput de scoring*, mas o scoring já não é o
   gargalo — o page-read é. Portanto o batching é um ganho **secundário** que só importa DEPOIS de separar o layout.

## Consequências

- **North Star P1 (gap 25×): NÃO cumprido pelo M59 v3.** Segue aberto — mas o eixo algorítmico está resolvido; o
  que falta é o **layout de código separado**.
- **Próximo passo (M59-completar OU M61-class, motivado por medição): layout v4 — código separado do f32.**
  `element tuple = só o código AQ` (compacto) + região "raw f32" **separada**, acessada só no rerank do top-`k·over_fetch`. A conta @500k×768:

  | Estrutura | tamanho | acesso |
  |---|---|---|
  | Array de códigos AQ | 500k × 4 B ≈ **2 MB** | quente (todo o walk) → cabe em cache |
  | Listas de vizinhos (grafo) | 500k × 16 × 6 B ≈ **48 MB** | quente → cabe em RAM sob pressão |
  | Raw f32 (rerank only) | ≈ **1.5 GB** | frio — só ~`k·over_fetch` nós/query |

  Working set quente **~50 MB** (cabe) vs **1.5 GB** (não cabe sob pressão). O walk vira cache-resident e só o
  rerank toca o disco → aí o ganho materializa. **Ressalva de projeto (validar por teste de mesa):** o grafo TAMBÉM
  precisa ficar separado/compacto — se qualquer metadata do HNSW puxar o tuple f32 por acidente durante o
  `score_candidate`, o ganho some. O teste de mesa DEVE verificar exatamente quais bytes são tocados por candidato.
- **Complementar (não primário):** o kernel AH batched (`ah_score_block`) e/ou um carrier IVF de batch-scan
  aceleram o scoring DEPOIS que o layout separa o hot-path — ganho de segunda ordem, não a causa-raiz.
- **A fundação M59 fica no código** (não removida): codebook anisotrópico (`aq.rs`), kernel AH SIMD (`vec/ah.rs`),
  persistência v3 (`hnsw_page.rs`), reloption + scan wiring — tudo testado (175 pg_tests), backward-compat v1/v2
  intacta, opt-in. É a base direta do v4 (só o layout de armazenamento muda).
- **Benchmark sob pressão real:** o teste a `--memory=1.3g`/1.5 GB foi gentil demais (o p50 mal mudou in-RAM→pressão).
  O v4 deve ser medido a **2M** (f32 ≈ 6 GB ≫ RAM) para provar a tese estrutural, não só a `--memory=800m`.
- **Sem claim de "superioridade vetorial"** (Regra 5): o artefato honest-negative é o único claim.

## Opções consideradas

1. **Declarar o M59 fechando o gap** — rejeitada: a medição mostra paridade (1.01–1.03×), não ≥2×. Claim falso (Regra 5).
2. **Atribuir a paridade a "falta de carrier IVF batch-scan"** — rejeitada por análise: o batching é secundário; a
   causa-raiz medida é a co-localização código/f32 (o working set não encolheu). Mesmo com IVF, código junto do f32
   thrasharia igual. Registrar a causa errada seria impreciso (a review de benchmark exigiu precisão).
3. **Layout v4 — separar código do f32 (esta decisão)** — a correção primária, validada por matemática (~50 MB
   hot vs 1.5 GB), implementável no próprio carrier HNSW. É onde o AQ deve de fato vencer sob pressão.

## Caveats

Dados gaussian-mixture sintéticos (não SIFT1M) — a direção é estrutural (working set), não dependente do dataset;
follow-up SIFT1M. Teto de recall 0.958/0.974 < 0.99 é o gap de grafo (M60, ortogonal — comparação a recall casado).
`pq_subspaces=8` coarse; mais subespaços não mudam o veredito de layout.
