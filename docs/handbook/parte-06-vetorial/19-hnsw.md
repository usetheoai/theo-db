# Capítulo 19 — HNSW: grafos navegáveis de pequeno mundo hierárquicos

> **Capítulo-farol.** Este é o template de qualidade de todo capítulo ORIGINAL do manual: vai da teoria ao paper,
> à matemática, à **nossa implementação real** (`arquivo:linha`), ao **nosso benchmark medido** e ao **gap
> honesto** contra o estado da arte. Se um capítulo do coração não alcança este nível de aterrissagem, ele não
> está pronto.

**Pré-requisitos:** Cap. 3 (grafos, heap, skip lists), Cap. 17 (distâncias L2/cosine/IP e a paridade f32),
Cap. 14 (a Index Access Method API), Cap. 15 (persistência page-native & WAL).

---

## 19.1 — TEORIA: de skip lists a grafos de pequeno mundo hierárquicos

O problema do vizinho mais próximo aproximado (ANN) é: dado 1 milhão de vetores de 128 dimensões e uma consulta
`q`, achar os `k` mais próximos **sem** comparar `q` com todos (busca exata = O(N·d) por consulta, inviável a
escala). HNSW é hoje um dos algoritmos ANN mais usados no mundo (pgvector, Qdrant, Weaviate, Lucene, Redis) por um
motivo: **recall alto (>0.95) com busca em ~O(log N)**, tudo em memória, sem treinar quantizadores.

A ideia nasce de três blocos:

1. **Small-world networks (Watts–Strogatz, 1998).** Grafos onde a distância média entre dois nós cresce como
   `log(N)` mesmo com poucas arestas por nó — o "seis graus de separação". Se você constrói um grafo de
   proximidade onde cada vetor aponta para seus vizinhos mais próximos **e** tem algumas arestas "longas", uma
   busca gulosa (sempre ande para o vizinho mais próximo da consulta) chega perto do alvo em poucos saltos.

2. **NSW — Navigable Small World (Malkov et al., 2014).** Aplica isso a ANN: insere os vetores um a um, conectando
   cada novo nó aos seus `M` vizinhos mais próximos já no grafo. As primeiras inserções viram naturalmente as
   arestas "longas" (o grafo ainda é esparso), dando navegabilidade. Problema: a busca gulosa pode ficar presa em
   mínimos locais, e o grau de entrada dos primeiros nós explode.

3. **HNSW — Hierarchical NSW (Malkov & Yashunin, 2016/2018).** A contribuição decisiva: **camadas**, exatamente
   como uma *skip list* (Cap. 3). A camada 0 (base) tem todos os nós; cada camada acima tem exponencialmente
   menos nós. A busca começa no topo (poucos nós, saltos longos, aproximação grosseira), desce camada por camada
   refinando, e faz a busca fina só na base. Isso remove os mínimos locais do NSW e dá o `O(log N)` — o topo é o
   "expresso", a base é o "trem-bala local".

> **Paper seminal:** Yu. A. Malkov, D. A. Yashunin, *Efficient and Robust Approximate Nearest Neighbor Search
> Using Hierarchical Navigable Small World Graphs*, IEEE TPAMI 2018 (arXiv:1603.09320). Leitura obrigatória — as
> Figuras 1 e 2 do paper explicam a estrutura em camadas melhor que qualquer prosa.

**Intuição visual (skip list ↔ HNSW):**

```
Camada 2 (topo):   A ─────────────── E              ← poucos nós, saltos longos (aproximação grosseira)
                   │                 │
Camada 1:          A ───── C ─────── E ───── G       ← mais nós
                   │       │         │       │
Camada 0 (base):   A─B─C─D─E─F─G─H─I─J─K─L─M─N─O      ← TODOS os nós (busca fina)
```

Uma consulta entra por `A` (ponto de entrada, no topo), desce até se aproximar do alvo, e só na base explora `ef`
candidatos para escolher os `k` melhores.

---

## 19.2 — MATEMÁTICA: atribuição de camadas, busca gulosa, complexidade

### Atribuição de camada (a distribuição exponencial)

Cada nó recebe uma camada máxima `l` sorteada de uma distribuição geométrica/exponencial:

