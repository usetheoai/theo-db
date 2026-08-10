---
type: Technology
title: pgrx
description: O framework que permite escrever extensões PostgreSQL em Rust; é o que torna viável o mandato de código próprio, e a fonte da maior parte das restrições técnicas do projeto.
resource: https://github.com/pgcentralfoundation/pgrx
tags: [tecnologia, rust, extensao, ffi, framework]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pgrx-repo
    resource: https://github.com/pgcentralfoundation/pgrx
    title: pgrx, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O pgrx é o framework que permite escrever **extensões PostgreSQL em Rust**, gerando o SQL de instalação a
partir de anotações no código e expondo os internals do servidor via FFI.[^recalled]

# Papel neste acervo

É **o que torna o mandato de código próprio viável**: o
[ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) decidiu Rust in-engine, e é o pgrx que
permite isso sem escrever C.

# As restrições dele que moldaram decisões

Boa parte das decisões técnicas mais finas do repositório existe por causa de um limite do framework — e
cada uma está registrada com a causa:

**Um único schema por ident de módulo.** Todos os externs compartilham o mesmo módulo de schema, o que
tornou a distribuição por feature arriscada e produziu o facade único do
[ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md).

**Sem derive para tipo varlena de dimensão variável.** O tipo próprio precisou ser definido por SQL de
extensão com seis traits implementados à mão — a receita descoberta no
[spike](/references/spike-m69-theovec-pgrx.md).

**Ausência de acesso a certos lookups de catálogo**, que fixou o escopo de opclass única na primeira
versão dos access methods ([ADR 0010](/decisions/0010-m26-index-am-scope.md)).

**Um GUC customizado que não força restrição de superusuário**, o que exigiu guarda explícita num hook de
injeção de crash para não virar vetor de negação de serviço
([ADR 0014](/decisions/0014-m48-crash-safe-fold-reclaim-mechanism.md)).

**Uma função de SPI que não é read-only apesar do nome sugerir**, o que abria janela de snapshot e
queimava identificadores de transação por busca
([ADR 0055](/decisions/0055-m140-4-lexical-robustness-consumer.md)).

# A armadilha central do FFI

**Um erro do PostgreSQL faz longjmp — não é um panic de Rust.** Testar isso errado dá falso verde, e
esquecer disso derruba o backend. É a razão de o núcleo lexical viver
[num crate que não linka o framework](/decisions/0053-m140-2-lexical-core-crate.md): torna
**estruturalmente impossível** tocar o banco de uma thread errada.

[^pgrx-repo]: pgrx, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
