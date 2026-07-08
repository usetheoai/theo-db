# M57 (P0) — SBQ-inline superiority: recall×QPS a escala + pressão de RAM (veredito D3)

**Veredito: HONEST-NEGATIVE.** A tese "o AM próprio se justifica porque o SBQ inline entrega **≥2× QPS** a
recall≥0.99 sob pressão de memória" está **FALSIFICADA** por medição. O SBQ é recall-neutro vs f32 mas
**consistentemente MAIS LENTO** (nunca mais rápido) — in-RAM e sob pressão. Consequência: reabrir o ADR-0015
(ver `docs/adr/0018-m57-sbq-inline-not-superior.md`).

## Método

- **Harness:** `benchmarks/run_m51_sbq_inline.py` (recall gate + comparação a pgvector) e
  `benchmarks/run_m57_pressure.py` (split `--phase build|measure` para constranger RAM ENTRE build e medição —
  um build HNSW precisa de `maintenance_work_mem` que o estado constrangido não daria). Ambos reusam SPECS/_conn/
  _measure/_ground_truth (Regra 9). GT = seqscan exato cosine. Métrica: recall@10 + p50 + QPS 1-cliente.
- **Dados:** gaussian-**mixture** (256 centros, ruído tight) — o gaussian puro é degenerado para ANN (recall
  arbitrário; ver § Caveat 1). Cosine, dim=768 (eixo dos embeddings reais).
- **Ambiente:** droplet DigitalOcean c-8 (8 vCPU dedicado / 16 GB), box **limpa** (`load_per_run < 1.5` — sem
  saturação, lição m46). PG `theodb:m58` (com o cosine SIMD do M58), `shared_buffers=1GB`, `--network host`.
- **Pressão:** build a 16 GB → `docker update --memory=<N> pgm57` + `drop_caches` → measure. Índices a 500k×768d:
  f32 ~1.5 GB, códigos SBQ (8-bit) ~384 MB, tabela ~1.5 GB.
- **Configs SBQ:** `sbq_bits=8`, `ef_search=400`, `over_fetch ∈ {2,4,8,16}` (o knob de recuperação de recall, M40).

## Resultado 1 — recall gate + comparação a pgvector (100k×768d, in-RAM)

| Índice | recall@10 | p50 | QPS | build |
|---|---|---|---|---|
| theodb_hnsw_sbq | 0.974 | 1.18 ms | 848 | 67 s |
| theodb_hnsw_f32 | 0.974 | 1.07 ms | 938 | 63 s |
| pgvector_hnsw | 0.992 | 1.41 ms | 708 | 41 s |

- SBQ é **recall-neutro vs f32** (0.974 = 0.974) — o rerank exato f32 preserva recall (DoD do M51 vale a escala).
- theodb HNSW é **~1.2× QPS > pgvector** a recall equivalente — o AM tem valor geral, **mas não vindo do SBQ**.
- Já aqui o SBQ (848) é mais lento que o f32 (938): **nenhuma vantagem in-RAM**.

## Resultado 2 — SBQ vs f32 sob pressão de RAM (500k×768d, o núcleo P0)

recall idêntico 0.956 em todos os regimes (SBQ = f32 — recall-neutro; o 0.956 < 0.99 é a qualidade do grafo HNSW
do theodb a esta escala/ef, não do SBQ). QPS 1-cliente, melhor ponto do sweep:

| Regime (ef_search=400) | SBQ QPS | f32 QPS | **SBQ/f32** |
|---|---|---|---|
| in-RAM (16 GB, tudo cacheado) | 90 | 256 | **0.35×** |
| pressão (`--memory=1.8g`) | 194 | 266 | **0.73×** |
| pressão tight (`--memory=1.3g`, < shared_buffers+índice f32) | 218 | 284 | **0.77×** |
| in-RAM, ef_search=1000 (recall casado 0.974, o máximo do grafo) | 47.7 | 152 | **0.31×** |

**O f32 vence o SBQ em TODOS os regimes** (in-RAM, sob pressão, ef baixo/alto). A tese ≥2× exigiria SBQ ≥2× *mais
rápido*; medimos SBQ *0.31–0.77×* (mais lento). A ef alto o SBQ piora (mais nós visitados → mais Hamming+rerank).

## Por que a tese falhou (mecanismo)

1. **HNSW tem localidade de acesso.** Uma query toca ~`ef·log N` nós, não o índice inteiro. As páginas quentes
   (camadas superiores + nós visitados) permanecem cacheadas mesmo quando o índice f32 (1.5 GB) excede a RAM — logo
   o f32 **não thrasha** sob pressão (QPS f32 até *subiu* de 256→284 entre in-RAM e tight, dentro do ruído). A
   premissa "índice não cabe → I/O de disco por query" não vale para HNSW.
2. **O caminho de leitura do SBQ é fundamentalmente mais caro por query:** Hamming-walk sobre os códigos + rerank
   exato f32 no top `k·over_fetch`. Esse custo de CPU domina e **cresce relativamente com a escala** (100k: SBQ
   0.90× do f32; 500k: 0.35× in-RAM) — o oposto do que a tese previa.

O ganho do SBQ (códigos pequenos) só pagaria se o gargalo fosse I/O de índice sob pressão — e o HNSW, por design,
não expõe esse gargalo.

## Veredito D3

- **NÃO reter** a tese do AM próprio *pela superioridade do SBQ inline*. O SBQ é recall-neutro mas mais lento —
  não há caso de QPS que o justifique sobre o f32 HNSW simples.
