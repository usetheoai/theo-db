# Quantização vetorial (compressão dos índices ANN)

> **Status:** ✅ **Entregue (M22 + M51 + M59 + M83 + M85 + M86 + E1/E2).** A quantização é configurada por
> **reloptions próprios** compartilhados pelos três access methods own-code `theodb_ivfflat` / `theodb_hnsw` /
> `theodb_symqg`. Todas as opções são registradas em **um único `relopt_kind`**
> (`theodb_rs/src/am/options.rs:104`) e ligadas ao `amoptions` de cada AM
> (`theodb_rs/src/am/mod.rs:160`) — cada AM lê só as opções que implementa. Kernels own-code (std-only, sem
> dependências novas, algoritmos permissivos reimplementados): SBQ (`theodb_rs/src/sbq.rs`), AQ anisotrópico
> (`theodb_rs/src/vec/aq.rs`), Asymmetric-Hashing LUT16 / FastScan (`theodb_rs/src/vec/ah.rs`), RaBitQ f32-free
> (`theodb_rs/src/vec/rabitq.rs`).

> **Benchmarks (evidência medida):** paridade de recall + memória billion-scale, **nunca** "mais rápido que o
> ScaNN". RaBitQ f32-free 3.28× menor a paridade de recall em SIFT1M — [`docs/benchmarks/e1-rabitq-inpg-verdict.md`](../benchmarks/e1-rabitq-inpg-verdict.md)
> (spike bruto em [`docs/benchmarks/rabitq-spike/`](../benchmarks/rabitq-spike/)). SBQ paridade vs pgvectorscale —
> [`docs/benchmarks/m22-sbq-parity.md`](../benchmarks/m22-sbq-parity.md). SQ8-refine 3.5× menor —
> [`docs/benchmarks/m85-sq8-refine.md`](../benchmarks/m85-sq8-refine.md). SOAR spill —
> [`docs/benchmarks/m86-soar-spill.json`](../benchmarks/m86-soar-spill.json). Veredito honesto do North Star
> (M73/M74, `docs/adr/0035`) — [`docs/benchmarks/m73-headtohead-verdict.md`](../benchmarks/m73-headtohead-verdict.md).

