---
type: Decision
title: ADR 0063 — halfvec/sparsevec fora de escopo, e a versão do shim que dizia tê-los
description: A decisão pedida era sobre implementar os tipos. A medição encontrou algo mais barato e mais urgente: o shim declarava `vector 0.7.0`, que é a versão em que o pgvector introduziu halfvec, e uma app que checa `extversion >= '0.7.0'` recebia SIM e quebrava depois.
tags: [adr, pgvector, shim, compatibilidade, drop-in, honestidade, b-038]
adr_id: "0063"
adr_status: Accepted
decision_date: 2026-08-20
generated: { by: claude-code/opus-5, at: 2026-08-20T00:00:00Z }
---

Relacionado — o alias que completou a outra metade do shim:
[ADR-0058](0058-pgvector-compat-shim.md).

# Contexto

O [[B-038]] pediu uma decisão: implementar `halfvec`/`sparsevec` ou declará-los fora de escopo. O
item já dizia, com honestidade, que era *"o que menos dói e o mais caro"* — tipos novos com I/O
binário, operadores, opclasses e cast.

Medido em 2026-08-20 contra `ghcr.io/usetheoai/theo-db:latest`, os dois tipos de fato não existem.

# O que a medição encontrou, e que não era o que se procurava

```
SELECT extversion FROM pg_extension WHERE extname='vector';   ->  0.7.0
SELECT extversion >= '0.7.0';                                  ->  t
CREATE TABLE h (e halfvec(3));                                 ->  ERRO: tipo não existe
```

**`0.7.0` é a versão em que o pgvector introduziu `halfvec` e `sparsevec`.** Uma aplicação que faz a
checagem de capacidade padrão — *"a extensão é ≥ 0.7.0? então posso usar halfvec"* — recebia **sim**
e quebrava na primeira coluna.

Isso é pior do que não ter o tipo. O cabeçalho do próprio shim diz por quê:

> *"se por qualquer razão o tipo real não estiver presente, a app DEVE falhar aqui — alto, cedo e
> claro — em vez de acreditar que tem pgvector e quebrar depois, de forma obscura"*

O guard fail-fast protegia o tipo `vector`, e a versão declarada desmentia o guard.

## A causa é de numeração, e ela é generalizável

Os arquivos do shim numeravam a ordem em que **nós** construímos, não a linha do tempo do pgvector:
o alias `ivfflat` estava na "0.7.0" do shim, quando no pgvector real o `ivfflat` existe desde a
**0.1.0** e o `hnsw` desde a **0.5.0**.

**O número de versão de um shim de compatibilidade não é um número de build — é o contrato de
capacidade que a aplicação consulta.** Quando ele codifica a nossa cronologia, ele responde a
pergunta errada com confiança.

# Decisão

**`halfvec` e `sparsevec` ficam FORA DE ESCOPO**, e a versão declarada passa a ser **`0.6.0`**.

A superfície entregue — tipo `vector` próprio, mais os aliases `hnsw` e `ivfflat` — é a do pgvector
0.6.x, a última antes de `halfvec`/`sparsevec`. `0.6.0` é o número honesto, e declará-lo faz a
checagem de capacidade responder a verdade.

Verificado por execução: `extversion >= '0.7.0'` agora devolve **`f`**, e a superfície continua
inteira (dois AMs, seis opclasses).

Uma instalação já feita corrige a alegação sem ser recriada:
`ALTER EXTENSION vector UPDATE TO '0.6.0'` — provado contra o servidor, com a superfície preservada.

## Por que fora de escopo, e não implementar

`halfvec` é otimização de **memória**, não capacidade nova: metade dos bytes por vetor. Este produto
já tem quantização própria (RaBitQ, SQ8, PQ), que ataca o mesmo eixo por outro caminho e é o que o
[[ADR-0036]] mediu. Implementar `halfvec` compraria compatibilidade de sintaxe para um ganho que o
produto já oferece com nome diferente.

**Isso não é argumento para nunca fazer.** Se um caso de uso real aparecer — uma app que não pode
mudar o DDL, um caso de quantização do VectorDBBench que precisamos rodar —, o custo é conhecido e a
decisão se reabre. O que este ADR remove é o **silêncio**: hoje o produto responde
`ERROR: type "halfvec" does not exist` e ninguém tinha decidido dar essa resposta.

# Alternativas consideradas

**Implementar os dois tipos agora.** Rejeitada por custo contra benefício medido: tipos novos com
I/O binário compatível, operadores, opclasses nos dois AMs e casts, para um eixo que a quantização
própria já cobre.

**Manter `0.7.0` e documentar a lacuna.** Rejeitada porque documentação não é lida por
`extversion >= '0.7.0'`. A app não consulta a wiki; consulta o catálogo.

**Renumerar para um esquema próprio (ex.: `1.0.0`).** Rejeitada porque destrói a única coisa que o
número serve para fazer num shim: ser comparável com o do pgvector.

# Consequências

- A checagem de capacidade passa a responder a verdade, que é o que o guard do shim sempre quis.
- Uma app que precise de `halfvec` descobre **na hora do `CREATE EXTENSION`**, pela versão, em vez
  de na primeira coluna.
- O caminho de volta existe e foi testado, então imagens já lançadas não ficam presas à alegação.
- Se `halfvec`/`sparsevec` entrarem um dia, a versão sobe para `0.7.0` **junto com eles** — nunca
  antes. É a regra que este ADR estabelece e que a numeração anterior violava.
