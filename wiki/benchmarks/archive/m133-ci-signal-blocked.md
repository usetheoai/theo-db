---
type: Measurement
title: m133 — por que o sinal de CI estava morto: evidência primária, não inferência
description: O primeiro diagnóstico era inferência plausível; a causa verbatim veio de consultar a API do próprio serviço — a diferença entre supor e verificar.
resource: git:f7c7b93:docs/benchmarks/archive/m133-ci-signal-blocked.md
tags: [benchmark, ci, diagnostico, evidencia-primaria, arquivo, m133]
milestone: M133
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m133
    resource: git:f7c7b93:docs/benchmarks/archive/m133-ci-signal-blocked.md
    title: M133 — por que o sinal de CI está morto
    last_modified: 2026-07-21
---

# A distinção que dá título ao artefato

> O primeiro diagnóstico era **inferência**. A causa verbatim vem da anotação do próprio serviço.

O raciocínio inicial — "não há log, logo provavelmente é cobrança" — era **plausível e não verificado**.
A causa real foi obtida **consultando a API do serviço** e lendo a mensagem literal, que confirmava a
suspeita.

**O ponto não é que a inferência estava errada — ela estava certa.** O ponto é que **uma inferência
certa e uma evidência primária não têm o mesmo valor**, e agir sobre a primeira quando a segunda está a
uma chamada de distância é aceitar risco sem necessidade.

# Por que registrar isso

Um sinal de CI morto é o pior tipo de falha silenciosa: **os gates continuam existindo no papel e param
de rodar**. Todo o aparato de qualidade que o repositório mantém — gates de licença, de correção, de
regressão — depende de o CI de fato executar.

Diagnosticar **por que** ele parou, com evidência citável, é o que permite consertar a causa em vez do
sintoma.

É a mesma disciplina que o [m131](/benchmarks/m131-columnar-agg-accelerated.md) aplicou ao anexar um
depurador a um processo travado em vez de raciocinar sobre o código.
