---
type: Technology
title: HNSW
description: O grafo navegável de pequeno mundo hierárquico — o algoritmo ANN mais usado da indústria, e o índice vetorial default do TheoDB, escolhido por evidência.
resource: https://arxiv.org/abs/1603.09320
tags: [tecnologia, ann, grafo, algoritmo, indice]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: hnsw-paper
    resource: https://arxiv.org/abs/1603.09320
    title: Malkov & Yashunin, Efficient and Robust ANN Search Using HNSW Graphs
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O HNSW é um algoritmo de busca aproximada de vizinhos mais próximos baseado num **grafo de proximidade em
camadas**, análogo a uma skip list: a camada base contém todos os nós, e cada camada acima contém
exponencialmente menos.[^hnsw-paper] A busca desce do topo — poucos nós, saltos longos — refinando até a
base, o que remove os mínimos locais da busca gulosa e dá complexidade logarítmica.

A explicação completa, com a matemática de atribuição de camadas e a implementação página a página, está
no [capítulo do handbook](/references/handbook-19-hnsw.md).

# Papel neste acervo

**É o índice default**, escolhido **por evidência** em
[decisão de índice](/decisions/m2-index-decision.md) — venceu a alternativa em recall, throughput, tempo
de build e, em baixa dimensão, tamanho.

O projeto tem **implementação própria** dele
([ADR 0010](/decisions/0010-m26-index-am-scope.md)), com persistência page-native e travessia sob demanda
([m35](/benchmarks/m35-hnsw-structured-scan.md)), e a superfície de uso está em
[índice HNSW](/features/02-indice-hnsw.md).

# O que o projeto aprendeu sobre ele

**A navegabilidade é uma propriedade distinta da conectividade.** Uma análise estrutural mostrou grafo
com out-degree cheio e **100% das misses por roteamento** — o que levou a ativar o mecanismo de
`extendCandidates` do paper original, recomendado para dados muito clusterizados
([ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md)).

**Ele tem localidade de acesso, e isso derruba uma intuição comum sobre quantização.** Como uma query
toca poucos nós, o índice em precisão plena **não thrasha** mesmo excedendo a RAM — o que falsificou a
tese de que comprimir traria QPS ([ADR 0018](/decisions/0018-m57-sbq-inline-not-superior.md)).

**Ele é o carrier errado para quantização.** O walk é pointer-chasing, e o ganho de quantização exige
scan em lote contíguo — conclusão do [ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md).

**Ele é O(N) em RAM para reconstruir**, o que é limite inerente e não defeito de implementação — a razão
de a manutenção usar tombstones no caminho comum
([ADR 0017](/decisions/0017-m55-index-maintenance-at-scale.md)).

[^hnsw-paper]: Malkov & Yashunin, HNSW
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
