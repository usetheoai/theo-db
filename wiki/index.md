---
okf_version: "0.2"
---

# TheoDB — acervo de conhecimento

Este bundle é a documentação do **TheoDB** convertida para [Open Knowledge Format](https://github.com/google/open-knowledge-format): um banco de dados open-source compatível com PostgreSQL, entregue como extensão, com capacidades vetoriais, de IA, colunares, lexicais e de grafo.

**282 conceitos**, derivados dos 264 documentos da antiga árvore `docs/` mais a decomposição das entidades que eles nomeiam.

Este bundle é a **documentação viva do projeto** — a árvore `docs/` que o originou foi removida do repositório, e permanece recuperável no histórico git em `f7c7b93`. Cada conceito registra sua origem no campo `resource`, na forma `git:f7c7b93:docs/…`.

## Por onde começar

Se você quer **usar** o banco, comece pelo [quickstart](guides/quickstart.md) e depois pelas [features](features/index.md).
Se você quer **entender por que ele é assim**, comece pelas [decisões](decisions/index.md) — em particular a [virada estratégica](decisions/0006-own-code-postgres-based-rust-go.md) e o [veredito do pilar vetorial](decisions/0035-m73-northstar-vector-verdict.md).
Se você quer **avaliar as alegações**, todo número vive em [benchmarks](benchmarks/index.md), e nenhuma afirmação de performance existe sem artefato.

## O que dá caráter a este acervo

Ele registra **tanto o que funcionou quanto o que foi refutado**, com o mesmo cuidado. Uma parte grande dos artefatos são *honest-negatives*: hipóteses do próprio projeto que a medição derrubou — [quantização não traz QPS](decisions/0018-m57-sbq-inline-not-superior.md), [o quantizador não era o gargalo](benchmarks/m40-ceiling-probe.md), [o rerank degradou a qualidade](benchmarks/archive/m65-rerank.md), [o grafo perde para o vetor na tarefa que motivou o pilar](benchmarks/archive/m111-m112-graphrag-retrieval.md).

Há também **retratações preservadas** — um [veredito de superioridade](benchmarks/sift1m-carrier-verdict.md) que não sobreviveu a medição rigorosa, e [números invalidados por dados degenerados](decisions/0012-benchmark-data-degeneracy.md) — mantidos com o aviso no topo, porque apagá-los esconderia que foram citados.

# Decisões

Sessenta registros de decisão de arquitetura, do [invariante de não forkar o engine](decisions/0001-no-engine-fork.md) ao [custo honesto de um fail-open](decisions/0059-m169-fail-open-cobre-falha-de-spill.md).

* [Índice completo](decisions/index.md)

# Features

Dezenove capacidades do produto, com o que é entregue, o que é API-alvo, e as ressalvas medidas.

* [Índice completo](features/index.md)

# Benchmarks

Cento e sessenta e nove medições, com método, números, vereditos e limites declarados.

* [Índice completo](benchmarks/index.md)

# Guias e operação

* [Guias](guides/index.md) — quickstart, self-host, embeddings, funções de IA, migrações
* [Runbooks](runbooks/index.md) — diagnóstico de recall baixo e latência alta

# Referências e entidades

* [Referências](references/index.md) — handbook, pesquisa, spikes, empacotamento, segurança
* [Tecnologias](technologies/index.md) — as peças que os conceitos nomeiam, explicadas no contexto deste projeto
* [Glossário](glossary.md) — os termos recorrentes

# Proveniência

* [log.md](log.md) — histórico de mudanças deste bundle, incluindo o que ficou de fora e por quê
