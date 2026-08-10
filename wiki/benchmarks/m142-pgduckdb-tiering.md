---
type: Measurement
title: m142 — tier-out: 175 MB a menos na imagem default, com as duas imagens testadas
description: Remove o único componente C++ do caminho default e valida as duas imagens com smokes que falham o build se o estado esperado não bater.
resource: git:f7c7b93:docs/benchmarks/m142-pgduckdb-tiering.md
tags: [benchmark, empacotamento, tiering, imagem, smoke, m142]
milestone: M142
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m142
    resource: git:f7c7b93:docs/benchmarks/m142-pgduckdb-tiering.md
    title: M142 — pg_duckdb tier-out
    last_modified: 2026-07-22
---

**Manchete:** o tier-out enxuga a imagem default em **175 MB (887 → 712 MB)**, removendo **o único
componente C++ do caminho default**; a capacidade continua **opt-in** numa imagem separada, com a
superfície funcionando ponta a ponta.

# O que a validação assere

As duas imagens são **buildadas do zero** e passam por smokes distintos:

- na **default**, assere-se a **ausência** da dependência — nem extensão, nem preload, nem a biblioteca
  de sistema — **e** que o guard levanta o erro tipado correto, com a dica de qual imagem puxar;
- na **htap**, assere-se a **presença** e o fluxo completo.

**Todos os gates falham com código diferente de zero.** Um script de validação que não falha não valida —
e este é o tipo de detalhe que separa verificação de teatro.

# Por que asserir a ausência importa

O risco de uma imagem "enxuta" é ela não estar enxuta. **Verificar que algo NÃO está lá** é tão
necessário quanto verificar que o que deve estar está — e é o que impede que uma mudança futura
reintroduza a dependência sem ninguém notar.

# O guard como decisão de UX

Sem ele, o usuário da imagem default receberia um erro obscuro sobre uma função inexistente. Com ele,
recebe **erro tipado com o próximo passo** — a disciplina de fail-fast aplicada a empacotamento.

A decisão é o [ADR 0056](/decisions/0056-m142-pgduckdb-htap-tiering.md), e foi **passo intermediário**:
a remoção total veio em [m143](/benchmarks/m143-pgduckdb-removal.md).
