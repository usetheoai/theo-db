---
type: Decision
title: ADR 0053 — Núcleo lexical num crate livre de pgrx (theodb_lexical)
description: O núcleo é lógica pura e vivia dentro do cdylib pgrx, então cargo test tentava linkar símbolos do Postgres e falhava; extraí-lo torna os testes rodáveis stock sem contradizer o ADR 0009.
resource: git:f7c7b93:docs/adr/0053-m140-2-lexical-core-crate.md
tags: [adr, arquitetura, rust, crate, testabilidade, dip, m140]
adr_id: "0053"
adr_status: Accepted
decision_date: 2026-07-22
milestone: M140.2
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0053
    resource: git:f7c7b93:docs/adr/0053-m140-2-lexical-core-crate.md
    title: ADR 0053 — Núcleo lexical num crate pgrx-free
    last_modified: 2026-07-22
---

# Contexto

O spike anterior ([ADR 0051](/decisions/0051-m139-tantivy-pg-page-directory-design.md)) provou que o
núcleo do motor lexical é **livre de pgrx por desenho**: importa apenas a biblioteca padrão e o
[Tantivy](/technologies/tantivy.md), sem nenhum símbolo do Postgres.

Mas ele vivia **dentro** do crate cdylib pgrx, e por isso `cargo test` tentava linkar os símbolos do
Postgres e **falhava**. Os testes do núcleo só rodavam pelo runner do pgrx, que não linka no ambiente
de medição — deixando-os efetivamente presos.

# Decisão

**O núcleo lexical vive num crate próprio, rlib, cuja única dependência é o Tantivy, sem pgrx**; o
crate cdylib o consome atrás de uma feature. O crate cdylib vira a raiz do workspace, com o núcleo
como membro.

# A reconciliação com o ADR 0009 — não há reversão silenciosa

O [ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md) decidiu que a **superfície SQL** é
um módulo facade único, porque todos os externs compartilham um único schema derivado do ident do
módulo.

**Este ADR não contradiz aquele.** O núcleo lexical tem **zero externs** — não é superfície SQL, é
**lógica pura**, isto é, outra camada. A restrição do ADR 0009 é sobre a camada de externs; separar
uma camada de lógica pura por **testabilidade** é ortogonal a ela.

Os externs do spike permanecem no crate cdylib — hoje como externs de topo, colocação herdada e
temporária, fora do facade. O que a reconciliação exige é que o **núcleo extraído** tenha zero
externs — e tem. É inversão de dependência na forma canônica: **o núcleo define o trait de storage; a
camada pgrx o implementa sobre o heap.**[^adr0053]

# Alternativas consideradas

**Manter o núcleo dentro do crate pgrx e testar só pelo runner especial** — os testes ficariam presos,
e testabilidade stock é justamente o ganho deste milestone. **Colocar o workspace na raiz do
repositório** — arrastaria diretórios não-Rust para dentro de um workspace cargo. **Publicar o crate**
— YAGNI, o uso é interno.

# Consequências

**Habilita** rodar os testes do núcleo sem pgrx, e testar query e scoring puros sem o link. O CI passou
a rodar o teste do núcleo mais um **gate objetivo**: a árvore de dependências do núcleo precisa ter
**zero** ocorrências de pgrx.

**Restringe:** o núcleo é genuinamente livre de pgrx — se um tipo pgrx vazar para lá, **o crate para
de compilar**. O manifesto sem pgrx é o gate, e ele é objetivo, não uma convenção que alguém precisa
lembrar de seguir.

**Custo:** o crate principal vira workspace, o que foi validado como compatível com o ferramental
existente.

[^adr0053]: ADR 0053 — Núcleo lexical num crate pgrx-free
