---
type: Measurement
title: m137 — cadeia de upgrade da extensão, com limite honesto
description: O upgrade funciona pela primeira vez em 120 releases — e o milestone declara-se INCOMPLETO porque o teste que prova convergência a partir de um catálogo antigo de verdade ainda não rodou.
resource: git:f7c7b93:docs/benchmarks/m137-upgrade-chain.md
tags: [benchmark, upgrade, extensao, incompleto, honestidade, m137]
milestone: M137
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m137
    resource: git:f7c7b93:docs/benchmarks/m137-upgrade-chain.md
    title: M137 — cadeia de upgrade do theodb_rs
    last_modified: 2026-07-21
---

**Manchete:** o comando de upgrade da extensão **funciona — pela primeira vez em 120 releases**.

**Mas o teste que prova convergência a partir de um catálogo antigo de verdade ainda não rodou, e por
isso este milestone NÃO está completo.**

# Por que a segunda frase é o que importa

É raro um artefato declarar o próprio milestone incompleto tendo um resultado positivo para mostrar. A
distinção que ele faz é precisa:

- **o que foi provado:** o mecanismo de upgrade executa e produz o estado esperado;
- **o que NÃO foi provado:** que ele **converge a partir de um catálogo real antigo**, com todo o
  histórico de objetos que 120 releases acumularam.

**São coisas diferentes.** Um script de upgrade que funciona sobre uma instalação limpa e falha sobre uma
antiga é exatamente o defeito que a cadeia existe para prevenir — e testar só o caso fácil daria falsa
confiança.

# Por que a cadeia importa tanto neste projeto

Uma extensão sem cadeia de upgrade obriga toda instalação a reinstalar — que é a limitação que o
[ADR 0025](/decisions/0025-m66-chunking-strategies.md) teve de declarar quando colunas novas apareceram
no schema, e o risco que o [ADR 0058](/decisions/0058-pgvector-compat-shim.md) nomeia ao alertar que
**subir uma versão sem shipar o script quebra toda instalação existente**.

A disciplina estabelecida aqui é o que o [ADR 0056](/decisions/0056-m142-pgduckdb-htap-tiering.md) e o
[ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md) depois usaram para mudar a superfície sem
quebrar quem já tinha instalado.
