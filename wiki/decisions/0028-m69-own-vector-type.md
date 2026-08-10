---
type: Decision
title: ADR 0028 — Tipo vetorial próprio theodb.vector, byte-idêntico ao pgvector
description: O TheoDB passa a shipar seu próprio tipo vector em Rust, com layout on-disk bit-a-bit igual ao do pgvector para habilitar cast binário sem função e migração sem reescrita.
resource: git:f7c7b93:docs/adr/0028-m69-own-vector-type.md
tags: [adr, tipo, vector, pgrx, varlena, own-code, m69]
adr_id: "0028"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M69
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0028
    resource: git:f7c7b93:docs/adr/0028-m69-own-vector-type.md
    title: ADR 0028 — M69 tipo vetorial próprio
    last_modified: 2026-07-09
---

A fundação da independência total do [pgvector](/technologies/pgvector.md). Até aqui o TheoDB
**reusava** o tipo `vector`; a partir daqui ele tem o seu.

**Prior-art honesto:** os dois AMs vetoriais próprios permissivos de referência — VectorChord e
[pgvectorscale](/technologies/pgvectorscale.md) — **reusam** o tipo do pgvector. O TheoDB seria o
primeiro AM permissivo a shipar tipo `vector` próprio em [pgrx](/technologies/pgrx.md). Um spike
prévio retirou o risco.

# D1 — layout `#[repr(C)]` byte-idêntico

O header on-disk é `{ varlena: u32, dim: u16, unused: u16, elements: [f32;0] }` — 8 + 4·dim bytes,
bit-a-bit igual ao do pgvector, com `SET_VARSIZE` little-endian e `unused` sempre zero (que viaja no
wire e é validado).

Isso é a **pré-condição do cast binário `WITHOUT FUNCTION`**: habilita coexistência agora e
**migração grátis depois**, sem reescrita de heap.

**Rejeitado:** layout próprio — perderia a coercibilidade binária, forçando reescrita O(N) de toda
tabela na migração.

# D2 — definição via `extension_sql!` com funções de I/O

O pgrx 0.16.1 não tem derive para tipo varlena de dimensão variável, então os seis traits necessários
são implementados à mão sobre `NonNull<VecHeader>`.

# D3 — reusar os kernels de distância existentes

Os operadores `<->`, `<#>` e `<=>` chamam os kernels f32 já existentes — sem reimplementar distância.
O `<#>` é o inner product **negativo**, por paridade com o pgvector.

# D5 — naming e o caminho para drop-in

O tipo nasce como `theodb.vector` — nome `vector` no schema `theodb` —, coexistindo com
`public.vector` do pgvector **sem colisão**, já que os schemas são distintos (colisão de nome de
tipo é um problema real e documentado no ecossistema). As funções são prefixadas para evitar colisão,
e os operadores são sobrecarregados por tipo de argumento.

O passo seguinte moverá o tipo para `public`, fazendo `::vector` do usuário resolver ao tipo próprio
— drop-in.

**Rejeitadas:** um nome distinto como `theovec` (exigiria rename depois) e manter `theodb.vector`
permanentemente namespaced (não seria drop-in).

# Escopo da "paridade" — a honestidade fina

A paridade é de **valor e wire, byte a byte**, e **não** de texto de erro:

- **Provado byte a byte:** layout on-disk e wire binário idênticos — o gate compara
  `md5('…'::vector::bytea) = md5('…'::theodb.vector::bytea)` em dimensões 1, 3, 5, 128 e **300**
  (acima de 255, para exercitar o byte alto do `u16 dim`), além do cast nos dois sentidos.
- **Deliberadamente NÃO idêntico:** as **mensagens de erro e o SQLSTATE** são próprios do TheoDB,
  não verbatim os do pgvector. Os erros são tipados e claros, mas um cliente que discrimina por
  SQLSTATE verá o código do TheoDB.[^adr0028]

# Consequências

Habilita a remoção total do pgvector. O código próprio passa a cobrir I/O em texto e wire binário,
typmod, operadores e casts. E há **zero regressão no hot path**: este milestone não tocou o access
method, com os testes do HNSW todos verdes.

**Ressalvas:** é território novo, sem peer permissivo que tenha feito o mesmo. E a coexistência
ainda exige o pgvector instalado — por desenho, removido no passo seguinte
([ADR 0029](/decisions/0029-m70-drop-pgvector.md)).

# Licença

Código **original**. A técnica de varlena foi aprendida de fontes permissivas (pgvector está sob
PostgreSQL License, mais os headers do Postgres e a documentação do pgrx). VectorChord é AGPLv3 /
Elastic License — **apenas estudo, nunca copiado**.

[^adr0028]: ADR 0028 — M69: tipo vetorial próprio theodb.vector
