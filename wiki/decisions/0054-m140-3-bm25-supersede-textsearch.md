---
type: Decision
title: ADR 0054 — BM25 próprio supersede a exceção permissiva do pg_textsearch
description: A superfície BM25 de produção passa a ser código próprio sobre Tantivy MIT; o argumento não é ganho de qualidade (é paridade), e sim own-code permissivo sem dependência externa quebrada.
resource: git:f7c7b93:docs/adr/0054-m140-3-bm25-supersede-textsearch.md
tags: [adr, bm25, lexical, own-code, supersede, m140]
adr_id: "0054"
adr_status: Accepted
decision_date: 2026-07-22
milestone: M140.3
supersedes_in_part: ["0013"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0054
    resource: git:f7c7b93:docs/adr/0054-m140-3-bm25-supersede-textsearch.md
    title: ADR 0054 — BM25 own-code supersede pg_textsearch
    last_modified: 2026-07-22
---

Fecha a linha aberta lá atrás pelo [ADR 0003](/decisions/0003-permissive-bm25-pg-textsearch.md), que
identificara uma peça permissiva de [BM25](/technologies/bm25.md) sem adotá-la, e pelo
[ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md), que a manteve como exceção.

# Contexto

Desde então, três coisas mudaram:

- O spike provou **BM25 próprio viável dentro do Postgres**: [Tantivy](/technologies/tantivy.md) sob
  licença MIT, com um `Directory` nosso e buffer-then-flush ao heap, herdando MVCC, WAL e crash-safety.
- A medição mostrou que a engine própria **bate o baseline embarcado** em retrieval lexical puro, com
  índice ~3,5× menor, e fica em **paridade** com o `pg_textsearch` na qualidade de ranking — cerca de
  4% de diferença de implementação, dentro do ruído.
- E a perna in-DB do `pg_textsearch` foi medida **quebrada**, nunca exercida ponta a ponta.

# Decisão

**A superfície BM25 de produção é código próprio**; a exceção permissiva para BM25 fica **superseded**.
O `pg_textsearch` passa a ser **referência de benchmark**, não componente de produto. O columnar do
ADR 0013 fica intocado.

# Racional

**Mandato de código próprio.** O Tantivy MIT entra como composição própria — o `Directory`, o cache e
a superfície são nossos —, enquanto o `pg_textsearch` é dependência externa que exigiria
`shared_preload_libraries` e reinício, e está com a perna in-DB quebrada. É o oposto de "bateria
inclusa".

**Paridade medida mais vantagens estruturais.** A paridade de ranking está medida, e o próprio código
adiciona índice muito menor, tokenização adequada a logs e identificadores, e o caminho para features
que o full-text nativo não tem — sem custo de dependência externa.

**Honestidade:** **o ganho de qualidade sobre o `pg_textsearch` não é o argumento** — é paridade. O
argumento é código próprio permissivo, com cache e storage nossos.[^adr0054]

# Alternativas consideradas

**Manter o `pg_textsearch` como superfície BM25** — dependência externa, preload, reinício, perna
quebrada, e fora do mandato, com zero benefício medido de qualidade. **Manter só o full-text nativo** —
medido perdendo para BM25 em retrieval lexical puro, e o consumidor real se beneficia da troca.

# Consequências

O roadmap lexical passa a ser código próprio, e o milestone seguinte prova MVCC, VACUUM e crash da
superfície de produção, ligando o primeiro consumidor real.

**Plano de saída do `pg_textsearch`:** como ele **nunca foi default** — era medição descartável, jamais
embarcada —, **não há usuário de produção a migrar**. A saída é remover a imagem de benchmark, não
executar migração de dados.

**Restringe:** a superfície BM25 é a função decidida no
[ADR 0052](/decisions/0052-m140-1-lexical-storage-decision.md) — nem access method, nem
`pg_textsearch`. Mudar exige ADR novo.

[^adr0054]: ADR 0054 — BM25 own-code supersede a exceção permissiva do pg_textsearch
