---
type: Reference
title: Empacotamento, extensões e tuning — e a suíte de regressão do upstream
description: A distribuição usa o binário PostgreSQL não modificado, e a prova disso é passar 100% da suíte de regressão oficial — o gate que torna a wire-compatibility verificável em vez de alegada.
resource: git:f7c7b93:docs/packaging/packaging-and-tuning.md
tags: [referencia, empacotamento, regressao, wire-compat, tuning, ci]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: packtune
    resource: git:f7c7b93:docs/packaging/packaging-and-tuning.md
    title: Packaging, extensions & tuning
---

> **Nota de contexto histórico.** A tabela de extensões pré-instaladas descrita na origem envelheceu — o
> [pgvector](/technologies/pgvector.md), o [pgvectorscale](/technologies/pgvectorscale.md) e o
> `plpython3u` foram removidos depois ([ADR 0029](/decisions/0029-m70-drop-pgvector.md)). **O que
> permanece válido, e é o essencial, é o método de prova.**

O TheoDB é uma **distribuição compatível com PostgreSQL**: o engine é o binário oficial **não
modificado**, empacotado num container — o invariante do
[ADR 0001](/decisions/0001-no-engine-fork.md).

# A prova que torna a compatibilidade verificável

A distribuição passa **100% da suíte de regressão CORE do PostgreSQL upstream**:

```
# All 225 tests passed.
```

E o modo como isso é produzido é o que dá peso ao número:

```bash
docker build -f packaging/Dockerfile.regress -t theo-db-regress .
docker run --rm theo-db-regress
```

A imagem de teste parte **da própria distribuição**, então **o engine sob teste é o engine que embarca**.
O código-fonte da suíte vem da tag exata correspondente, configurada com a mesma superfície de features
para que as saídas esperadas batam.

**Como o engine não é forkado, uma suíte verde confirma que o reempacotamento não regrediu o core.** É
essa diferença — testar o artefato, não uma aproximação dele — que separa "wire-compatible" de vibe.
A imagem de regressão é descartável e nunca embarca.

# Tuning conjunto

As extensões coexistem sem conflito, por viverem em namespaces e access methods separados. As linhas de
base recomendadas:

- **Busca vetorial:** o índice default é o [HNSW](/features/02-indice-hnsw.md), decidido por evidência em
  [decisão de índice](/decisions/m2-index-decision.md). O knob de recall é em tempo de query, e subir
  `maintenance_work_mem` acelera a construção.
- **Embeddings:** o endpoint do modelo vai por GUC, conforme
  [embeddings a partir do SQL](/guides/sql-embeddings.md), e **a chamada é síncrona** — trabalhos grandes
  vão em lote, fora de uma única instrução
  ([ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)).

# O que o tuning mudou desde então

Dois pontos da orientação original foram **superados por medição**, e vale saber qual é qual:

- O **default de `ef_search`** costuma ser baixo demais para produção; o caminho correto hoje é o
  recomendador determinístico do [ADR 0026](/decisions/0026-m67-autotune-recommender.md), em vez de
  tentativa e erro.
- Os knobs da extensão de terceiro que a origem cita **não existem mais** — os access methods são
  próprios, e seus parâmetros estão documentados em [HNSW](/features/02-indice-hnsw.md) e
  [IVFFlat](/features/03-indice-ivfflat.md).

Para diagnosticar em produção, o playbook é o
[runbook de diagnóstico vetorial](/runbooks/vector-scan-diagnostics.md).

# Relacionado

A auditoria de licença que acompanha o pacote está em
[auditoria de licenças](/references/license-audit.md), e a disciplina de cadeia de upgrade em
[m137](/benchmarks/m137-upgrade-chain.md).
