---
type: Decision
title: ADR 0058 — Shim da extensão vector: completar o drop-in do pgvector no nível de tooling
description: O drop-in fora entregue no nível SQL mas nunca no de tooling — nenhuma aplicação real subia, porque CREATE EXTENSION vector falhava; só o dogfood encontrou isso.
resource: git:f7c7b93:docs/adr/0058-pgvector-compat-shim.md
tags: [adr, compatibilidade, pgvector, shim, dogfood, tooling]
adr_id: "0058"
adr_status: Accepted
decision_date: 2026-07-24
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0058
    resource: git:f7c7b93:docs/adr/0058-pgvector-compat-shim.md
    title: ADR 0058 — Shim de extensão vector
    last_modified: 2026-07-24
---

O ADR que melhor demonstra por que dogfood não é cerimônia: **109+ artefatos de benchmark não achariam
este defeito, porque nenhum deles inicializa uma aplicação.**

# O que o dogfood encontrou

O [ADR 0029](/decisions/0029-m70-drop-pgvector.md) decidira que o tipo próprio ocupa `public.vector`
**justamente para ser drop-in**. Ao apontar uma aplicação real para um TheoDB self-hosted, ela **não
subiu**:

```
CREATE EXTENSION IF NOT EXISTS vector;
ERROR:  extension "vector" is not available
```

O tipo existe e os operadores funcionam; o que falta é o **objeto de extensão nominal**. Toda aplicação
pgvector roda esse comando no bootstrap — em scripts de migração e em testes de integração. Resultado:
**nenhuma aplicação conseguia inicializar**, então os 30 dias de uso sustentado que o dogfood exige
**nem podiam começar a contar**.

Diagnóstico honesto: a compatibilidade foi entregue e validada no nível **SQL e de tipos**, mas o nível
**tooling e drivers** nunca fora exercitado — porque nenhuma aplicação real havia sido apontada ao
banco.

# Decisão

Prover uma extensão `vector` **shim** que **não implementa nada**: o tipo, os operadores e as opclasses
continuam sendo código próprio. O shim existe apenas para que o `CREATE EXTENSION` suceda.

- **Depende da extensão própria**, de modo que `CASCADE` num banco limpo a instala sozinha, e o
  PostgreSQL barra remover a base enquanto houver coluna `vector`.
- **A versão declarada mapeia o contrato de features** que o tooling inspeciona — não é alegação de ser
  o pgvector.
- **Honestidade:** o comentário do control, visível ao usuário, declara literalmente que o tipo, os
  operadores e as opclasses **são providos por código próprio, não pelo pgvector**. E o harness de
  regressão **asserta esse texto**, de modo que a honestidade não pode regredir em silêncio.
- **Fail-fast:** o script não é vazio — valida que o tipo existe e, se não, levanta erro tipado com
  dica. Uma aplicação nunca deve acreditar que tem pgvector e quebrar depois, de forma obscura.

# Alternativas rejeitadas

**Pedir que cada aplicação remova o comando do bootstrap** — multiplica atrito exatamente onde o
dogfood precisa de zero atrito, e contradiz o drop-in já decidido; **atrito de migração é o principal
motivo pelo qual dogfoods não acontecem**. **Reintroduzir o pgvector** — contradiz a independência
conquistada; o shim mantém a implementação própria e só empresta o nome. **Publicar versão fictícia** —
tooling que checa versão para decidir features receberia um número sem significado.[^adr0058]

# Limitações declaradas, não escondidas

1. **O drop-in continuava incompleto** na primeira versão: o índice que as aplicações escrevem ainda
   falhava. O efeito honesto foi **mover a falha da linha 6 para a linha 44** da migração real — progresso
   mensurável, não "aplicação roda inteira". *(Fechado no adendo abaixo.)*
2. **Tooling sem `CASCADE`** falha em banco sem a extensão base — mitigado instalando a dependência no
   template padrão, para que todo banco criado depois a herde.
3. **Subir a versão exige script de upgrade**, sob pena de quebrar toda instalação existente.
4. **Nunca instalar sobre um PostgreSQL que já tenha o pgvector** — os nomes de arquivo colidem. O
   layout on-disk é byte-idêntico, então o pior caso é confusão de identidade, não corrupção — mas o
   cenário não é suportado.
5. **Privilégio:** herda o default fail-closed do PostgreSQL. Tornar a extensão "trusted" **não** deve
   ser feito: com a dependência, tornaria instalável por não-superuser uma cadeia que puxa código
   privilegiado com saída HTTP.

# Adendo — aliases de access method e opclasse

A limitação nº 1 foi **fechada**. O shim ganhou, via script de upgrade obrigatório, um alias de access
method apontando para o **mesmo handler próprio**, mais as três opclasses que as aplicações escrevem,
reusando os mesmos operadores e funções de suporte já existentes.

**Nada foi reimplementado** — é rotulagem de catálogo sobre a implementação existente. O harness
asserta que o alias e o nome próprio compartilham o **mesmo handler**, para que uma segunda
implementação divergente não passe despercebida.

**Evidência decisiva** — a migração versionada real de uma aplicação, aplicada **sem alterar uma linha**:

| Momento | Resultado |
|---|---|
| antes do shim | falha na **linha 6** |
| com o shim | falha na **linha 44** |
| com os aliases | **sucesso** — tabelas e índices criados |

Não-vacuidade provada: instalando a versão anterior do shim, a mesma migração volta a falhar.

**Limitação que permanece:** o outro access method do pgvector não recebeu alias; aplicações que o
escrevem continuam falhando. Estendê-lo é trabalho subsequente, com o mesmo padrão.

[^adr0058]: ADR 0058 — Shim de extensão vector