Esta página ensina a **reduzir memória e (em cenários out-of-RAM) acelerar** os índices vetoriais do TheoDB via
`WITH (...)` no `CREATE INDEX`. Cada técnica troca recall × memória × velocidade de forma diferente; todos os
exemplos usam **apenas reloptions e GUCs verificados no código** (com `file:line`), então cada `CREATE INDEX`
parseia contra a superfície real.

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` (own-code), que provê o tipo `vector`, os AMs `theodb_ivfflat` / `theodb_hnsw` /
`theodb_symqg` e todos os reloptions de quantização. O `pgvector` foi removido no M70.

---

# 2. Reloptions de quantização — referência verificada

Todas as opções abaixo são **compartilhadas** pelos três AMs (um único `relopt_kind`, `options.rs:104`). Cada AM
consome só as que implementa; uma opção irrelevante para um AM é ignorada, não é erro. Ranges vêm direto de
`theodb_rs/src/am/options.rs`:

| Reloption | Tipo | Default | Range | Semântica | `file:line` |
|---|---|---|---|---|---|
| `lists` | int | `100` | `1 … 32768` | Nº de listas k-means do `theodb_ivfflat` (parâmetro de build). | `options.rs:15-17` · reg. `112-120` |
| `sbq_bits` | int | `0` (off) | `0 … 8` | Bits/dim dos códigos SBQ inline do `theodb_hnsw` (0 = f32-only). | `options.rs:22-24` · reg. `121-129` |
| `pq_subspaces` | int | `0` (off) | `0 … 2048` | Nº de subespaços `m` do AQ anisotrópico (`>0` liga AQ; exige `dim % m == 0`). | `options.rs:30-32` · reg. `130-139` |
| `pq_bits` | int | `4` | `4 … 4` | Bits por subquantizador — **só 4** (sweet-spot LUT16 `pshufb`). | `options.rs:36-38` · reg. `140-149` |
| `aq_threshold` | int | `1000` (η=1.0) | `1000 … 1000000` | Razão anisotrópica `η × 1000` (1000 = isotrópico). | `options.rs:43-45` · reg. `150-158` |
| `separate_storage` | int | `0` | `0 … 1` | 1 = layout v5 storage-separado (códigos e f32 em páginas distintas). | `options.rs:51-53` · reg. `159-168` |
| `refine` | int | `0` (f32) | `0 … 2` | Tier de rerank Stage-2: `0`=f32 (v5), `1`=SQ8 (v6), `2`=RaBitQ f32-free (v8). | `options.rs:59-61` · reg. `169-177` |
| `soar_lambda` | int | `0` (off) | `0 … 5000` | Peso de ortogonalidade do spill SOAR `λ × 1000` (`>0` derrama cada vetor p/ 2ª lista). | `options.rs:81-83` · reg. `178-187` |
| `rabitq_bits` | int | `7` | `1 … 8` | Bits/dim dos códigos RaBitQ do rerank v8 (só faz sentido com `refine=2`). | `options.rs:66-68` · reg. `188-196` |
| `degree_bound` | int | `32` | `32 … 512` | Out-degree do grafo co-locado `theodb_symqg` (arredonda p/ múltiplo de 32). | `options.rs:73-75` · reg. `197-205` |

Opclasses por AM (`mod.rs:338-380`): `theodb_ivfflat_{l2,cosine,ip}_ops`, `theodb_hnsw_{l2,cosine,ip}_ops`,
`theodb_symqg_l2_ops` (**symqg é L2-only** — o estimador de sinal 1-bit é L2-only, `mod.rs:355-359`). `l2` é a
opclass DEFAULT de cada AM.

---

# 3. SBQ — quantização scalar binária (`theodb_hnsw`)

```sql
CREATE INDEX products_hnsw_sbq
ON products
USING theodb_hnsw (
    description_embedding theodb_hnsw_cosine_ops
)
WITH (
    sbq_bits = 1
);
```

SBQ empacota `sbq_bits` bits por dimensão (limiar por média, `theodb_rs/src/sbq.rs`). A busca ranqueia candidatos
por **distância de Hamming** sobre os códigos e depois **reranqueia** o topo pelo kernel f32 exato — o padrão de
recuperação de recall do pgvectorscale. `sbq_bits = 1` dá a maior compressão (`ceil(dim·1/8)` bytes/vetor). A
memória é **paridade com o pgvectorscale nos mesmos bits** (medido em
[`docs/benchmarks/m22-sbq-parity.md`](../benchmarks/m22-sbq-parity.md), veredito `PARITY_REACHED`); o recall com
rerank é o diferencial.

---

# 4. SBQ — largar o pool de rerank com `over_fetch` (GUC de scan)

```sql
SET theodb_hnsw.over_fetch = 8;

