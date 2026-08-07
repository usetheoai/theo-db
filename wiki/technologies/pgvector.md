---
type: Technology
title: pgvector
description: A extensão vetorial de referência do ecossistema PostgreSQL — foi a base do TheoDB, depois o baseline de paridade, e por fim foi removida e substituída por código próprio.
resource: https://github.com/pgvector/pgvector
tags: [tecnologia, extensao, vetorial, baseline, removido]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pgvector-repo
    resource: https://github.com/pgvector/pgvector
    title: pgvector, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O pgvector é a extensão que trouxe busca vetorial ao PostgreSQL e virou o padrão de facto do
ecossistema — presente em praticamente toda oferta gerenciada de Postgres. Ela provê o tipo `vector`, os
operadores de distância `<->`, `<#>` e `<=>`, e índices [HNSW](/technologies/hnsw.md) e IVFFlat, em C, sob
licença permissiva.[^recalled]

# Papel neste acervo — três papéis, em sequência

**Primeiro, foi a base.** O [ADR 0001](/decisions/0001-no-engine-fork.md) escolheu o modelo de extensão
justamente para incorporá-la sem forkar o engine.

**Depois, virou o baseline de paridade.** Praticamente toda medição vetorial do repositório a usa como
controle: [paridade numérica](/benchmarks/m20-vector-ops-parity.md) dos kernels, paridade de
[recall dos índices](/benchmarks/m21-ann-index-parity.md),
[fronteira de recall × QPS](/benchmarks/m45-pareto-sift1m.md), e o
[critério de recall](/decisions/0030-m60-recall-parity-not-absolute-099.md) que passou a ser
**paridade com ela** em vez de um valor absoluto — porque **ela própria não atinge esse valor** no corpus
medido.

**Por fim, foi removida.** O [ADR 0028](/decisions/0028-m69-own-vector-type.md) construiu um tipo próprio
**byte-idêntico** ao dela, e o [ADR 0029](/decisions/0029-m70-drop-pgvector.md) a removeu por completo —
tornando o projeto o primeiro access method permissivo com tipo `vector` 100% próprio.

# O legado que permanece

**A grafia.** Os operadores e a sintaxe continuam idênticos, deliberadamente — é isso que torna a
migração drop-in.

E quando o dogfood descobriu que aplicações reais **não subiam** porque executam `CREATE EXTENSION vector`
no bootstrap, o [ADR 0058](/decisions/0058-pgvector-compat-shim.md) criou um shim que satisfaz o
tooling **sem reintroduzir a dependência** — e que declara, no próprio comentário visível ao usuário, que
a implementação é própria.

# Migração

Instalações existentes migram pelo procedimento de
[migração de tipo](/guides/pgvector-migration.md), que **exige janela de manutenção** — porque o tipo
próprio ocupa o mesmo nome e os dois não coexistem.

[^pgvector-repo]: pgvector, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
