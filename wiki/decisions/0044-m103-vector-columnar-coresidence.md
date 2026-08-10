---
type: Decision
title: ADR 0044 — Vetor e colunar num substrato só (co-residência inspirada no Lance)
description: Guardar o índice vetorial como colunas ao lado das analíticas permite compor prefiltro escalar, top-k e agregação num scan com column pruning — provado byte-idêntico à busca exata.
resource: git:f7c7b93:docs/adr/0044-m103-vector-columnar-coresidence.md
tags: [adr, vetor, columnar, co-residencia, column-pruning, filtered-ann, m103]
adr_id: "0044"
adr_status: Accepted
decision_date: 2026-07-16
milestone: M103
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0044
    resource: git:f7c7b93:docs/adr/0044-m103-vector-columnar-coresidence.md
    title: ADR 0044 — M103 vector + columnar in one substrate
    last_modified: 2026-07-16
---

# Contexto

O pilar embarcara o substrato colunar e os operadores de IA como access methods **separados** do
índice vetorial IVF e do caminho de ANN filtrado. A ideia de co-residência é guardar o índice
vetorial — id de partição IVF mais o vetor bruto — **como colunas**, ao lado das colunas
escalares e analíticas, de modo que um top-k vetorial prefiltrado por escalar e uma projeção
analítica **componham num único scan com column pruning**.

A novidade não é um índice novo: a máscara de linhas, o kernel exato de rerank com seu desempate e o
particionamento IVF **já existiam**. A novidade é o **layout co-residente mais a prova de identidade
byte a byte**.

# Decisão

Guardar o índice vetorial como colunas (`tid`, `part_id`, `label`, `vec`) co-residentes com as
colunas analíticas numa tabela do
[access method colunar próprio](/decisions/0042-m99-own-code-columnar-tam.md) — código próprio, com
o formato Lance servindo apenas como estudo de desenho, dado o portão de licença.

Uma função materializa a partição IVF real por linha; outra roda prefiltro escalar por máscara de
label → probe IVF → **rerank exato** → top-k, lendo **somente as 4 colunas de índice**.

## D1 — reusar o kernel e o desempate exatos, para identidade por construção

Tanto o top-k filtrado colunar quanto o oráculo de identidade rerankeiam com o **mesmo** kernel de
distância e ordenam com o **mesmo** comparador de desempate.

**Rejeitado:** um produto escalar f32 nativo do Arrow — ordem de somatório diferente produz drift de
último ULP, que muda o desempate sob empates de distância e quebraria o gate. Só o kernel e o
desempate idênticos garantem identidade.

## D2 — identidade byte a byte em probe total É a prova de equivalência de recall

Com probe total, o conjunto de candidatos é **toda linha mascarada**, então o top-k filtrado colunar
é byte-idêntico à força bruta filtrada exata — provando que o layout co-residente **não perde
recall**.

**Rejeitado:** comparar contra um índice vivo dentro do teste — o oráculo de força bruta exata é
afirmação **mais forte**, porque é o verdadeiro top-k, não concordância com outro caminho aproximado.

## D3 — o ganho de custo é o column pruning, isolado do confundidor

A busca ponta a ponta é **dominada pelo L2**: em probe total, o rerank percorre todas as linhas,
independentemente da largura do payload. Portanto **ela não consegue quantificar o ganho** — mostra
apenas que o pruning não adiciona custo dependente da largura (razão 1,014, dentro de um desvio).

**O controle isolado é que quantifica:** decodificar só as 4 colunas de índice leva **49,6 ms ± 0,3**,
contra **219,8 ms ± 1,8** decodificando todas — o pruning **economiza 77,4% do tempo de decode**, bem
acima do piso de ruído. O tamanho em disco (4,67× mais largo) é fato **separado**: tamanho em disco
não é custo de decode.[^adr0044]

## D4 — o teto honesto

O benchmark reporta column pruning e a composição de knn filtrado com agregação. O **recall é
declarado igual por construção** — é o gate, nunca um ganho — e **não há claim de QPS contra o
ScaNN**. Co-residência **não fecha o gap de paradigma**.

# Fronteira honesta

O comportamento out-of-RAM em escala de bilhão é **projeção honesta, não medido**. O caminho de probe
reduzido sobre colunar é exercitado in-memory. E como o Lance é um formato de arquivo, o que existe
aqui é um **índice materializado em side-store**, **não** um substituto do row-store transacional — a
consistência entre os dois stores, nesta fatia, é a de um snapshot estático, com manutenção
incremental registrada como follow-up.

[^adr0044]: ADR 0044 — M103: vector + columnar in one substrate (Lance-inspired co-residence)
