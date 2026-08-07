---
type: Measurement
title: m168 — decode proporcional a k no top-k, e a recusa de recoletar
description: Prova que as mudanças posteriores foram exclusivamente de comentário — com o comando que verifica isso — e argumenta que uma sétima coleta trocaria os números sem responder pergunta nenhuma.
resource: git:f7c7b93:docs/benchmarks/m168-streaming-topk-verdict.md
tags: [benchmark, columnar, top-k, proveniencia, decisao-de-medicao, m168]
milestone: M168
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m168
    resource: git:f7c7b93:docs/benchmarks/m168-streaming-topk-verdict.md
    title: M168 — decode O(k) para o top-k de projeção
    last_modified: 2026-07-29
---

Fecha um item pendente do milestone anterior **e um falso-admit medido** — ou seja, um caso em que o
roteamento aceitava algo que não deveria.

# A decisão de medição mais interessante do repositório

Os artefatos foram coletados num commit específico. As mudanças posteriores no código são
**exclusivamente de comentário** — e o documento **dá o comando que verifica isso**, mostrando que o
filtro por linhas não-comentário volta vazio.

Conclusão registrada:

> Nenhuma linha de código mudou, então **nenhum número aqui depende de uma recoleta**. Uma sétima coleta
> apenas **trocaria todos os números de novo sem responder pergunta nenhuma**.

**Isso é rigor, não preguiça** — e a diferença está em ser **verificável**. A alegação "nada mudou" é
comum e frequentemente falsa; aqui ela vem com o comando que qualquer um roda.

E o argumento de fundo é correto: numa máquina com variância, **recoletar troca os números por outros
igualmente válidos**. Se a pergunta é "o código mudou?", a resposta vem do diff, não de uma nova
execução.

# A proveniência

Cada artefato carrega o **hash do binário** no cabeçalho, tornando a associação entre número e build
verificável em vez de declarada — o mesmo cuidado que
[m167](/benchmarks/m167-projection-topk-verdict.md) levou a extremos.
