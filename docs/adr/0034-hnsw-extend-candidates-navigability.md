# ADR-0034 — HNSW build: `extendCandidates` (default ON) para fechar a degradação de recall por escala

- **Status:** Accepted (2026-07-10)
- **Contexto:** Gap 1 do pilar vetorial (theodb → paridade pgvector), Roadmap v5.

## Contexto (medido, white-box + droplet)

O `theodb_hnsw` degradava recall com escala: recall@10 0.998@100k → **0.974@500k** (pgvector 0.988). 7 levers
black-box refutados. Trocamos para **white-box** (analisador que mede a estrutura do grafo, local): conectividade
perfeita (out-degree cheio), mas **100% das misses são ROTEAMENTO** e o hop-distance cresce com a escala (5→7.3→9.6)
— um grafo mal-navegável. Causa paper-grounded: faltava **`extendCandidates`** (Malkov-Yashunin INSERT, recomendado
para dados **extremamente clusterizados** — exatamente nosso regime de 256 clusters gaussianos).

## Decisão

Estender o pool de candidatos com os vizinhos-dos-vizinhos (na mesma camada) antes do `select_from`, nos dois
caminhos de build (`ann/hnsw.rs` sequencial + `ann/hnsw_parallel.rs` paralelo). Tunável via
`THEODB_HNSW_EXTEND_CANDIDATES` (**default ON**; `=0` desliga para build ~2-3× mais rápido).

## Evidência (500k×768d, `docs/benchmarks/gap1-extend-candidates.md`)

- **Recall f32 0.974 → 0.990; SBQ 0.986 → 0.994** (= paridade de VALOR de recall com pgvector 0.994). A curva
  inteira subiu ~5pt; o f32 agora **alcança ≥0.99 a 500k** (antes platôava em 0.974). 63/63 pg_tests GREEN.
- **HONESTO (Regra 3):** NÃO é paridade de FRONTIER — a pgvector ainda tem recall maior no mesmo `ef` (ef=200:
  0.988 vs 0.952) → a iso-recall o theodb é ~1.8× mais lento. O fix sobe o teto, não iguala a eficiência
  recall-por-ef. E o build fica ~2-3× mais lento (por isso o opt-out).

## Alternativas rejeitadas

- **Manter sem extend:** deixa o recall platôando em 0.974 (abaixo do pgvector) — o eixo do North Star onde
  estávamos atrás.
- **Default OFF:** não entregaria o ganho de recall por padrão; como recall-quality > build-speed p/ a maioria dos
  workloads vetoriais, o default é ON com escape.
- **Só mexer no query-time / ef_search:** refutado — o defeito é do BUILD (estrutura do grafo), não da busca.

## Consequências

- **Positivas:** recall classe-pgvector a 500k (fecha a degradação por escala); tunável.
- **Custos:** build ~2-3× mais lento (erode a vantagem de build-speed) — mitigado pelo opt-out env. Frontier de
  latência ainda da pgvector (~1.8× a iso-recall) — **follow-up:** refinar a diversidade do `select_from`
  (`SelectNeighbors`/`keep_pruned` exato — Qdrant/hnsw_rs); tunar via reloption per-index.

## Cross-references

- Evidência: `docs/benchmarks/gap1-extend-candidates.md`, `docs/benchmarks/gap1-extend-candidates/*.json`
- Root blocker: `docs/benchmarks/p0-vector-superiority-root-blocker.md`
- Reposicionamento: `docs/adr/0033-north-star-reposition-proposal.md`
- Referências (permissivas): Qdrant `graph_layers_builder.rs`, hnsw_rs (`keep_pruned`/`extend_candidates`)
