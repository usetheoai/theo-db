---
type: Measurement
title: m122 — o embed assíncrono libera o horizonte de xmin
description: Duas evidências independentes — a prova pelo mecanismo no código-fonte e a observação com um endpoint lento deliberado — estabelecem que a correção é real, não no-op.
resource: git:f7c7b93:docs/benchmarks/m122-async-embed-xmin.md
tags: [benchmark, mvcc, xmin, vectorizer, prova-de-mecanismo, m122]
milestone: M122
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m122
    resource: git:f7c7b93:docs/benchmarks/m122-async-embed-xmin.md
    title: M122 — Async embed releases the xmin horizon
    last_modified: 2026-07-20
---

**Veredito: a divisão em três fases LIBERA o horizonte durante o embed; o caminho anterior o PRENDIA.
Correção real, não no-op.**

# As duas evidências, e por que ambas

**Prova de mecanismo, no código-fonte:** a API de transação usada empurra um snapshot ativo por todo o
corpo — e é *isso* que prendia o horizonte. Ver o mecanismo é definitivo, porque explica **por que** o
comportamento ocorre.

**Observação, com endpoint deliberadamente lento:** um endpoint mock que dorme 8 segundos torna a janela
grande o bastante para ser observada. Sem essa instrumentação, a janela seria curta demais para amostrar
com confiança.

**Construir a condição que torna o efeito observável** é diferente de esperar que ele apareça. E ter
mecanismo **e** observação protege contra os dois erros: uma medição que casa com o mecanismo errado, e
um mecanismo correto que não se manifesta.

# Por que importava

O horizonte prendido **atrasa o autovacuum local** pela duração inteira da chamada HTTP — que pode chegar
a ~90 s com endpoint travado. Não é sutil em produção.

# A divergência deliberada da SOTA

A implementação de referência do ecossistema **mantém a transação aberta através do embed, de
propósito** — o que é aceitável para um worker **externo**, e não para um **in-process**, cujo horizonte
gateia o autovacuum diretamente.

O racional completo, incluindo a recuperação de crash pelo menos-uma-vez e a ressalva prospectiva sobre
multi-worker, está no [ADR 0049](/decisions/0049-m122-three-phase-async-embed.md).
