---
type: Decision
title: ADR 0009 — A api-surface do theodb_rs é um módulo facade único (api.rs)
description: Emenda a um ADR intra-plano: os externs pgrx ficam num único api.rs de 640 LoC e o budget heurístico de 500 LoC é formalmente dispensado para esse arquivo.
resource: git:f7c7b93:docs/adr/0009-theodb-rs-api-surface-single-module.md
tags: [adr, arquitetura, rust, pgrx, m25, code-quality]
adr_id: "0009"
adr_status: Accepted
decision_date: 2026-07-01
owner: human:paulohenriquevn
milestone: M25
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0009
    resource: git:f7c7b93:docs/adr/0009-theodb-rs-api-surface-single-module.md
    title: ADR 0009 — theodb_rs api-surface as a single api.rs facade module
    last_modified: 2026-07-01
---

Uma emenda: o plano do M25 travara num ADR intra-plano a distribuição **por-feature** dos
externs, e a implementação entregou um **facade único**. Divergir de um ADR travado exige emenda
formal — este documento é essa emenda, e o defeito real que o review apontou não foi o desenho,
foi a divergência não registrada.

# Contexto

O M25 dividiu o god-file `lib.rs` (721 LoC, acima do budget heurístico de 500). O plano previa
que cada módulo de domínio — `embed`, `nl`, `hybrid`, `sbq` — passasse a possuir seus próprios
`#[pg_extern]` e blocos `extension_sql!`, citando o layout do
[pgvectorscale](/technologies/pgvectorscale.md).

Ao mover o código, apareceu uma restrição estrutural que o plano não previu: **todos os externs
compartilham um único `#[pg_schema] mod theodb_rs`** — o schema SQL vem do *ident* do módulo.
Distribuí-los exigiria ou N blocos `#[pg_schema] mod theodb_rs` espalhados por N arquivos (um
padrão [pgrx](/technologies/pgrx.md) menos comum e não validado neste repositório), ou reescrever
a fronteira de schema. Ambos introduzem **risco novo** sobre código já provado byte-idêntico.

O resultado adotado foi um `api.rs` único de 640 LoC, contendo o `mod theodb_rs {externs}` mais
os 8 blocos `extension_sql!` movidos **verbatim**. Isso deixou o `lib.rs` em 92 LoC.

# Tensão reconhecida

Registrada sem maquiagem: o critério de pronto do plano dizia "todo arquivo alterado dentro do
budget de 500 LoC", e `api.rs` tem 640 — literalmente não cumprido, e o documento de benchmark
registra os 640.

# Decisão

A api-surface é um **módulo facade único**, e o budget de 500 LoC é **formalmente dispensado**
para esse arquivo, por cinco razões:

1. **É a fronteira de camada, não um god-module.** A regra de arquitetura proíbe god-modules —
   acúmulo de código *não-relacionado*. O `api.rs` tem **uma** responsabilidade coesa: a
   superfície SQL. Coesão alta, não lixeira.
2. **~87% é SQL declarativo.** O arquivo é majoritariamente strings DDL, com complexidade
   ciclomática ~zero. O CCN máximo pós-M25 (`ann_query::knn`, 15) está *fora* do `api.rs`. O
   risco que o budget de LoC mitiga — funções longas e complexas — não se aplica a DDL.
3. **O budget de 500 é heurístico, não consenso.** A própria auditoria de arquitetura o marca
   como folclore sem fonte forte. A métrica **essencial** do objetivo (`lib.rs < 200 LoC`) foi
   cumprida, com 92.
4. **Esforço ≠ Complexidade.** Fatiar um facade declarativo coeso em 8 arquivos minúsculos só
   para satisfazer um número folclórico é complexidade acidental auto-imposta.
5. **Consistência parcial e honesta com a SOTA.** `pgvectorscale` e `vectorchord` mantêm `lib.rs`
   fino (47 e 83 LoC) e empurram a superfície para módulos dedicados — que é o que o M25 fez.
   Divergência honesta: eles espalham por feature, nós concentramos, forçados pelo
   `#[pg_schema]` único.

# Alternativas rejeitadas

**A1 — split por-feature** (o ADR intra-plano original): introduz risco novo sobre código já
provado byte-idêntico, com ganho cosmético. Reabri-la exige prova de que o split multi-schema é
seguro — um slice próprio. **A2 — separar externs de DDL em dois arquivos:** reduz LoC sem
multi-schema, mas espalha uma unidade de leitura ("o extern X e seu wrapper SQL") por dois
arquivos para cumprir um número. Reconsiderável se o `api.rs` acumular lógica não-declarativa.
**A3 — manter no `lib.rs`:** era o achado original — duas responsabilidades num arquivo.

# Consequências

`lib.rs` vira raiz de composição fina e cumpre a métrica essencial; a api-surface fica numa
fronteira nomeada e coesa; e há **zero mudança de comportamento** — schema e DDL byte-idênticos,
provados por rebuild, 72 testes de integração verdes e revisor de paridade
([m25](/benchmarks/m25-craft-hardening.md)).

Aceita-se que `api.rs` permaneça acima do budget heurístico. Se vier a acumular lógica
**não-declarativa**, A1/A2 reabrem num slice dedicado.[^adr0009]

[^adr0009]: ADR 0009 — theodb_rs api-surface as a single api.rs facade module