$$ l = \lfloor -\ln(U) \cdot m_L \rfloor, \quad U \sim \text{Uniforme}(0,1], \quad m_L = \frac{1}{\ln(M)} $$

O fator `m_L = 1/ln(M)` é a escolha ótima do paper (§4.1): faz o número esperado de camadas ser `~log_M(N)` e o
grau médio ficar limitado. Com `M=16` e `N=10⁶`: `l_max ≈ ln(10⁶)/ln(16) ≈ 13.8/2.77 ≈ 5`. Ou seja, um grafo de
1 milhão de vetores tem **~5–6 camadas** — o "expresso" tem pouquíssimos nós.

A probabilidade de um nó chegar à camada `l` cai geometricamente: `P(nível ≥ l) = M^{-l}`. Por isso a camada 0 tem
`N` nós, a camada 1 tem `~N/M`, a camada 2 tem `~N/M²`, etc.

### Parâmetros

| Símbolo | Papel | No TheoDB |
|---|---|---|
| `M` | arestas por nó nas camadas superiores | `HNSW_M = 16` |
| `M₀` = `2M` | arestas na camada 0 (base tem o dobro) | `m0 = m*2 = 32` |
| `ef_construction` | tamanho da lista de candidatos **no build** | `HNSW_EF_CONSTRUCTION = 64` |
| `ef_search` | tamanho da lista de candidatos **na query** | GUC `theodb_hnsw.ef_search`, default 64 |

`M` e `ef_construction` trocam qualidade-do-grafo por tempo-de-build; `ef_search` troca recall por velocidade **na
consulta** (sem rebuild) — é o botão que o usuário gira.

### Busca gulosa por camada (`SEARCH-LAYER`)

Na base, HNSW mantém dois heaps: `C` (candidatos a expandir, min-heap por distância) e `W` (os `ef` melhores
achados, max-heap). Expande sempre o candidato mais próximo de `q`; para cada vizinho não visitado, calcula a
distância e o insere em `W` se melhorar. Para quando o candidato mais próximo em `C` é pior que o pior de `W`.

### Complexidade

- **Busca:** `O(log N)` saltos entre camadas × `O(ef · M)` distâncias na base ≈ **`O(ef · M · d)`**, independente
  de `N`. Essa independência de `N` é a propriedade que perseguimos no §19.4 (o benchmark *flat-in-N*).
- **Build:** `N` inserções, cada uma fazendo uma busca `SEARCH-LAYER` com `ef_construction` ≈ **`O(N · log N ·
  ef_construction · M · d)`**. É caro — voltamos a isso no §19.4 (build de 17,5 min a 1M, single-thread).
- **Memória:** `O(N · (d + M))` — os vetores (`N·d·4` bytes em f32) mais as listas de vizinhos (`N·M` ponteiros).

---

## 19.3 — NOSSA IMPLEMENTAÇÃO

O TheoDB implementa HNSW em **duas camadas** com responsabilidades separadas (SRP):

1. **O grafo em memória** — `theodb_rs/src/ann/hnsw.rs` (383 LoC): a estrutura de dados pura, algoritmo de
   Malkov & Yashunin em Rust, sem PostgreSQL. Testável isoladamente.
2. **A persistência page-native** — `theodb_rs/src/am/hnsw_page.rs` (634 LoC): serializa o grafo em páginas do
   PostgreSQL e o percorre **on-demand** durante o scan (milestone **M35**).

### 19.3.1 — O grafo em memória (`ann/hnsw.rs`)

A estrutura espelha a matemática do §19.2 (`hnsw.rs:7`):

```rust
pub(crate) struct HnswIndex {
    metric: Metric,                    // L2 / cosine / inner-product (cap. 17)
    m: usize, m0: usize,               // M e M0 = 2M
    ef_construction: usize,
    vectors: Vec<Vec<f32>>,            // os vetores, indexados por nó
    ids: Vec<i64>,                     // o heap-TID de cada nó
    levels: Vec<usize>,                // a camada máxima de cada nó
    neighbors: Vec<Vec<Vec<usize>>>,   // neighbors[nó][camada] = índices de nós vizinhos
    entry: Option<usize>,              // o ponto de entrada (nó da camada mais alta)
    max_level: usize,
}
```

