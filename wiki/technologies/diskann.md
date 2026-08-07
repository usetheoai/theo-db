---
type: Technology
title: DiskANN
description: A família de índices ANN projetada para grafos residentes em disco em escala de bilhão; foi o substituto permissivo de qualidade-ScaNN do projeto, com envelope de projeto declarado.
resource: https://github.com/microsoft/DiskANN
tags: [tecnologia, ann, grafo, disco, escala]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: diskann-repo
    resource: https://github.com/microsoft/DiskANN
    title: DiskANN, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O DiskANN é uma família de algoritmos ANN baseados em grafo, projetada para índices que **não cabem em
memória**: o grafo fica em disco, e uma representação comprimida em memória guia a navegação, com o
refinamento lendo os vetores completos apenas para os candidatos finais.[^recalled] A construção do grafo
usa uma poda que preserva alcançabilidade com grau limitado.

# Papel neste acervo — e a lição sobre envelope de projeto

Foi o índice que o [ADR 0004](/decisions/0004-scann-fork-decision.md) adotou como **substituto permissivo
de qualidade-ScaNN**, evitando construir um índice nativo antes de haver evidência.

Mas a história dele aqui é sobretudo uma lição sobre **medir dentro do envelope de projeto**:

- No [primeiro benchmark](/benchmarks/m14-scann-fork-decision.md), ele **atravessou a barra de recall** —
  mas em dados sintéticos de baixa dimensão, e a decisão ficou **provisional** por isso.
- Sobre [dados reais de baixa dimensão](/benchmarks/archive/2026-06-27-glove-25-angular.md), ele **perdeu
  em todos os eixos**, inclusive tamanho — e a vantagem de compressão vista antes **desapareceu**,
  revelando-se artefato de alta dimensionalidade.

A conclusão registrada em [decisão de índice](/decisions/m2-index-decision.md) é a que importa:

> **A proposta de valor do DiskANN exige AMBOS — alta dimensionalidade E grande escala.** Nenhum dos dois
> benchmarks estava no envelope de projeto dele.

**Medir uma técnica fora do regime para o qual ela foi desenhada produz um veredito verdadeiro e
irrelevante.** Reconhecer isso — em vez de concluir "o DiskANN é pior" — é o que torna a decisão honesta:
o índice permaneceu **disponível e documentado** para o regime dele, com a superioridade lá declarada
**não medida** pelo projeto.

# Situação atual

Saiu da distribuição junto com a extensão que o provia
([ADR 0029](/decisions/0029-m70-drop-pgvector.md)).

[^diskann-repo]: DiskANN, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
