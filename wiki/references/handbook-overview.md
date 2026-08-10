---
type: Reference
title: Handbook de engenharia — o currículo técnico interno
description: Ensina engenharia de banco através do sistema real do projeto, com um padrão de cinco camadas e um contrato de honestidade que decide se um capítulo pertence ao coração ou ao roadmap.
resource: git:f7c7b93:docs/handbook/README.md
tags: [referencia, handbook, formacao, curriculo, honestidade, metodo]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: hbook
    resource: git:f7c7b93:docs/handbook/README.md
    title: Formação de Engenharia — TheoDB
---

Currículo técnico interno que ensina engenharia de banco de dados **através do sistema que está sendo
construído de verdade** — cada conceito aterrissa em código real, numa decisão registrada, num benchmark
reproduzível e no estado da arte **com o gap honesto**.

# O que ele deliberadamente não é

Existem livros excelentes sobre álgebra linear, algoritmos, internals do PostgreSQL e os papers seminais.
**Não os reescrevemos** — seríamos piores que os originais.

O que **nenhum** deles tem é este sistema: as decisões registradas, os blueprints de investigação, os
artefatos de benchmark e o código próprio implementando tipo vetorial, índices, SIMD, quantização e
superfície de IA. **Esse é o fosso.**

Um capítulo que vai do paper original até o arquivo de implementação, até o benchmark medido, até o gap
honesto contra o SOTA — isso **ninguém fora do projeto consegue escrever**.

# Os três modos

| Modo | O que faz |
|---|---|
| **Curado** | trilha de leitura anotada às fontes canônicas, mais o "por que isto importa aqui". Aterrissa, não reproduz |
| **Original** | o coração: cada capítulo ancorado em código, decisão e benchmark reais |
| **Roadmap** | marcado honestamente como aposta futura, não como fato |

# O contrato de honestidade

1. **Toda citação de código resolve no disco.** Zero citações alucinadas — se uma referência não existe,
   ela não entra.
2. **Todo número de performance vem de artefato reproduzível**, com hardware e comando de reprodução.
3. **Gaps são explícitos.** Onde o projeto perde para o SOTA, o livro **diz com o número**, não esconde.
4. **Aspiracional é marcado como aspiracional.**

# O padrão de cinco camadas

```
1. TEORIA               — o conceito, o paper seminal, a intuição
2. MATEMÁTICA           — as fórmulas, a complexidade de build e de query
3. NOSSA IMPLEMENTAÇÃO  — o código real, com arquivo e linha, e as decisões
4. NOSSO BENCHMARK      — os números medidos, com hardware e reprodução
5. SOTA & GAP           — como o estado da arte faz, onde ganhamos, onde perdemos
```

**A regra de corte é elegante:** se um capítulo **não consegue preencher a camada 3**, ele pertence ao
roadmap, não ao coração. Isso é o que impede o livro de virar prosa sobre coisas não construídas.

# Estado

O [capítulo sobre HNSW](/references/handbook-19-hnsw.md) está escrito e é o **template de qualidade** —
todo capítulo original deve alcançar aquele nível de aterrissagem. Os demais têm índice definido e são
escritos **um por vez**.

A justificativa para crescer capítulo a capítulo, e não num despejo único, é a mesma disciplina do resto
do projeto: um despejo seria raso e alucinado.

# A mecanização

Um validador de citações **mecaniza o contrato de honestidade** — toda citação precisa resolver no disco,
todo número precisa de benchmark ou da marca explícita de não medido, e toda URL precisa estar na lista
permitida. **Um capítulo só é dado como pronto com o validador passando.**

É a mesma ideia dos gates estruturais que o resto do repositório usa: uma regra que ninguém precisa
lembrar de seguir, porque a ferramenta a exige.
