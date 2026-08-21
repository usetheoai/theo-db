---
type: Measurement
title: b007 — o CSR de grafo PERDE para SQL recursivo até 2 saltos, e só ganha do 3º em diante
description: Primeira medição do pilar de grafo contra qualquer baseline. Travessia de 1, 2 e 3 saltos pelo CSR e por WITH RECURSIVE indexado, no mesmo servidor, mesma tabela, mesmo MVCC. O cruzamento fica entre 2 e 3 saltos.
tags: [grafo, csr, baseline, latencia, honest-negative, b-007]
item: B-007
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Peças: [b043](b043-teto-lexical-e-o-cliente.md) — a retratação que ensinou a desconfiar de um número
antes de publicá-lo. O baseline segue o contrato de `cycle-discover`: é "o que o usuário faria sem
nós" (regra fora do bundle, em `.claude/rules/`).

# Por que esta medição existe

O [[B-007]] registrava **23 funções de grafo no binário default e 35 testes** — a maior superfície
pública do projeto depois do vetorial — e **nenhum artefato comparando o pilar com coisa alguma**.
Qualquer afirmação sobre o grafo, em qualquer direção, era sem lastro.

O baseline não é outro banco de grafo. É `WITH RECURSIVE` no próprio PostgreSQL, porque a pergunta
que o usuário tem é *vale a pena instalar isto?* — e não a de quem já decidiu adotar um banco de
grafo. Ele roda na **mesma tabela de arestas, no mesmo servidor, pagando o mesmo MVCC**.

# O que foi medido

Grafo sintético, 200.000 vértices, grau médio 8 (1.599.990 arestas), 100 fontes semeadas,
3 repetições, um cliente, laço fechado. TheoDB `ghcr.io/usetheoai/theo-db:develop` em contêiner.
Suíte `graph/synthetic/vs-recursive-sql`, run `20260821T142720Z-…-e65961cc`.

## Consulta — mediana do p50 sobre as repetições

| travessia | CSR | `WITH RECURSIVE` | razão |
|---|---|---|---|
| 1 salto | 1,281 ms | **0,589 ms** | **SQL 2,17× mais rápido** |
| 2 saltos | 1,565 ms | **0,825 ms** | **SQL 1,90× mais rápido** |
| 3 saltos | **2,396 ms** | 5,419 ms | CSR 2,26× mais rápido |

## Construção e espaço

| | tempo | espaço |
|---|---|---|
| CSR (`theodb.graph_build`) | 1,08 s | **14,4 MB** |
| Índices B-tree que o baseline precisa | 0,63 s | 30 MB |

A tabela de arestas (68 MB de heap) é paga pelos dois lados e não entra na conta.

# O veredito, incluindo a parte desfavorável

**O CSR só compensa a partir de 3 saltos.** Até 2 saltos, um `WITH RECURSIVE` sobre índices B-tree
comuns é aproximadamente o dobro mais rápido. Isso é coerente com o que as duas estruturas fazem: a
1 salto o trabalho é um index range scan que o Postgres faz muito bem, e o CSR paga overhead de
travessia para pouca profundidade; a partir de 3 saltos a junção recursiva multiplica tuplas e o
layout contíguo passa a valer.

O que o CSR ganha em espaço é real e independe da profundidade: **2,1× menor** que os índices que o
baseline exige, custando 1,7× mais tempo para construir.

**Não é permitido dizer "travessia de grafo mais rápida"** sem qualificar a profundidade. Até 2
saltos a frase é falsa, e está medida.

# Limites desta medição — declarados, não omitidos

- **Grafo uniforme aleatório.** Grafos reais são lei de potência, onde travessias profundas explodem
  e a vantagem do CSR *provavelmente* cresce. Isso é hipótese, **não medido aqui**, e não entra em
  nenhuma alegação.
- **Um cliente, laço fechado.** Isto é latência, não vazão. Nada aqui diz o que acontece sob
  concorrência — e o [[B-043]] é o precedente de quanto essa confusão custa.
- **Contêiner na máquina de desenvolvimento**, não droplet isolado. Os valores absolutos são do
  ambiente; a razão é a parte transferível.
- Um só grau médio (8). O sweep de fanout existe no arnês e não foi rodado aqui.

# Quatro defeitos do arnês que esta medição encontrou primeiro

Nenhum era do produto, e cada um teria produzido um número publicado e errado.

1. **Os dois lados mediam coisas diferentes.** O CSR é não-dirigido e inclui a semente
   (`theodb_rs/src/graph.rs:44` e `:429`); o oráculo era dirigido e a excluía. Para a fonte 1048:
   oráculo 8 vértices, CSR 22. A razão entre esses tempos não teria referente.
2. **`GraphSpec.directed` era aceito sem efeito** — só o adapter fake o lia. O `PostgresAdapter`
   agora **recusa** `directed=True` em vez de ignorar o pedido.
3. **O baseline não tinha índice.** Sem ele, cada passo recursivo fazia seq scan de 1,6 M arestas e
   uma consulta de 3 saltos levava 18 s. Comparar contra isso seria propaganda, não medição.
4. **Sem aquecimento, quem rodava primeiro pagava.** O p50 de 1 salto saía *maior* que o de 2 — mais
   trabalho custando menos. Invertendo a ordem, o efeito seguiu a ordem. A frio, o SQL parecia
   **6,2×** mais rápido a 1 salto; a quente são **2,17×**.

E um quinto, de contabilidade: `structure_bytes` reportava `pg_relation_size` da **tabela de
arestas** (71 MB) como se fosse o tamanho do CSR (14,4 MB) — inflando 5× nosso próprio custo de
memória, contra nós.
