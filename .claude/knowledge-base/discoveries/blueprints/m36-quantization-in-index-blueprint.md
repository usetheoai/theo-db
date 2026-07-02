# Blueprint: M36 — quantização-no-índice (o gate measurement-first FALSIFICOU a premissa)

> **Discovery verdict:** ⚠️ **PREMISSA FALSIFICADA — o milestone precisa de re-escopo.** O primeiro checkbox do DoD
> do M36 ("`THEODB_SCAN_PROFILE=1` confirma que `score_us` domina `reads_us`") FALHOU quando medido. A distância
> full-precision é **~15% do custo de scan**, não o gargalo. Este blueprint documenta a medição e propõe o
> re-escopo honesto. Método: measurement-first (o profiler `THEODB_SCAN_PROFILE`, dados reais).

**Slug:** `m36-quantization-in-index` · **Owner:** paulohenriquevn · **Created:** 2026-07-02

## Context

M36 foi escopado (roadmap + análise `council-vector-ann`) sob a hipótese de que o custo por candidato no scan é
dominado pela **distância full-precision f32** (`vec.rs:167` `l2_dist_from_bytes`), e que quantizar a distância
(distância assimétrica sobre códigos) fecharia o gap de ~25× vs ScaNN (M33). O DoD do M36 **explicitamente gateou
o milestone nesta medição** (checkbox #1). O `council-vector-ann` marcou a hipótese como **UNBENCHMARKED** e disse
"este é o primeiro número a levantar". Levantamos.

## A medição (o achado que reformula o milestone)

`THEODB_SCAN_PROFILE=1`, índice `theodb_ivfflat` sobre 200k×128 (vetores distintos, seed 42), varredura de probes.
As 3 fases do scan (`am/scan.rs`), estáveis em 5 runs e em 3 pontos de probes:

| probes | candidatos | **reads (I/O)** | **sort** | **score (distância)** |
|---|---|---|---|---|
| 10 | 10.216 | 51% | 35% | **14%** |
| 50 | 50.332 | 49% | 37% | **15%** |
| 100 | 100.107 | 44% | 41% | **15%** |

**Conclusão (dados, não suposição):** a distância full-precision é a **MENOR** das três fases (~14–15%). O I/O de
leitura das páginas (`reads`, ~44–51%) e a **ordenação de TODOS os candidatos** (`sort`, ~35–41%) dominam.

**Corolário:** mesmo *eliminando 100% da distância*, o speedup máximo seria ~1.18× — **muito longe** dos ~25×
necessários para fechar o gap do M33. **Quantizar a distância ataca o alvo errado.** A premissa do M36 está
falsificada pela evidência. (Isto é o measurement-first funcionando como projetado — como no M31b (degeneração de
dados), M34 (cross-use do planner) e M35 (pages-read vs wall-clock).)

## Por que os gargalos reais são reads e sort (ancorado no código)

- **`sort` (~38%) — `am/scan.rs:109`:** `results.sort_by(...)` ordena o vetor **inteiro** de candidatos
  (50k–100k) — O(C·log C) — quando o executor só quer o top-K (LIMIT). Um **heap top-K limitado**
  (`select_nth_unstable` / `BinaryHeap` de tamanho K) é O(C·log K) — ordens de grandeza mais barato. ScaNN nunca
  ordena 50k; mantém um heap limitado. **Fix barato, alto impacto, zero risco de recall.**
- **`reads` (~44–51%) — `am/scan.rs` (leitura das páginas de lista):** cada candidato lê **512 bytes** de vetor
  f32 (dim=128) da página. Aqui a **quantização AJUDA de verdade** — mas via **redução de I/O**, não de distância:
  SBQ 1-bit (`sbq.rs`) = **16 bytes/vetor** (32× menos dados por candidato) → muito menos bytes lidos por página →
  menos `reads`. O rerank f32 do top-over_fetch recupera o recall. **Este é o papel real da quantização no M36.**
- **`score` (~15%):** o alvo original do M36. Reduzi-lo com distância assimétrica (Hamming sobre códigos) é um
  bônus pequeno, não o motor.

## A verdade mais profunda sobre o gap de ~25× vs ScaNN (honesto)

O gap não é "distância por candidato". É **contagem de candidatos + o custo O(C·log C) de ordená-los**. ScaNN
varre MUITO menos candidatos (particionamento anisotrópico + SOAR podam mais forte) e usa heap limitado. Nós
varremos 50k e ordenamos todos. Fechar o gap exige atacar, em ordem de alavancagem medida:
1. **`sort` → heap top-K limitado** (elimina O(C·log C); ~38% do custo).
2. **`reads` → códigos quantizados menores no scan** (menos bytes/candidato; ~44% do custo) + rerank f32.
3. **contagem de candidatos → poda melhor** (menos candidatos varridos; ataca as três fases) — pesquisa maior.

## Coverage Corner 1 — Integration Tests
Round-trip: scan com heap top-K == scan com sort completo (mesmos top-K, recall idêntico) — o heap não muda o
resultado, só o custo. Scan com códigos SBQ + rerank == recall f32 dentro de tolerância. Provado via
`benchmarks/theodb_bench/` + `#[pg_test]`.

## Coverage Corner 2 — Dependencies
Nenhuma nova. Reusa `sbq.rs` (M22), `vec.rs`, `am/page.rs`. `select_nth_unstable`/`BinaryHeap` são std.

## Coverage Corner 3 — Tools
`THEODB_SCAN_PROFILE=1` (o profiler que produziu este achado), `benchmarks/theodb_bench/` (recall/QPS),
`EXPLAIN (ANALYZE, BUFFERS)` (pages-read).

## Coverage Corner 4 — Techniques
Heap top-K limitado (partial-sort). Scalar quantization para redução de I/O (não de distância). Rerank f32 do
top-over_fetch (o padrão do `sbq.rs:knn`, mas movido para o hot path do scan). Distância assimétrica (bônus).

## ADRs

### ADR-1 — RE-ESCOPO: M36 ataca reads+sort (os gargalos medidos), não a distância
**Decisão:** re-escopar o DoD do M36 de "quantizar a distância" para "atacar os gargalos medidos": (1) heap top-K
no `sort`, (2) códigos quantizados menores para cortar `reads` + rerank f32. **Rationale:** os dados
(`THEODB_SCAN_PROFILE`) mostram distância = ~15%; sort+reads = ~85%. Otimizar a distância seria otimizar o alvo
errado — um workaround para "nominalmente cumprir o milestone". **Rejeitado:** implementar distância assimétrica
como motor (ataca 15%; não fecha o gap — falsificado pela medição).

### ADR-2 — heap top-K primeiro (win barato, zero risco de recall), medir, depois quantização de I/O
**Decisão:** entregar o heap top-K limitado como o primeiro slice (elimina O(C·log C); não toca no recall), medir
o ganho, depois a quantização de I/O (mais arriscada p/ recall). **Rationale:** parsimony + risco crescente; o
heap é correção pura de complexidade sem trade-off de recall.

## Recommendations
1. **Surface o achado ao humano** — o DoD do M36 foi escrito sob uma premissa que a medição falsificou.
   Re-escopar o DoD é uma mudança de roadmap (convenção: decisão do humano).
2. Re-escopo proposto (ADR-1/2): heap top-K → medir → quantização de I/O + rerank → medir. Cada passo com
   benchmark `m36-*.json`, recall preservado como gate.
3. O claim de fechar 25× **não** vem só da quantização — vem de sort+reads+poda. Honesto no artefato.

## Top 3 risks
- **R1:** re-escopar sem o humano seria reescrever um DoD de roadmap unilateralmente (viola convenção). → surface.
- **R2:** a medição é em dados sintéticos 200k; a 1M/SIFT as proporções podem deslocar (mais candidatos → sort e
  reads crescem; distância continua ~15%). → confirmar a 1M no benchmark, mas a conclusão qualitativa (distância
  não domina) é robusta em 3 pontos de probes.
- **R3:** quantização de I/O tem risco de recall (SBQ-1bit teta ~0.86 no protótipo). → heap top-K primeiro (zero
  risco); quantização com rerank f32 + gate de recall.
