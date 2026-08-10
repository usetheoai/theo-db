---
type: Reference
title: Spike — viabilidade de um tipo vector próprio em pgrx
description: Prova que é possível definir um tipo varlena de dimensão variável em pgrx com layout byte-idêntico ao pgvector, e registra a receita de oito passos que virou a fundação do tipo próprio.
resource: git:f7c7b93:docs/spikes/m69-theovec-pgrx-feasibility/REPORT.md
tags: [referencia, spike, pgrx, tipo, varlena, viabilidade]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: spikem69
    resource: git:f7c7b93:docs/spikes/m69-theovec-pgrx-feasibility/REPORT.md
    title: Spike M69 — Viabilidade de tipo vector próprio em pgrx
    last_modified: 2026-07-09
---

**Veredito: risco retirado, positivo.** É o spike que destravou a independência total do
[pgvector](/technologies/pgvector.md).

# A pergunta

**Nenhum access method permissivo shipa um tipo `vector` próprio em [pgrx](/technologies/pgrx.md)** — os
dois de referência reusam o do pgvector. O padrão de **definir** um tipo denso de **dimensão variável**
(varlena com array flexível) **não tinha prior art público**.

A pergunta era binária: é viável, com layout byte-idêntico? Se não fosse, todo o caminho de independência
caía.

# Resultado

Sete testes verdes em PostgreSQL real, cobrindo I/O de texto, parse e **enforcement** de typmod, rejeição
de NaN com erro tipado, binding de operador, uso como coluna com ordenação — e o decisivo:

**compatibilidade binária provada**, com o cast sem função funcionando **nos dois sentidos**. É esse teste
que habilitou a migração sem reescrita prometida pelo
[ADR 0028](/decisions/0028-m69-own-vector-type.md).

# A receita descoberta

O valor durável do spike é esta lista — cada item foi descoberto, não presumido:

1. **Layout byte-idêntico**: `{ varlena: u32, dim: u16, unused: u16, elements: [f32; 0] }`, ocupando
   8 + 4·dim bytes.
2. **Tamanho do varlena em little-endian**, com o deslocamento correto.
3. **Seis traits de plumbing de Datum** implementados à mão — a API é ditada pelo pgrx.
4. **O tipo é criado por SQL de extensão, não por derive**: o pgrx **não tem** derive para varlena de
   tamanho variável, o que foi confirmado e não suposto.
5. **O enforcement de typmod exige o cast de coerção de comprimento** — *a peça não óbvia*. Sem ela,
   `tipo(N)` **parseia mas não enforça no INSERT**, o que é pior que não suportar, porque parece
   funcionar.
6. **O cast sem função** é o que prova que o layout é reinterpretável sem reescrita.
7. **Use o gerador de esqueleto do ferramental**, em vez de montar a estrutura do crate à mão.
8. **Caso negativo em teste**: um erro do PostgreSQL faz longjmp, **não é panic de Rust** — então o teste
   precisa declarar o erro esperado, e não tentar capturar unwind.

O item 8 é a classe de armadilha que torna FFI cara: o mecanismo de erro do C atravessa o modelo de
erro do Rust, e testar errado dá falso verde.

# O que ficou deliberadamente de fora

Wire binário, os demais operadores, os casts adicionais e as opclasses — tudo isso ficou para a
implementação real. **O spike responde à pergunta de viabilidade e para**, que é a disciplina que o
distingue de um começo de implementação.

# Nota de licença

O código do spike é **original**. A técnica de manipulação de varlena foi aprendida de fontes
**permissivas**. Uma implementação de referência conhecida é **AGPL** e foi **apenas estudada, nunca
copiada** — a mesma política de [auditoria de licenças](/references/license-audit.md).
