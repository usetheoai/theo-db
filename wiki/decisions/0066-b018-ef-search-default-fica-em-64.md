---
type: Decision
title: ADR-0066 — o default de `theodb_hnsw.ef_search` fica em 64, e a saída para o caso de junção é por consulta
description: Baixar para 40 compra 1,377× de QPS e custa 7,02 pontos de recall@10, medido. O pgvector paga o mesmo preço pelo mesmo ganho — é a mesma troca com os dois projetos em lados opostos.
tags: [vetorial, hnsw, planner, default, b-018, decisao-medida]
item: B-018
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Medição que sustenta esta decisão:
[b018 — o planner larga o HNSW na junção](../benchmarks/b018-planner-hnsw-juncao.md). Peça:
[pgrx](../technologies/pgrx.md).

# Contexto

O [[B-018]] reproduziu: numa junção com **filtro seletivo na tabela juntada**, a ordem de junção
inverte, `embeddings` deixa de dirigir, e o HNSW não pode mais servir a ordenação — aparece um `Sort`.
Com `enable_sort = off` o plano não muda (`Disabled: true`): não há caminho alternativo a gerar.

A causa não é a nossa implementação. Em `ef_search = 64` o **pgvector 0.8.6 produz plano e custos
idênticos aos nossos** — 567,66 / 559,36 / 560,45 / 552,13, número por número. No mesmo `ef`, nosso
index scan custa 425,60 contra 469,68 dele, e nosso índice ocupa 680 páginas contra 751. Somos mais
baratos e menores nos dois eixos. A diferença inteira é o **default**: 64 nosso (herdado do `SCAN_EF`
fixo pré-M35, `am/guc.rs:22`), 40 dele.

# Decisão

**O default fica em 64.** A saída para o caso de junção é `SET LOCAL theodb_hnsw.ef_search` **por
consulta**, não uma mudança global.

## O que a medição disse

SIFT1M completo, `theodb_hnsw` m=16, k=10, 500 consultas, 3 repetições, droplet `g-16vcpu-64gb`
(nyc3), benchmark registrado `vector/sift1m/ef-default`:

| `ef_search` | QPS | IC95 | recall@10 |
|---|---|---|---|
| 40 | 901,2 | [884,6, 917,7] | **0,8316** |
| **64** | 654,4 | [648,0, 660,8] | **0,9018** |

- **Ganho de baixar:** 1,377× de QPS (IC95 [1,355×, 1,402×], p = 0,0003).
- **Custo de baixar:** **7,02 pontos de recall@10**, queda relativa de 7,8%.

O recall é determinístico (CV 2e-16) — índice fixo, consultas fixas, `ef` fixo dão o mesmo conjunto.
O que varia é só o QPS, com CV de 1,6% e 0,86%.

## Por que 7 pontos são caros aqui e não em qualquer lugar

O pilar vetorial deste projeto sustenta uma alegação específica: **paridade de recall classe-pgvector**
(M60/M69/M70, veredito medido do M73). Sete pontos de recall@10 não são um ajuste de tuning — são a
alegação. Trocá-los por um caso de escolha de plano contradiz o que o pilar inteiro afirma.

Um produto cuja promessa fosse latência acima de qualidade tomaria a decisão oposta com a mesma
medição, e estaria certo. A decisão é do produto, não do número.

## O argumento que fecha: o pgvector paga o mesmo preço

Em `ef_search = 64` o pgvector também larga o índice. O default de 40 dele compra o plano com os
**mesmos** 7 pontos de recall, no índice dele. **Não são duas escolhas de engenharia diferentes; é a
mesma troca, com os dois projetos em lados opostos.**

Isso importa porque desfaz a leitura fácil — "o pgvector escolhe o índice e nós não, logo somos
piores". A comparação honesta é: nós entregamos mais recall por default e ele entrega mais QPS por
default, e cada um pode virar para o lado do outro com uma linha.

# Alternativas descartadas

- **Baixar o default para 40** — descartada pela medição acima. Sete pontos de recall.
- **Inflar o custo do plano concorrente, ou desinflar o do index scan, em `am/cost.rs`** — seria mentir
  ao planner sobre um custo que é verdadeiro. E o `am/cost.rs` é port fiel do `hnsw.c` do pgvector 0.8;
  divergir dele para forçar um plano trocaria um modelo auditável por um ajustado ao caso.
- **Gerar um caminho que preserve o pathkey com `embeddings` no lado interno** — não existe: um scan
  ordenado só serve a ordenação quando sua relação dirige, e isso é do PostgreSQL, não nosso.

# Consequências

- O caso de junção com filtro seletivo **continua existindo** e está documentado com a saída por
  consulta. Não é um defeito escondido; é uma troca registrada.
- `SET LOCAL` é a forma correta — escopo de transação, o que o [[B-055]] já estabelece como necessário
  sob *transaction pooling*.
- O benchmark `vector/sift1m/ef-default` fica registrado no arnês, então a troca é re-medível quando
  o grafo ou o quantizador mudarem. Uma decisão baseada em número só continua válida enquanto o número
  continuar valendo.
