---
type: Reference
title: Handbook capítulo 19 — HNSW, da teoria à página de 8 KB
description: O capítulo-farol do currículo interno; vai do paper à matemática de camadas, à implementação em duas camadas, ao benchmark e ao gap honesto.
resource: git:f7c7b93:docs/handbook/parte-06-vetorial/19-hnsw.md
tags: [referencia, handbook, hnsw, teoria, implementacao, page-native]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: hb19
    resource: git:f7c7b93:docs/handbook/parte-06-vetorial/19-hnsw.md
    title: Capítulo 19 — HNSW
---

O **capítulo-farol** do [handbook](/references/handbook-overview.md): o padrão de qualidade que todo
capítulo original deve alcançar.

# Teoria — de skip lists a grafos hierárquicos

A ideia do [HNSW](/technologies/hnsw.md) nasce de três blocos encadeados:

1. **Redes de pequeno mundo.** Grafos onde a distância média cresce como `log(N)` mesmo com poucas
   arestas por nó. Se cada vetor aponta para seus vizinhos próximos **e** há algumas arestas longas, uma
   busca gulosa chega perto do alvo em poucos saltos.
2. **NSW.** Aplica isso a busca aproximada, inserindo vetores um a um. As primeiras inserções viram
   naturalmente as arestas longas. **Problema:** a busca gulosa fica presa em mínimos locais, e o grau
   dos primeiros nós explode.
3. **HNSW.** A contribuição decisiva é **camadas**, exatamente como uma skip list. A base tem todos os
   nós; cada camada acima tem exponencialmente menos. A busca começa no topo — poucos nós, saltos longos,
   aproximação grosseira — e desce refinando. Isso remove os mínimos locais e dá o `O(log N)`.

```
Camada 2:   A ─────────────── E              ← poucos nós, saltos longos
            │                 │
Camada 1:   A ───── C ─────── E ───── G
            │       │         │       │
Camada 0:   A─B─C─D─E─F─G─H─I─J─K─L─M─N─O    ← TODOS os nós, busca fina
```

# Matemática

Cada nó recebe uma camada máxima sorteada de uma distribuição exponencial:

$$ l = \lfloor -\ln(U) \cdot m_L \rfloor, \quad U \sim \mathrm{Uniforme}(0,1], \quad m_L = \frac{1}{\ln M} $$

O fator $m_L = 1/\ln M$ é a escolha ótima do paper: faz o número esperado de camadas ser $\log_M N$ e
limita o grau médio. Com $M = 16$ e $N = 10^6$, isso dá **~5 a 6 camadas** — o "expresso" tem
pouquíssimos nós.

| Símbolo | Papel | Valor aqui |
|---|---|---|
| `M` | arestas por nó nas camadas superiores | 16 |
| `M₀ = 2M` | arestas na camada base — o dobro | 32 |
| `ef_construction` | lista de candidatos **no build** | 64 |
| `ef_search` | lista de candidatos **na query** | GUC, default 64 |

**`M` e `ef_construction` trocam qualidade de grafo por tempo de build; `ef_search` troca recall por
velocidade na consulta, sem rebuild** — é o botão que o usuário gira.

**Complexidade.** Busca: `O(ef · M · d)`, **independente de N** — e essa independência é a propriedade
que o benchmark persegue. Build: `O(N · log N · ef_construction · M · d)`, que é caro. Memória:
`O(N · (d + M))`.

# Implementação em duas camadas

**O grafo em memória** é a estrutura pura em Rust, sem PostgreSQL, testável isoladamente.

Um detalhe que o capítulo destaca como padrão pedagógico: há um **teto de nível que não existe no
paper**. É decisão de engenharia forçada pela persistência — a tupla de vizinhos precisa caber numa
página de 8 KB, e um nível astronomicamente alto (probabilidade praticamente zero em dados reais)
estouraria isso. A implementação de referência do ecossistema faz o mesmo.

> *"A teoria é limpa; a implementação carrega decisões que a teoria não vê."*

O gerador aleatório é **determinístico e semeado** — não criptográfico, mas garante que dois builds do
mesmo corpus produzam o mesmo grafo, o que é essencial para benchmarks reproduzíveis.

A **seleção de vizinhos** implementa a heurística do paper: mantém um candidato só se ele estiver mais
perto da consulta do que de qualquer vizinho já escolhido. Isso evita clusters redundantes e dá arestas
diversas — **melhor navegabilidade que simplesmente pegar os M mais próximos**. Essa mesma heurística é o
lugar onde o [ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md) depois interveio.

**A persistência page-native** responde à pergunta de engenharia real: *como percorrer o grafo lendo só
as páginas dos nós que a busca visita?*

A versão anterior serializava tudo num blob e **desserializava por completo a cada query** — O(N) por
consulta, o que a 1M significava gigabytes. O layout atual:

```
bloco 0      = meta: parâmetros, ponto de entrada, limites das faixas
blocos [1..] = element tuples (tamanho fixo): tag, nível, TID, ponteiro, dimensão, vetor
blocos [n..] = neighbor tuples: as arestas de todas as camadas, como ponteiros
```

**O empacotamento é uma decisão de desenho:** como o grafo inteiro está em memória no build, **todos os
endereços são calculáveis antes de qualquer I/O**. Element tuples de tamanho fixo tornam o endereço de um
nó **analítico**. O resultado é uma **única passada de escrita com WAL**, sem tupla placeholder e sem
sobrescrita de tupla — que a implementação de referência precisa porque constrói em disco. Menos
superfície de FFI, e o empacotador é testável **sem um PostgreSQL rodando**.

**A travessia sob demanda** lê a meta, desce as camadas superiores lendo uma página por passo, e na base
mantém os melhores num heap, com um conjunto de visitados garantindo que cada nó seja lido **no máximo
uma vez**. A pontuação é calculada **direto sobre os bytes da página**, sem materializar o vetor.

# Números e gap

O benchmark correspondente é [m35](/benchmarks/m35-hnsw-structured-scan.md), e o gap honesto contra o
estado da arte é o de [m33](/benchmarks/m33-scann-headtohead.md) — cuja interpretação correta, como o
capítulo insiste, está no veredito medido do
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md): **é gap de paradigma, não de tuning**.

A feature correspondente, do ponto de vista de uso, é o [índice HNSW](/features/02-indice-hnsw.md).
