# M59 v4 — separação código/vetor: teste de mesa byte-a-byte antes de implementar

Design note (owner sign-off 2026-07-08): o layout v3 co-localiza o código AQ com o f32 → o working set quente
do walk não encolheu → paridade (ADR-0019). O v4 separa. Este doc é o **teste de mesa** que a review pediu:
traçar EXATAMENTE quais bytes o `score_candidate` toca, para garantir que o grafo não puxa o f32 por acidente.

## Layout atual (v3) — o que `score_candidate` toca hoje

Element tuple (offsets reais, `hnsw_page.rs`):
```
E_TAG      0        1B
E_DELETED  2        1B   (tombstone)
E_VERSION  3        1B
E_TID      4..12    8B   heap tid
E_NBR_BLK  12..16   4B   ┐ ponteiro p/ o neighbor tuple (grafo) — página SEPARADA
E_NBR_OFF  16..18   2B   ┘
E_DIM      18..20   2B
E_VEC      20..20+dim*4  =3072B (dim=768)   ← o vetor f32
code_bytes 20+dim*4..    =⌈m/2⌉B (=4B)       ← o código AQ, DEPOIS do f32
```
Tamanho: 20 + 3072 + 4 = **3096 B**. ~2 tuples/página (8 KB).

**Trace do `score_candidate(X)` no walk v3:**
1. Ler o element item de X = **3096 B inteiros** (granularidade é o item/página).
2. `decode_element` → `code_bytes` (4B, p/ AH) + `vec_bytes` (3072B, p/ rerank) + `nbr_addr` (no header).
3. Pontuar por AH usa só os 4B de código — **mas para chegar neles paginou 3096B (o f32 junto).**
4. Expandir vizinhos: `nbr_addr` (header) → ler o neighbor tuple (página separada).

→ **Por candidato o walk pagina 3096 B (incl. 3072 B de f32).** Working set quente = 500k×3096 ≈ 1.5 GB.
Esta é a causa-raiz medida da paridade.

## Layout v4 — separar o f32 do hot-path

Novo element tuple (HOT — só o necessário p/ walk+score+expand):
```
E4_TAG      0       1B
E4_DELETED  2       1B
E4_VERSION  3       1B   (=4)
E4_TID      4..12   8B
E4_NBR_BLK  12..16  4B   ┐ ponteiro grafo (neighbor tuple) — inalterado
E4_NBR_OFF  16..18  2B   ┘
E4_RAW_BLK  18..22  4B   ┐ NOVO: ponteiro p/ o raw-f32 tuple (região FRIA) — só p/ rerank
E4_RAW_OFF  22..24  2B   ┘
E4_DIM      24..26  2B
E4_CODE     26..    ⌈m/2⌉B (=4B)   ← só o código AQ; SEM f32
```
Tamanho hot: 26 + 4 = **30 B**. ~266 tuples/página → 500k/266 ≈ 1880 páginas ≈ **15 MB**.

Raw-f32 tuple (COLD — região separada, só rerank):
```
R_TAG   0      1B
R_VEC   4..4+dim*4  =3072B
```
Endereçado por `E4_RAW_BLK/OFF`. Região: 500k × 3080 ≈ **1.5 GB** (fria).

**Trace do `score_candidate(X)` no walk v4:**
1. Ler o hot element item de X = **30 B** (não 3096).
2. `decode_element_v4` → `code_bytes` (4B, AH) + `nbr_addr` (expandir) + `raw_addr` (guardado, NÃO lido agora).
3. Pontuar por AH usa os 4B. **NÃO toca f32.** ✓
4. Expandir vizinhos via `nbr_addr` (neighbor tuple, página separada). **NÃO toca f32.** ✓

→ **Por candidato o walk pagina 30 B (código) + o neighbor tuple. ZERO bytes de f32.** ✓✓
O f32 (via `raw_addr`) é lido **só no rerank** do top-`k·over_fetch` (~320 nós/query).

**Ressalva do owner verificada:** o neighbor tuple (grafo) já é uma página separada (E_NBR_*), NÃO contém f32
(só slots `(blk,off)` de vizinhos — `nbr_size = 4 + slots*6`). Então nenhum metadata do grafo puxa o f32. ✓

## Working set — a conta do ganho (@500k×768)

| Estrutura | v3 (hoje) | v4 | acesso |
|---|---|---|---|
| element (hot) | 1.5 GB (com f32) | **15 MB** (código) | quente — todo candidato |
| neighbor (grafo) | 48 MB | 48 MB | quente |
| raw f32 | (dentro do element) | 1.5 GB (separado) | **frio** — só rerank |
| **hot total** | **~1.5 GB** | **~63 MB** | |

Sob pressão de RAM (< 1.5 GB): v3 thrasha o walk inteiro; v4 mantém 63 MB cache-resident e só o rerank
(~320 × 3080 B ≈ 1 MB/query, random) toca a região fria. **É onde o ganho materializa.**

## Escopo da implementação (v4)

1. **Layout/codec** (`hnsw_page.rs`): novo element tuple v4 (código, sem f32) + raw-f32 tuple + `E4_RAW_*`; a
   meta v4 aponta a região raw. `decode_element_v4`/`pack_v4`. Backward-compat: v1/v2/v3 inalterados (version byte).
2. **Build** (`build.rs`): ao empacotar AQ, escrever o hot element (código) + o raw-f32 tuple na região fria +
   linkar `raw_addr`. O fold preserva ambos.
3. **Scan** (`traverse`): walk pontua por AH lendo só o hot tuple; rerank lê o f32 via `raw_addr`.
4. **TDD:** round-trip v4; **teste de bytes** (assert que `score_candidate` no v4 NÃO decodifica `vec_bytes` — ex.:
   um raw-region envenenado/ausente ainda pontua o walk, só falha no rerank); backward-compat v1/v2/v3; recall
   end-to-end preservado; edge/negative.
5. **Benchmark:** rebuild + medir a **2M×768** (f32 ≈ 6 GB ≫ 16 GB? não — a 2M o f32 = 2M×3072 = 6 GB; usar
   `--memory` p/ < 6 GB) sob pressão real → AQ v4 vs f32. A tese: AQ v4 ≥ 2× sob pressão (hot 250 MB vs 6 GB).

## Veredito de mesa

A conta fecha: v4 reduz o hot working set de 1.5 GB → 63 MB (24×), com o f32 movido p/ frio/rerank-only, e o
grafo (já separado) não puxa f32. Sob pressão real (2M) o AQ v4 deve vencer o f32 — o que o v3 não conseguiu por
co-localização. Implementar.
