# ADR 0018 — SBQ-inline NÃO é superior: veredito D3 do M57 (honest-negative)

**Status:** Accepted · **Date:** 2026-07-08 · **Milestone:** M57 · **Gate:** D3 (anti-sunk-cost) · **Deciders:** CTO (paulohenriquevn)
**Relacionado:** ADR `0015` (SBQ-inline keep/kill — este ADR fecha o D3 que o 0015 deixou pendente de medição a escala), ADR `0012` (benchmark data-degeneracy — a armadilha que quase invalidou esta medição), ADR `0002` (North Star / measurement-first)
**Evidência:** `docs/benchmarks/m57-sbq-superiority.{md}` + `docs/benchmarks/m57-raw/*.json`

## Contexto e problema

O ADR-0015 (M51) manteve o SBQ-inline condicionalmente, com o D3 gated em uma medição a **escala com pressão de
RAM**: reter a tese do AM próprio SÓ se o SBQ inline entregasse **≥2× QPS a recall≥0.99 sob pressão de memória**
(a premissa: os códigos SBQ pequenos cacheiam enquanto os vetores f32 grandes spillam para disco). Enquanto não
medido, "superioridade vetorial" pelo SBQ era **UNBENCHMARKED** — e tratá-la como cumprida violaria a Regra 5 do
projeto. O M57 é essa medição.

## Decisão

**Rejeitar a tese de superioridade do SBQ-inline.** O SBQ é recall-neutro vs f32 mas **consistentemente mais lento**
(0.35–0.77× do QPS do f32) em TODOS os regimes medidos — in-RAM e sob pressão de RAM até 1.3 GB. A tese ≥2× está
**FALSIFICADA por medição**. O AM próprio **não se justifica pela superioridade do SBQ**.

## Evidência (500k×768d cosine, box limpa, `theodb:m58`)

recall idêntico 0.956 (SBQ = f32, recall-neutro):

| Regime | SBQ QPS | f32 QPS | SBQ/f32 |
|---|---|---|---|
| in-RAM (16 GB) | 90 | 256 | 0.35× |
| pressão 1.8 GB | 194 | 266 | 0.73× |
| pressão 1.3 GB (tight) | 218 | 284 | 0.77× |

## Por que (mecanismo — generaliza além do dataset)

1. **HNSW tem localidade de acesso** (toca ~`ef·log N` nós/query) → o índice f32 **não thrasha** sob pressão mesmo
   excedendo a RAM; as páginas quentes ficam cacheadas. A premissa "índice não cabe → I/O por query" não vale.
2. **O read-path do SBQ (Hamming-walk + rerank exato f32) é mais caro por query** e piora relativamente com escala
   (100k: 0.90×; 500k: 0.35× in-RAM) — o oposto do previsto.

## Consequências

- **Reabre/finaliza o ADR-0015:** o own-AM NÃO é retido pela justificativa SBQ. A superioridade vetorial do North
  Star (GOTO P0) NÃO é cumprida pelo SBQ — segue não-cumprida por esse eixo.
- **O AM próprio tem valor geral medido à parte** (theodb HNSW ~1.2× QPS > pgvector a 100k, recall equivalente) —
  tese DIFERENTE (qualidade do grafo/scan), não coberta por este veredito. Se o own-AM for retido, é por ela, com
  seu próprio benchmark.
- **P1 (M59) reenquadrado:** o gap para o SOTA (ScaNN) é **quantização anisotrópica + Asymmetric Hashing SIMD**, não
  bit-quantization escalar. O SBQ inline é fator-constante desfavorável; o M59 deve mirar o eixo anisotrópico.
- O SBQ-inline permanece no código como formato versionado (não removido — sem custo de manutenção ativo e é a base
  de experimentos futuros de quantização), mas **não é o caminho de performance** e não deve embasar claims.

## Opções consideradas

1. **Reter o SBQ como default de performance** — rejeitada: medição mostra 0.35–0.77× (mais lento). Seria um claim
   falso (Regra 5).
2. **Remover o SBQ-inline** — rejeitada (YAGNI reverso): o formato versionado não custa manutenção ativa e é base de
   M59 (quantização anisotrópica). Manter como experimento, não como default.
3. **Honest-negative + reenquadrar (esta decisão)** — medir, registrar a falsificação, e mover o esforço de
   performance para o eixo certo (anisotrópico/M59). Alinha com anti-sunk-cost (CLAUDE.md).

## Caveats

Dados gaussian-mixture sintéticos (não SIFT1M) — a direção é mecânica (localidade HNSW + custo do rerank), mas os
absolutos podem mover; follow-up em SIFT1M rastreado. pgvector não buildou a 500k (`/dev/shm=64MB` do docker) —
baseline pgvector só a 100k. recall 0.956<0.99 a 500k é qualidade do grafo (item à parte do veredito SBQ).