**Atribuição de camada** (`hnsw.rs:31` e `hnsw.rs:49`) — a fórmula do §19.2, com um detalhe de robustez nosso:

```rust
let ml = 1.0 / (m.max(2) as f64).ln();                     // m_L = 1/ln(M)
// ...
let level = ((-(rng.next_f64().ln()) * ml) as usize).min(HNSW_MAX_LEVEL);
```

O `.min(HNSW_MAX_LEVEL)` (`HNSW_MAX_LEVEL = 32`, `hnsw.rs:343`) **não existe no paper** — é uma decisão de
engenharia nossa, forçada pela persistência do §19.3.2: a tupla de vizinhos de um nó precisa caber em **uma página
de 8 KB**, e um nível astronomicamente alto (probabilidade ~0 em dados reais: o máximo real a 1M é ~5) estouraria
isso. pgvector faz o mesmo via `HnswGetMaxLevel`. É um exemplo do padrão do livro: *a teoria é limpa; a
implementação carrega decisões que a teoria não vê.*

O RNG é um SplitMix64 determinístico e seeded (`ann/mod.rs`, ADR de reprodutibilidade) — **não** criptográfico,
mas garante que dois builds do mesmo corpus produzam o mesmo grafo (essencial para o teste de round-trip do
§19.3.2 e para benchmarks reproduzíveis).

**Inserção** (`hnsw.rs:55` `insert`) segue o Algoritmo 1 do paper: desce do topo até `level+1` com busca gulosa de
1 candidato (`greedy_descend`, `hnsw.rs:106`), depois de `level` até 0 faz `SEARCH-LAYER` com `ef_construction`,
seleciona `M` vizinhos e cria arestas bidirecionais — podando o grau do vizinho se ele exceder `M`.

