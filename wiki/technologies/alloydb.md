---
type: Technology
title: AlloyDB
description: O banco compatível com PostgreSQL do Google Cloud que serve de âncora SOTA do TheoDB — o alvo declarado, e a referência contra a qual as divergências são justificadas.
resource: https://cloud.google.com/alloydb
tags: [tecnologia, banco-de-dados, sota, ancora, gcp]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: alloydb-site
    resource: https://cloud.google.com/alloydb
    title: AlloyDB, documentação oficial do Google Cloud
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O AlloyDB é o banco **compatível com PostgreSQL** do Google Cloud, com engine baseada no PostgreSQL
acrescida de módulos proprietários — motor colunar em memória, índice vetorial baseado em
[ScaNN](/technologies/scann.md), integração de IA e storage desagregado.[^recalled] Existe também numa
edição executável fora da nuvem.

# Papel neste acervo — a âncora, não o concorrente

O AlloyDB é o **alvo SOTA declarado** do projeto: decisões de produto e arquitetura **espelham** o
AlloyDB, e se afastam dele **apenas com justificativa explícita**.

Esse enquadramento é o que dá sentido a boa parte do repositório. Ele aparece de três formas:

**Como mandato.** O [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) define o north
star como "igual ou superior ao AlloyDB" para usuários OSS e on-prem.

**Como limite medido.** O [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md) registra que
superar o AlloyDB em QPS vetorial é **estruturalmente não-alcançável** por uma extensão PostgreSQL
permissiva — e o [ADR 0033](/decisions/0033-north-star-reposition-proposal.md) formaliza o
reposicionamento decorrente.

**Como fonte de divergências declaradas.** Onde o AlloyDB usa colunar **in-memory automático**, o projeto
usa [lakehouse em disco](/features/15-lakehouse-parquet.md) — porque os pares in-memory do ecossistema
PostgreSQL são AGPL e a licença os barra. Onde o AlloyDB oferece integração de IA acoplada a um provedor,
o projeto oferece [independência de modelo](/features/07-funcoes-ia-sql.md).

# A superioridade estrutural reivindicada

O que o projeto afirma ter **hoje, sem benchmark**: abertura auditável, ausência de licença por vCPU,
portabilidade entre laptop e bare-metal, e **independência de modelo** — contra o acoplamento a um
provedor específico.[^alloydb-site]

# A disciplina de nomenclatura

Compatibilidade de API com o AlloyDB é razão declarada para escolhas de assinatura, como a semântica
síncrona por linha das funções de IA
([ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)) — mas **não é imitação cega**: o
[ADR 0024](/decisions/0024-m65-ai-rerank-cross-encoder.md) diverge deliberadamente de um nome de função
para evitar colisão interna.

[^alloydb-site]: AlloyDB, documentação oficial do Google Cloud
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