- O AM próprio ainda tem **valor geral medido** (theodb HNSW ~1.2× > pgvector a 100k) — mas essa é uma tese
  DIFERENTE (qualidade do grafo/scan), não a do SBQ. Ver ADR-0018.
- Reabrir o **ADR-0015** (own-AM) reenquadrando a justificativa: não é o SBQ. `docs/adr/0018-...`.

## Caveats honestos

1. **Dados sintéticos gaussian-mixture**, não embeddings reais (SIFT1M/OpenAI). Estrutura de cluster é uma
   aproximação; um dataset real pode mover os absolutos. Mas a **direção** (SBQ mais lento que f32, sem thrash do
   f32) é mecânica (localidade do HNSW + custo do rerank), não dependente do dataset. Follow-up: repetir em SIFT1M.
2. **pgvector não buildou a 500k** — `/dev/shm=64MB` (default do docker) < 6.4 GB que o build paralelo do pgvector
   pede. Baseline pgvector só existe a 100k (Resultado 1). Não afeta o veredito (SBQ vs f32, ambos theodb).
3. **recall casado 0.974 < 0.99 a 500k — teto de qualidade do grafo do theodb, e o gate 0.99 NÃO é alcançável por
   tuning de build.** Testado: `ef_construction` 64→200 **DEGRADOU** o recall (0.974 → **0.832** a 500k, 3 runs;
   build ~20% mais lento confirmando que efc=200 foi aplicado) — o oposto do esperado. O theodb **JÁ aplica** a poda
   por diversidade estilo-pgvector (`select_from` em `ann/hnsw.rs:255` — mantém um candidato só se estiver mais perto
   da query que de qualquer vizinho já-mantido), então o teto de 0.974 e a degradação-por-efc **NÃO** são "falta do
   heurístico" (`m0=2*m=32` também está correto, = pgvector). **Causa-raiz ISOLADA: o build paralelo do HNSW**
   (`ann/hnsw_parallel.rs`, usado acima de 4096 nós) tem links **racy** (o comentário do módulo admite: "the build
   is NON-DETERMINISTIC — racy insert order"). O recall do grafo cai com a escala (5k=1.0 → 100k=0.974 → 500k=0.956)
   e, anomalamente, com `ef_construction` maior (0.974→0.832 a efc=200) — sinal de que o **pruning de vizinhos sob
   contenção de lock** (`select_from` chamado no linking concorrente) descarta arestas boas com base em leituras
   stale quando há mais candidatos. `M` está travado em 16 pelo layout de página (`hnsw.rs:428`). **DUAS tentativas
   de fix foram REFUTADAS por medição** (honestidade, Regra 3): (a) `ef_construction` 64→200 → recall 0.832 (pior);
   (b) o "minimal fix" MERGE (mesclar back-links in-flight em vez de sobrescrever `node[layer]`, que o próprio
   comentário do código previa) → recall **0.846** com recall **não-monotônico em ef_search** (sinal de grafo
   corrompido — manter o conjunto `selected` diversity-pruned é superior a mesclar back-links arbitrários). Ambas
   revertidas; a melhor config medida (OVERWRITE + efc=64, recall 0.974) foi mantida. **BISSECÇÃO DECISIVA
   (`THEODB_HNSW_PARALLEL_THRESHOLD`): o build SEQUENCIAL determinístico a 100k dá recall 0.96 — igual/pior que o
   paralelo (0.974).** Logo o teto **NÃO é contenção paralela** (o que refuta os fixes de linking, incl. o MERGE) e
   sim a **qualidade do ALGORITMO BASE do HNSW** do theodb (`search_layer`/`greedy_descend`/`select_from`),
   presente nos dois builds. theodb 0.96 vs pgvector 0.978 @100k (mesmo run) = gap algorítmico de ~1.8pt. **Cruzar
   0.99 exige melhorar o algoritmo base do HNSW — milestone próprio (M60-class), FORA do escopo do M57** (que mede o
   SBQ). **TRÊS tentativas de fix refutadas por medição** (efc→0.832, MERGE→0.846, m=32→0.952 — todas PIORES ou
   iguais, todas revertidas; `m=32` piorar é anômalo). **Lead forte:** o recall é plateau/não-monotônico em
   `ef_search` e NENHUMA mudança de build move o teto → o gargalo é provavelmente o **SCAN** (`am/hnsw_page.rs`
   traverse), não a conectividade do grafo — investigar o beam-search/heap do traverse primeiro no M60. O veredito
   SBQ é robusto ao recall casado medido (SBQ sempre < f32). Evidência: `m57-raw/m57p_efc200_r*` (efc),
   `m57_recallfix.json` (MERGE), `m57_seq100k.json` (bissecção), `m57_m32_100k.json` (m=32).
   pgvector a 500k (shm=8g): recall 0.936, ~289 qps — baseline (theodb f32 0.974 tem recall *maior* a 500k).
4. **1-cliente** (sem concorrência). Um QPS multi-cliente pode mudar absolutos, não a razão SBQ<f32 (mesmo custo
   por query relativo).

## Reprodução

```
# build (16 GB):   python3 run_m57_pressure.py --phase build --n 500000 --dim 768 --nq 50 --make --state S.json
# in-RAM:          python3 run_m57_pressure.py --phase measure --state S.json --mem-note unconstrained
# pressão:         docker update --memory=1300m --memory-swap=1300m pgm57 && sync && echo 3>/proc/sys/vm/drop_caches
#                  python3 run_m57_pressure.py --phase measure --state S.json --mem-note pressure_1.3g
```

Dados brutos: `docs/benchmarks/m57-raw/{m57_100k_clustered,m57p_unconstrained,m57p_pressure,m57p_pressure_tight}.json`.