**Seleção de vizinhos** (`hnsw.rs:171` `select_from`) implementa a *heurística* do paper (Algoritmo 4), a mesma
que o pgvector usa: mantém o candidato `e` só se ele for mais perto da consulta do que de qualquer vizinho já
escolhido. Isso evita clusters redundantes e dá arestas diversas (melhor navegabilidade que "os M mais
próximos"). Se a heurística deixar o grau abaixo de `M`, completa com os mais próximos restantes.

**Busca** (`hnsw.rs:200` `search`) é o Algoritmo 5: greedy nas camadas superiores, depois `SEARCH-LAYER`
(`hnsw.rs:128`) com `ef_search` na base, e trunca em `k`.

> **Decisão registrada:** implementamos HNSW próprio (não usamos o do pgvector) porque o TheoDB precisa do grafo
> como estrutura Rust que persistimos e percorremos com o nosso layout — ver blueprint
> `m21-own-ann-index-blueprint.md` e ADR `0010-m26-index-am-scope.md`. Coexistimos com o pgvector (lemos os
> valores dele, nunca o índice dele) — ADR `0001-no-engine-fork.md`.

### 19.3.2 — A persistência page-native (`am/hnsw_page.rs`, milestone M35)

O grafo em memória é lindo, mas o PostgreSQL armazena índices em **páginas de 8 KB no disco**. A pergunta de
engenharia do M35 foi: *como percorrer o grafo lendo só as páginas dos nós que a busca visita, em vez de carregar
o grafo inteiro por consulta?*

A versão ingênua (que tínhamos até M34) serializava o grafo num **blob único** e a cada query
desserializava-o **por completo** — O(N) por consulta (~6,5 GB a 1M). O M35 substituiu isso pelo layout
page-native, espelhando o pgvector (`hnsw.h`: `HnswElementTupleData` / `HnswNeighborTupleData`):

```
bloco 0        = meta: parâmetros + ponto de entrada (blkno, offno, level) + limites das faixas
blocos [1..]   = element tuples (tamanho fixo): [tag, level, tid, ponteiro-p/-vizinhos, dim, vetor f32]
blocos [nbr..] = neighbor tuples (um por nó): todos os vizinhos de todas as camadas, como ponteiros (blkno,offno)
```

Cada nó do §19.3.1 vira uma **element tuple** (o vetor + o TID do heap + um ponteiro para sua neighbor tuple) e
uma **neighbor tuple** (as arestas). O ponteiro interno é um par `(BlockNumber, OffsetNumber)` — o endereço de uma
tupla numa página (Cap. 15). A magia `"THSS"` (`hnsw_page.rs:20`) distingue o formato estruturado do blob legado.

**O empacotamento** (`hnsw_page.rs:253` `pack`) é uma decisão de design nossa (ADR-2 do blueprint M35): como o
grafo inteiro está em memória no build, **todos os endereços são calculáveis antes de qualquer I/O**. Element
tuples têm tamanho fixo → o endereço do nó `i` é analítico (`bloco 1 + i/por_página`, `offset 1 + i%por_página`).
As neighbor tuples (tamanho variável por nível) são empacotadas por um packer determinístico e puro. Resultado:
uma única passada de escrita WAL-logada, **sem tupla placeholder e sem `PageIndexTupleOverwrite`** (que o pgvector
precisa porque constrói em disco). Menos superfície de FFI, e o packer é testável sem um PostgreSQL rodando.

**A travessia on-demand** (`hnsw_page.rs:472` `traverse`) é o coração do M35 — espelha o §19.2 mas lê páginas em
vez de percorrer ponteiros de memória:

```
1. lê a meta (1 página) → ponto de entrada
2. camadas superiores (ef=1): expande o candidato → lê sua neighbor tuple (1 página);
   cada vizinho não visitado → lê sua element tuple (1 página) + pontua com SIMD direto nos bytes
3. camada base (ef_search): idem, mantendo os ef melhores num heap; visited-set por (blkno,offno)
   garante que cada nó é lido no máximo uma vez
```

A pontuação usa `l2_dist_from_bytes` (`vec.rs:167`), que calcula a distância L2 **direto sobre os bytes da página**
com AVX2+FMA (Cap. 24) — zero alocação de `Vec<f32>` por nó (o padrão hot-path do M31b). O contrato de erro é
rígido (Cap. 8): toda decodificação retorna `Result`; uma página corrompida vira um erro tipado, **nunca um panic
atravessando a fronteira C** (`am/scan.rs:81` `scan_hnsw_structured` converte via `pg_sys::error!`).

**A decisão KISS que cortou metade do milestone** (ADR-1 do blueprint M35): o grafo é **imutável entre rebuilds**.
INSERT vai para uma região *pending* (append), DELETE dispara um rebuild no VACUUM. Como o grafo construído nunca
muda, **não precisamos** da maquinaria mais dura do pgvector — insert incremental on-disk, tombstones, detecção de
tupla stale por versão. É o princípio **Esforço ≠ Complexidade** do TheoDB: alto esforço no que o problema exige
(o codec + a travessia), zero complexidade acidental (a maquinaria de mutação que o nosso modelo pending/VACUUM
torna desnecessária).

**Onde isto se conecta ao PostgreSQL:** o scan despacha no formato via a magia da página
(`am/scan.rs:70`), lê o `ef_search` da GUC (`am/scan.rs:96`), percorre, e dobra a região *pending*. Tudo por trás
da Index Access Method API do Cap. 14 (`amgettuple` entrega um TID por vez ao executor). Um `SELECT ... ORDER BY
embedding <-> '[...]' LIMIT k` vira uma travessia de grafo page-native.

---

## 19.4 — NOSSO BENCHMARK (milestone M35)

Toda afirmação de performance aqui vem de `docs/benchmarks/m35-hnsw-structured-scan.{md,json}` — SIFT1M (1M×128,
Euclidiano), hardware Intel i7-1355U (CPU móvel, single-thread), 1000 queries seed 42, 3 runs. Reproduza com
`python3 benchmarks/run_m35_hnsw.py`.

**Frontier recall–QPS a 1M** (varredura de `ef_search`):

| `ef_search` | recall@10 | QPS | p50 |
|---|---|---|---|
| 40 | 0.9272 | 318.9 | 3.15 ms |
| 100 | 0.9789 | 100.4 | 10.06 ms |
| 200 | 0.9926 | 60.4 | 17.56 ms |

O botão `ef_search` faz exatamente o que a teoria (§19.2) prevê: mais candidatos → mais recall, menos QPS. Este é
o valor da GUC do §19.3.2 — o usuário escolhe o ponto de operação **sem rebuild**.

**A vitória O(N) → O(ef·M).** Antes do M35 (medido no M32), o `theodb_hnsw` blob fazia **1,6 QPS** (p50 607 ms) a
1M — a desserialização do grafo inteiro por query. No ponto de recall preservado (`ef_search=100`, recall 0.979 ≥
o recall 0.964 do blob), a travessia estruturada faz **100,4 QPS = ~61× mais rápido**, honestamente medido no
mesmo recall. (Se um recall de 0.93 for aceitável, `ef_search=40` sobe a ~319 QPS ≈ 194× — mas isso é um recall
*menor*, então o número honesto de headline é 61× em recall preservado, não 194×.)

**A prova de que a busca é O(ef·M), não O(N).** Wall-clock não prova complexidade — p50 cresce sub-linearmente
com N por *cache misses* num índice maior, não por mais trabalho de travessia. A métrica correta é **páginas
lidas** (`EXPLAIN (ANALYZE, BUFFERS)`):

| N | páginas lidas (ef_search=100) |
|---|---|
| 50.000 | 2742 |
| 200.000 | 2962 |

N cresceu **4×**, páginas lidas cresceram **1,08×** — essencialmente constante. Isso é `O(ef·M)`, plano em N: a
assinatura do partial-read. (Lição de benchmark, aprofundada no Cap. 23: *meça a métrica que prova a
complexidade, não a que o cache confunde.*)

**O trade-off honesto:** o build estruturado leva **~17,5 min a 1M** (construção de grafo single-thread — a
complexidade `O(N log N ef_construction M)` do §19.2 é cara). É o modelo *build-once / scan-many*. Paralelizar o
build é trabalho futuro. Não escondemos isso: está na coluna `build (ms)` do artefato e na prosa do CHANGELOG.

---

## 19.5 — SOTA & GAP HONESTO

Onde o `theodb_hnsw` está no mapa do estado da arte (todos os números a 1M, mesmo hardware):

**vs pgvector (par maduro em C, mesmo banco):** no head-to-head justo do M34 (isolado, single-thread, mesmos
parâmetros), o `theodb_ivfflat` está em **paridade** com o pgvector ivfflat (às vezes ligeiramente à frente no
ponto de recall alto). Para o HNSW especificamente ainda não temos um side-by-side isolado justo vs o pgvector
hnsw — é uma medição pendente (honestidade: não afirmo paridade de HNSW sem o número).

**vs ScaNN — o algoritmo do índice vetorial do AlloyDB (milestone M33):** aqui está o **gap real e quantificado**.
No ponto de recall ≥ 0.99, o ScaNN OSS faz **~1920 QPS / p50 0.5 ms** contra os 78 QPS / 12.8 ms do nosso melhor
índice — **~25× mais rápido**. Recall: **paridade** (ambos ≥ 0.99). O verdadeiro diferencial do ScaNN não é o
grafo — é a **quantização anisotrópica** (comprime os vetores preservando a ordenação por produto interno) +
*asymmetric hashing* com SIMD, que corta o custo por distância em uma ordem de grandeza. HNSW full-precision (o
nosso) paga a distância em f32 completo por candidato.

> **O que fecha esse gap** é o Cap. 20 (Quantização): SBQ/PQ dentro do índice, à la ScaNN. É a única peça restante
> do North Star de superioridade vetorial (ADR `0002-north-star-equal-or-superior-to-alloydb.md`), e a próxima
> aposta de maior impacto. HNSW nos deu partial-read a escala (M35); quantização dará o salto de QPS por
> candidato. Ver também o ADR `0004-scann-fork-decision.md` (a decisão condicional de fork do ScaNN, gatilhada por
> benchmark).

**DiskANN / Vamana (Cap. 21)** é a outra fronteira: um grafo único (não hierárquico) otimizado para SSD, que faz
bilhões de vetores em um nó com o grafo em disco. É o caminho quando o corpus não cabe em RAM — complementar, não
concorrente, ao HNSW em memória.

---

## 19.6 — Pontos-chave

1. HNSW = skip list aplicada a um grafo de proximidade → busca `O(log N)` com recall alto, tudo em memória, sem
   treinar quantizadores. Paper: Malkov & Yashunin 2018 (arXiv:1603.09320).
2. Dois parâmetros de qualidade no build (`M`, `ef_construction`) e um de recall-na-query (`ef_search`) que o
   usuário gira sem rebuild — no TheoDB, a GUC `theodb_hnsw.ef_search`.
3. Nossa implementação separa o **grafo em memória** (`ann/hnsw.rs`) da **persistência page-native**
   (`am/hnsw_page.rs`, M35), que percorre o grafo lendo só as páginas visitadas → **O(ef·M)**, plano em N.
4. Medido: **100 QPS @ recall 0.98 a 1M**, ~61× o blob O(N) anterior em recall preservado; páginas lidas planas em
   N (a prova de complexidade correta). Trade-off honesto: build de ~17,5 min single-thread.
5. Gap honesto: **~25× atrás do ScaNN** (o algoritmo do AlloyDB) no eixo ANN puro — recall em paridade, throughput
   não. A quantização (Cap. 20) é o que fecha isso.

## 19.7 — Exercícios

1. **(Leitura de código)** Rastreie uma query `ORDER BY embedding <-> q LIMIT 10` num índice `theodb_hnsw`, do
   `amrescan` (`am/scan.rs`) até `traverse` (`hnsw_page.rs:472`). Conte quantas leituras de página uma travessia
   com `ef_search=64` faz e compare com o §19.4.
2. **(Matemática)** Para `M=16`, `N=10⁷`, estime o número esperado de camadas e o número de nós na camada mais
   alta. Confirme com a fórmula `P(nível ≥ l) = M^{-l}`.
3. **(Experimento)** Rode `benchmarks/run_m35_hnsw.py` com uma varredura de `ef_search` mais fina ({20, 60, 120,
   250}). Plote a curva recall × QPS. Onde está o "joelho" da curva no nosso hardware?
4. **(Design)** O §19.3.2 diz que mantemos o grafo imutável entre rebuilds (ADR-1). Que classe de bug o pgvector
   precisa tratar (detecção de tupla stale por versão) que nós **não** precisamos — e por quê? Quando esse
   trade-off deixaria de valer a pena?
5. **(Fronteira)** Leia o §19.5 e o Cap. 20. Esboce como você adicionaria quantização SBQ às element tuples do
   §19.3.2 sem quebrar a travessia. Onde a pontuação (`l2_dist_from_bytes`) mudaria?

## Referências

- **Paper seminal:** Yu. A. Malkov, D. A. Yashunin, *Efficient and Robust Approximate Nearest Neighbor Search
  Using Hierarchical Navigable Small World Graphs*, IEEE TPAMI 2018 — arXiv:1603.09320.
- **NSW:** Malkov et al., *Approximate nearest neighbor algorithm based on navigable small world graphs*,
  Information Systems, 2014.
- **Small-world:** Watts & Strogatz, *Collective dynamics of 'small-world' networks*, Nature 1998.
- **Referência de implementação:** pgvector `src/hnsw.{h,c}`, `hnswscan.c`, `hnswbuild.c`
  (`.claude/knowledge-base/references/pgvector/`).
- **Nossos artefatos:** blueprint `m21-own-ann-index-blueprint.md`, blueprint
  `m35-hnsw-structured-scan-blueprint.md`, benchmark `docs/benchmarks/m35-hnsw-structured-scan.{md,json}`,
  benchmark `docs/benchmarks/m33-scann-headtohead.{md,json}` (o gap), ADRs `0010`, `0011`, `0001`, `0002`, `0004`.
- **Código:** `theodb_rs/src/ann/hnsw.rs`, `theodb_rs/src/am/hnsw_page.rs`, `theodb_rs/src/am/scan.rs`,
  `theodb_rs/src/vec.rs`.
- **Comparação SOTA (Cap. 21):** TigerData, *HNSW vs. DiskANN*; Microsoft, *DiskANN on Azure Database for
  PostgreSQL*.