SELECT product_id, name
FROM products
ORDER BY description_embedding <=> theodb.embed('running shoes', 'text-embedding-3-small')
LIMIT 10;
```

Num índice SBQ o ranking de Hamming é aproximado; `over_fetch = N` alarga o pool de candidatos ×N antes do rerank
f32 exato, para o verdadeiro vizinho sobreviver ao ranking aproximado (recuperação de recall). É um **GUC de
scan** (por sessão, sem rebuild), default `1`, range `1 … 64` — `theodb_rs/src/am/guc.rs` (`OVER_FETCH`). Sem
efeito em índice f32-only.

---

# 5. AQ + Asymmetric Hashing — quantização de produto anisotrópica (`theodb_ivfflat`)

```sql
CREATE INDEX products_ivf_aq
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100,
    pq_subspaces = 16   -- liga AQ; pq_bits default = 4
);
```

`pq_subspaces = m` (`> 0`) liga a **quantização de produto anisotrópica** (`theodb_rs/src/vec/aq.rs`), treinada no
build para minimizar a *anisotropic loss* do ScaNN (Guo et al. 2020). O scan pontua cada código via o kernel
**Asymmetric-Hashing LUT16** (`theodb_rs/src/vec/ah.rs`): uma tabela `LUT[m][16]` por query, depois `m` lookups
`pshufb` por candidato — sem multiply, sem decode. **Restrição de build:** `dim % pq_subspaces == 0` (erro tipado
no `CREATE INDEX` se violar). `pq_bits` só aceita `4` (`options.rs:36-38`).

---

# 6. AQ anisotrópico — ajustar `aq_threshold` (η)

```sql
CREATE INDEX products_ivf_aq_aniso
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100,
    pq_subspaces = 16,
    aq_threshold = 2000   -- η = 2.0 (peso paralelo 2× mais forte)
);
```

`aq_threshold` carrega o `η` (razão de peso paralelo/ortogonal) **milli-escalado** (`η × 1000`) num único int
reloption — `1000` = isotrópico (recupera k-means exato), valores maiores penalizam mais o erro paralelo à direção
do datapoint (a técnica central do ScaNN). Range `1000 … 1000000` (`options.rs:43-45`); é clampeado a `≥ 1.0` na
resolução.

---

# 7. Storage separado — códigos e f32 em páginas distintas (v5)

```sql
CREATE INDEX products_ivf_aq_sep
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100,
    pq_subspaces = 16,
    separate_storage = 1
);
```

`separate_storage = 1` grava o layout v5: os códigos AQ ficam em páginas separadas dos vetores f32, então o scan
lê **só os códigos compactos** para o prune Stage-1 e faz random-read do f32 apenas dos sobreviventes do rerank
Stage-2 (ADR-0037). Range int `0 … 1` (`options.rs:51-53`). É pré-requisito de `refine`.

---

# 8. Rerank SQ8 — encolher o tier de sobreviventes 4× (v6)

```sql
CREATE INDEX products_ivf_aq_sq8
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100,
    pq_subspaces = 16,
    separate_storage = 1,
    refine = 1            -- rerank em SQ8 (dim B/vec) em vez de f32 (dim·4 B/vec)
);
```

`refine = 1` reranqueia os sobreviventes em códigos **SQ8** (`dim` bytes/vetor) em vez de f32 cru (`dim·4`
bytes/vetor), encolhendo 4× o tier de rerank no frontier de alto recall. Só faz sentido com `separate_storage = 1`.
Medido **3.5× menor** com veredito honesto de QPS warm-cache parcial (o Stage-2 não é o gargalo in-RAM) —
[`docs/benchmarks/m85-sq8-refine.md`](../benchmarks/m85-sq8-refine.md). Range int `0 … 2` (`options.rs:59-61`).

---

# 9. Rerank RaBitQ f32-free — índice sem vetores crus (v8)

```sql
CREATE INDEX products_ivf_aq_rabitq
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100,
    pq_subspaces = 16,
    separate_storage = 1,
    refine = 2,           -- rerank em códigos RaBitQ residuais
    rabitq_bits = 7       -- sweet-spot f32-free ~0.99 recall
);
```

`refine = 2` reranqueia em **códigos RaBitQ residuais** (`theodb_rs/src/vec/rabitq.rs`) — **zero acesso a vetor
cru**, o lever direto de RAM billion-scale. `rabitq_bits` (default `7`, range `1 … 8`, `options.rs:66-68`) controla
a fidelidade; `7` é o ponto f32-free de ~0.99 recall do paper (arXiv:2409.09913). Medido em SIFT1M: **3.28× menor a
paridade de recall** (161 MB vs 528 MB), warm **sem speedup**, cold (out-of-RAM) **2.5–2.8× menor latência** —
[`docs/benchmarks/e1-rabitq-inpg-verdict.md`](../benchmarks/e1-rabitq-inpg-verdict.md).

---

# 10. SOAR spill — redundância de listas (recall a lists baixo)

```sql
CREATE INDEX products_ivf_aq_soar
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100,
    pq_subspaces = 16,
    soar_lambda = 1000    -- λ = 1.0 (recomendação do paper a 1M)
);
```

`soar_lambda` (`> 0`) derrama cada vetor para uma **segunda lista** com penalidade de ortogonalidade `λ`,
milli-escalada (`λ × 1000`) num int reloption — `0` = off (default, atribuição só primária, byte-idêntico). O paper
recomenda `λ=1.0` a 1M (`soar_lambda = 1000`) e `λ=1.5` a billion-scale. Range `0 … 5000` (`options.rs:81-83`).
Medido em SIFT1M — [`docs/benchmarks/m86-soar-spill.json`](../benchmarks/m86-soar-spill.json).

---

# 11. FastScan 1-bit — grafo co-locado `theodb_symqg`

```sql
CREATE INDEX products_symqg
ON products
USING theodb_symqg (
    description_embedding theodb_symqg_l2_ops   -- symqg é L2-only
)
WITH (
    degree_bound = 32
);
```

O `theodb_symqg` co-loca um grafo de navegação com códigos de **sinal 1-bit** pontuados pelo kernel FastScan
(`theodb_rs/src/vec/ah.rs`). `degree_bound` fixa o out-degree por vértice (múltiplo de 32 para alinhamento
FastScan; `32` = m0 da base-layer HNSW). Um valor não-múltiplo-de-32 é **arredondado para cima**. Range `32 … 512`
(`options.rs:73-75`). Só a opclass `theodb_symqg_l2_ops` existe (`mod.rs:355-359`). Veredito medido do spike E2 em
[`docs/benchmarks/e2-symqg-inpg-verdict.md`](../benchmarks/e2-symqg-inpg-verdict.md).

---

# 12. GUCs de scan — recall × velocidade por sessão

```sql
SET theodb_ivfflat.probes   = 32;   -- default 10, range 1..32768  (guc.rs)
SET theodb_hnsw.ef_search   = 100;  -- default 64, range 1..1000   (guc.rs)
SET theodb_hnsw.over_fetch  = 8;    -- default 1,  range 1..64      (guc.rs)
```

A quantização é um parâmetro de **build** (reloption); o recall × velocidade por query é um parâmetro de **scan**
(GUC, sem rebuild) — `theodb_rs/src/am/guc.rs`. `probes` (quantas listas IVF o scan lê), `ef_search` (largura do
beam HNSW) e `over_fetch` (largura do pool de rerank SBQ/AQ) são os três knobs de recall em runtime.

---

# 13. Fluxo completo recomendado (memória mínima billion-scale)

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE INDEX products_ivf_billion
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 500,
    pq_subspaces = 16,
    separate_storage = 1,
    refine = 2,
    rabitq_bits = 7,
    soar_lambda = 1000
);

SET theodb_ivfflat.probes  = 64;
SET theodb_hnsw.over_fetch = 32;

SELECT product_id, name,
       description_embedding <=> theodb.embed('wireless headphones', 'text-embedding-3-small') AS distance
FROM products
ORDER BY distance
LIMIT 10;
```

