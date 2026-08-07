---
type: Measurement
title: e2 — grafo quantizado co-localizado dentro do PostgreSQL: mais lento
description: Dois access methods sobre a MESMA tabela, com o access method como única variável — e o veredito é que o índice existente é 2,6–3,9× mais rápido a recall casado.
resource: git:f7c7b93:docs/benchmarks/e2-symqg-inpg-verdict.md
tags: [benchmark, symqg, honest-negative, clean-room, isolamento, sift1m]
dataset: SIFT1M
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: e2inpg
    resource: git:f7c7b93:docs/benchmarks/e2-symqg-inpg-verdict.md
    title: E2 — SymphonyQG in-PostgreSQL verdict
    last_modified: 2026-07-18
---

# O isolamento

Os **mesmos** vetores indexados por **dois access methods sobre a MESMA tabela**, com as **mesmas**
queries oficiais e o **mesmo** ground truth — **a única variável é o access method**.

# O veredito

**O índice de grafo existente é 2,6–3,9× mais rápido a recall casado**, na faixa prática de recall. **O
gate de superioridade não foi atingido.**

E a lição de método por trás disso é a mesma que a linhagem vetorial aprendeu várias vezes: o
[spike fora do banco](/benchmarks/e2-symqg-spike.md) medira 1,8–2,66× **a favor** — **e isso não
transferiu** para dentro do PostgreSQL.

**Ganho medido in-memory frequentemente não sobrevive ao imposto de página, WAL e MVCC** — exatamente o
que o [ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md) documentou e o
[dossiê de pesquisa](/references/scann-storage-separation-2026-07.md) quantificou como 84% dos ciclos.

# Licença

Implementação **clean-room** a partir do paper. A referência em C++ tem licença restritiva e foi
**estudo apenas, nunca copiada** — a mesma disciplina registrada em
[auditoria de licenças](/references/license-audit.md).

# Consequência para o produto

O índice permanece disponível como **alternativa experimental**, com a orientação explícita de usar o
outro como default — ver [índice SymQG](/features/17-indice-symqg.md).
