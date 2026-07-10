# M60 — Recall do HNSW próprio a 500k×768d: medição decisiva vs pgvector (honest-negative + premissa corrigida)

**Data:** 2026-07-10 · **Hardware:** DigitalOcean c-8 (8 vCPU dedicado, 15 GB), fra1 · **pg17.10 (pgrx)** ·
**Corpus:** gaussian-mixture 256 centros, ruído apertado, 500 000×768d, métrica cosine · 50 queries in-distribution ·
GT = brute-force exato (seqscan). Reprodutível: `benchmarks/run_m60_recall.py` (theodb) + `benchmarks/run_m60_pgvector_control.py` (controle pgvector). Raw: `docs/benchmarks/m60-raw/`.

## Veredito: HONEST-NEGATIVE + a DoD do M60 está mal-especificada

O M60 partia da premissa (herdada do M57, inferida de **escalas diferentes**) de que o `theodb_hnsw` tem um gap
de recall de ~2–3pt **específico** vs pgvector, e que o gate **recall@10 ≥ 0.99** é alcançável. A medição
head-to-head no **mesmo corpus a 500k×768d** refuta as duas coisas:

| Índice (ef=1000, 500k×768d, mesmo corpus) | best recall@10 | p50 |
|---|---|---|
| **pgvector hnsw** (m=16, efc=64) | **0.988** | 12.2 ms |
| **theodb_hnsw f32** (own, m=16) | 0.974 | ~15.6 ms |
| **theodb_hnsw SBQ** (over_fetch=32, rerank) | **0.986** | (mais lento — ver M57 D3) |

1. **O gate 0.99 é um artefato do dado.** *O próprio pgvector só chega a 0.988* neste corpus. 256 clusters
   gaussianos apertados em 768d produzem muitos 10-vizinhos quase-equidistantes → o teto de recall@10 fica
   **abaixo de 0.99 para índices da classe HNSW**, pgvector incluído. Perseguir 0.99 absoluto aqui é perseguir um
   número que o SOTA permissivo não atinge nesta distribuição. **A DoD correta é PARIDADE com pgvector (~0.988),
   não 0.99 absoluto** (é a moldura recall-parity que o projeto já usa como North Star).
2. **Existe um gap real, porém menor: ~1.4pt** (f32 0.974 vs pgvector 0.988), e o theodb f32 é também um pouco
   mais lento — um déficit genuíno de qualidade-de-grafo do caminho f32 vs pgvector.
3. **O SBQ (com over_fetch=32 + rerank exato) chega a 0.986 — praticamente paridade com o pgvector** — às custas
   de QPS (o trade-off medido no M57 D3). O caminho f32 puro é o que fica ~1.4pt atrás.

## Hipótese de fix #1 (descida de build por beam) — REFUTADA por medição

O blueprint do discover (dual-source + SOTA) apontou como causa-raiz #1 a descida upper-layer do BUILD ser um
`greedy_descend` (hill-climb) em vez de um beam `search_layer(ef=1)` (Malkov-Yashunin Alg.1 / pgvector). Implementado
e medido a 500k×768d: **recall byte-idêntico ao pré-fix** (ef=1000 → 0.974; ef=200 → 0.898; idêntico ao
`m57-raw/m57p_ef1000.json`).

**Por que foi no-op (aprendizado):** `search_layer(ef=1)` só admite candidatos *mais próximos* que o melhor atual
(heap de resultado tamanho 1) → **nunca retrocede** → é **funcionalmente equivalente ao hill-climb**. E a descida do
pgvector *também* é ef=1 — logo a descida **nunca foi** a diferença theodb-vs-pgvector. 4ª direção refutada do M60
(após efc 64→200, MERGE back-links, m 16→32 do M57). O fix foi **revertido** (não se faz merge de um no-op cujo
comentário alega corrigir o recall — Regra 3).

## O gap real (~1.4pt f32) — onde investigar a seguir (não perseguido nesta iteração)

Refutados por medição/leitura (não repetir): entry-point promotion (correto), upper-layers construídos (correto),
ground-search accept (exato), descida de build (no-op), efc↑, m↑, MERGE back-links. Candidatos remanescentes para
a paridade f32:
- **`select_from` — ordem do keep-back.** theodb reenche `kept` a partir de TODOS os candidatos (nearest-first);
  pgvector reenche especificamente do conjunto PODADO (`wd[]`, `hnswutils.c:1149-1151`), preservando a diversidade
  pretendida. Diferença sutil de qualidade de aresta — cheap de testar (rebuild + remeasure), mas exige outro ciclo.
- **Rerank f32 leve no caminho f32** (o SBQ já mostra que over-fetch+rerank fecha o gap a 0.986) — mas isso é
  trocar QPS por recall, o que colide com o pilar de latência (M71). Trade-off a decidir com medição.

## Implicação para o Roadmap v5

A DoD do M60 (`recall@10 ≥0.99 a 500k×768d`) deve ser **reescrita para paridade com pgvector** (o alvo honesto e
alcançável). Com essa moldura: **SBQ já está em paridade (0.986 vs 0.988)**; o **f32 fica ~1.4pt atrás** e requer
mais um ciclo de investigação (`select_from` keep-back ordering é o próximo lever). M60 **NÃO está completo** — e
como M71–M74 dependem dele, o v5 não avança até M60 fechar (ou ter sua DoD reescrita e re-medida). Risco ALTO
confirmado empiricamente, exatamente como o roadmap previu ("3 direções já eliminadas; pode exigir vários ciclos").

## Reprodução

```bash
# theodb (build + sweep + SBQ + verdict), 500k×768d, ≥3 QPS runs:
PGHOST=localhost PGPORT=<pg> PGUSER=<u> PGDATABASE=postgres \
  python3 benchmarks/run_m60_recall.py --n 500000 --dim 768 --nq 50 --qps-runs 3 --out m60_recall.json
# pgvector controle (DB separado — public.vector colide com theodb_rs):
createdb pgvctl; PGDATABASE=pgvctl python3 benchmarks/run_m60_pgvector_control.py --n 500000 --dim 768 --nq 50 --out m60_pgvector.json
```
