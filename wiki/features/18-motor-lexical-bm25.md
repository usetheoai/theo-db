---
type: Feature
title: Motor lexical BM25 próprio — medido, mas fora do binário default
description: Existe e é melhor que ts_rank_cd em lexical puro por margem modesta, mas na fusão RRF não há ganho e num corpus lexical-pesado mede pior — por isso não é o default.
resource: git:f7c7b93:docs/features/18-motor-lexical-bm25.md
tags: [feature, lexical, bm25, tantivy, feature-flag, honest-negative]
feature_status: entregue como núcleo medido — NÃO no binário default
milestone: M139+M140
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat18
    resource: git:f7c7b93:docs/features/18-motor-lexical-bm25.md
    title: Busca lexical BM25 (motor own-code)
---

**Status: entregue como núcleo medido, e NÃO presente no binário default.** As funções de construção e
busca existem e foram medidas, mas são compiladas **apenas sob feature flag** — a extensão default não
as tem. **A perna lexical default da [busca híbrida](/features/06-busca-hibrida.md) continua sendo o
`ts_rank_cd` nativo.**

# O trade-off honesto — a leitura mais importante desta página

| Regime | Resultado medido |
|---|---|
| Lexical **puro** contra `ts_rank_cd` | o motor próprio **ganha**, por margem **modesta e dependente de contexto** ([m140.1](/benchmarks/m140-1-lexical-measurement.md)) |
| Contra `pg_textsearch` em nDCG@10 | fica **~4% abaixo** num regime ([m140.3](/benchmarks/m140-3-bm25-engine.md)) |
| Na **fusão RRF** | **não há ganho mensurável** |
| Num corpus **lexical-pesado**, na fusão | a troca mede **pior** ([m138](/benchmarks/m138-bm25-fusion.md)) |

**O BM25 é melhor em lexical puro, mas a fusão RRF lava a diferença.** É por isso que o default
embarcado permanece o nativo — uma decisão que privilegia o resultado medido do sistema completo sobre
o resultado medido de um componente isolado.

# O que ele é, arquiteturalmente

Um motor sobre [Tantivy](/technologies/tantivy.md) (MIT), com o índice persistido no **heap** do
PostgreSQL — herdando MVCC, WAL, TOAST e crash-safety de graça, sem página nem resource manager
próprio. As três decisões que o desenharam:

- **Heap em vez de access method próprio** ([ADR 0052](/decisions/0052-m140-1-lexical-storage-decision.md)),
  porque o índice medido é **menor**, não maior — derrubando o argumento clássico a favor do AM.
- **Núcleo num crate livre de pgrx** ([ADR 0053](/decisions/0053-m140-2-lexical-core-crate.md)), o que
  torna os testes rodáveis sem link do Postgres **e** garante estruturalmente que nenhuma thread do
  Tantivy toque o banco.
- **Buffer-then-flush obrigatório** ([ADR 0051](/decisions/0051-m139-tantivy-pg-page-directory-design.md)),
  porque foi **medido** que o Tantivy chama o storage de 4 threads distintas, e SPI é exclusivo da
  thread do backend — escrever direto derrubaria o backend.

# Robustez provada

Crash com replay de WAL, VACUUM recuperando tuplas mortas, e MVCC correto nos dois níveis de
isolamento — tudo provado **contra o binário embarcado**, não só em suíte unitária
([m140.4](/benchmarks/m140-4-robustness-consumer.md)).

# Uso, quando compilado

```sql
SELECT bm25_build(...);   -- constrói e incrementa a geração do índice
SELECT bm25_search(...);  -- consulta
```

O cache é MVCC-correto: um leitor com snapshot antigo não vê a construção de outra sessão, e o
statement seguinte sob read committed vê.

# Relacionados

A identificação original da peça permissiva está no
[ADR 0003](/decisions/0003-permissive-bm25-pg-textsearch.md); a decisão de que a superfície de produção
é código próprio, no [ADR 0054](/decisions/0054-m140-3-bm25-supersede-textsearch.md).