Combina AQ anisotrópico (prune Stage-1 barato) + storage separado + rerank RaBitQ f32-free (índice sem vetores
crus) + SOAR (recall a lists compactas). Perfil medido em SIFT1M:
[`docs/benchmarks/e1-rabitq-inpg-verdict.md`](../benchmarks/e1-rabitq-inpg-verdict.md).

---

# 14. Trade-off — recall × memória × velocidade

| Técnica | Reloption(s) | Memória | Recall | Velocidade | AM |
|---|---|---|---|---|---|
| Baseline f32 | (nenhuma) | 1× (`dim·4` B/vec) | máximo | referência | ivfflat / hnsw |
| SBQ scalar | `sbq_bits=1..8` | `ceil(dim·bits/8)` B/vec (paridade pgvectorscale) | recupera via rerank + `over_fetch` | Hamming rápido + rerank | hnsw |
| AQ + AH | `pq_subspaces=m, pq_bits=4` | `⌈m/2⌉` B/vec | paridade via AH + refine | LUT16 `pshufb` | ivfflat / hnsw |
| SQ8 refine (v6) | `+ separate_storage=1, refine=1` | tier rerank 4× menor (3.5× medido) | frontier alto recall | warm parcial (M85) | ivfflat |
| RaBitQ f32-free (v8) | `+ refine=2, rabitq_bits=7` | 3.28× menor (medido) | paridade (−1.1..1.6 pp) | warm sem ganho / cold 2.5–2.8× (E1) | ivfflat |
| SOAR spill | `+ soar_lambda=1000` | +1 lista por vetor | recall a lists baixo | build mais caro | ivfflat |
| FastScan 1-bit | `degree_bound=32..512` | 1-bit + grafo | veredito E2 | batched sign | symqg (L2-only) |

---

# 15. Veredito honesto — o que a quantização entrega (North Star)

> A superioridade de **QPS vetorial sobre o ScaNN/AlloyDB** foi **medida como NÃO-ALCANÇÁVEL** por uma extensão
> PostgreSQL permissiva: o gap ~25–44× @ recall 0.99 é de **paradigma** (AH-LUT anisotrópico do ScaNN + não pagar
> o imposto MVCC/WAL) — `docs/adr/0035` (M73) + `docs/adr/0036` (M74).

O que a quantização do TheoDB **entrega, medido**: (1) **paridade de recall** com o rerank exato/f32-free; (2) o
lever de **memória billion-scale** (RaBitQ 3.28× menor a paridade, SQ8 3.5× menor); (3) **latência cold (out-of-RAM)
2.5–2.8× menor** com RaBitQ. O que **não** entrega: ganho de QPS warm-cache (o Stage-2 de rerank não é o gargalo
in-RAM — reproduzido em M85 e E1). Posicionamento permitido: *"paridade de recall + memória billion-scale"*; **nunca**
*"mais rápido que o ScaNN/AlloyDB no vetor"* (`docs/adr/0035`, `../../.claude/rules/public-copy.md`).
