---
type: Decision
title: ADR 0034 — HNSW build: extendCandidates ligado por padrão para fechar a degradação de recall por escala
description: A análise white-box mostrou que 100% das misses eram roteamento, não conectividade; estender o pool com vizinhos-dos-vizinhos subiu o recall de 0,974 para 0,990 a 500k.
resource: git:f7c7b93:docs/adr/0034-hnsw-extend-candidates-navigability.md
tags: [adr, hnsw, recall, navegabilidade, build, white-box]
adr_id: "0034"
adr_status: Accepted
decision_date: 2026-07-10
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0034
    resource: git:f7c7b93:docs/adr/0034-hnsw-extend-candidates-navigability.md
    title: ADR-0034 — HNSW build extendCandidates
    last_modified: 2026-07-10
---

O momento em que a investigação trocou de método — e o método novo achou a causa que sete tentativas
black-box não acharam.

# Contexto medido

O `theodb_hnsw` degradava recall com a escala: `recall@10` de 0,998 a 100k caía para **0,974 a
500k**, contra 0,988 do pgvector. **Sete alavancas black-box foram refutadas.**

A virada foi trocar para **white-box** — um analisador que mede a *estrutura* do grafo. O resultado:
conectividade perfeita, com out-degree cheio, mas **100% das misses são de ROTEAMENTO**, e a
distância em hops cresce com a escala (5 → 7,3 → 9,6). Um grafo bem conectado e **mal navegável**.

A causa tem base no paper original do [HNSW](/technologies/hnsw.md): faltava o **`extendCandidates`**
do algoritmo de inserção, recomendado justamente para dados **extremamente clusterizados** — que é
exatamente o regime medido, com 256 clusters gaussianos.

# Decisão

Estender o pool de candidatos com os **vizinhos dos vizinhos**, na mesma camada, antes da seleção —
nos dois caminhos de build, sequencial e paralelo. Tunável por variável de ambiente, **ligado por
padrão**, com opt-out que devolve um build 2 a 3× mais rápido.

# Evidência

A 500k × 768d ([gap1](/benchmarks/gap1-extend-candidates.md)):

- **Recall f32 de 0,974 para 0,990; SBQ de 0,986 para 0,994** — paridade de **valor** de recall com
  o pgvector (0,994). A curva inteira subiu ~5 pontos, e o caminho f32 **passa a alcançar ≥0,99 a
  500k**, onde antes platôava.

**A ressalva honesta que o ADR faz questão de registrar:** isto **não é paridade de fronteira**. O
pgvector ainda tem recall maior **no mesmo `ef`** (com `ef=200`: 0,988 contra 0,952), o que
significa que, a iso-recall, o TheoDB é ~1,8× mais lento. **O fix sobe o teto; não iguala a
eficiência de recall por `ef`.**[^adr0034]

# Alternativas rejeitadas

**Manter sem o extend** — deixaria o recall platôando abaixo do pgvector, no eixo em que estávamos
atrás. **Default desligado** — não entregaria o ganho por padrão; como qualidade de recall importa
mais que velocidade de build na maioria dos workloads vetoriais, o default é ligado com escape.
**Mexer só no query-time** — refutado: o defeito é do **build**, isto é, da estrutura do grafo, não
da busca.

# Consequências

Recall classe-pgvector a 500k, fechando a degradação por escala, e tunável.

**Custos:** build 2 a 3× mais lento, o que corrói a vantagem de velocidade de build — mitigado pelo
opt-out. E a fronteira de latência continua com o pgvector, a ~1,8× iso-recall. O follow-up
identificado é refinar a diversidade da seleção de vizinhos, com precedente em implementações
permissivas.

[^adr0034]: ADR-0034 — HNSW build: extendCandidates (default ON)
