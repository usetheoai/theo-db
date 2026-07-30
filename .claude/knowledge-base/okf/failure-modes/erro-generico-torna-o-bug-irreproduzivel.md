---
type: Failure Mode
title: Uma mensagem de erro genérica apaga a causa — e o bug reportado deixa de ser reproduzível
description: O #132 dizia que o worker dead-letrava todo job; não reproduziu. O defeito real era o caminho de erro descartando a causa e um lote de zero linhas contado como sucesso.
resource: .claude/knowledge-base/discoveries/blueprints/vectorizer-worker-embed-blueprint.md
tags: [erro, diagnostico, observabilidade, issue]
timestamp: 2026-07-30T00:00:00Z
---

# Uma mensagem de erro genérica apaga a causa — e o bug deixa de ser reproduzível

## O caso (#132)

O issue relatava: o worker assíncrono do vectorizer **dead-letra todo job** — `state='failed'`, `attempts=5`,
`last_error='embed/upsert failed'` — enquanto `theodb.embed(...)` funciona numa sessão normal.

Reprodução ao vivo no droplet, build corrente: **5/5 linhas embedadas pelo background worker, fila vazia, zero
falhas.** O sintoma **não reproduz**.

Mas dois defeitos **reais e duráveis** apareceram no caminho:

| # | Defeito |
|---|---|
| (a) | o caminho de erro **descarta a causa subjacente** — `'embed/upsert failed'` é tudo que sobra |
| (b) | um lote de **zero linhas** é contado como **sucesso** |

> O que falhou de verdade não foi o embed — foi a **diagnosticabilidade**.

## Por que a classe é cara

`last_error='embed/upsert failed'` é compatível com: chave inválida, timeout, 429, DNS, TLS, endpoint errado,
dimensão divergente, tabela alvo sem coluna. **Todas as hipóteses continuam vivas depois de ler o log** — logo o
reporter não consegue montar uma repro, e quem investiga tem de recriar o ambiente inteiro para descobrir algo
que o programa **sabia** e jogou fora.

E o defeito (b) fecha o cerco: com zero linhas contando como sucesso, o caminho feliz também não distingue
"funcionou" de "não fez nada" — ver [gate-de-nao-vacuidade](../techniques/gate-de-nao-vacuidade.md).

## A regra

1. **Nunca colapse a causa.** Propague o erro tipado de origem (status HTTP, SQLSTATE, `errno`) no texto e no
   campo estruturado. `rules/error-handling.md` § 2: erro explícito e tipado, com contexto suficiente para
   reproduzir **sem** debugger.
2. **Um lote vazio não é sucesso.** É `no-op` — e no-op precisa de um estado próprio, não do estado verde.
3. **Antes de "consertar" um issue, reproduza.** Se não reproduz, isso **é** o achado — e o defeito costuma estar
   no que impediu o reporter de dizer o que aconteceu. Ver
   [diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md).

## Relacionados

- [failure-mode/diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md)
- [technique/gate-de-nao-vacuidade](../techniques/gate-de-nao-vacuidade.md)
- [invariant/worker-nao-ve-set-de-sessao](../invariants/worker-nao-ve-set-de-sessao.md)
