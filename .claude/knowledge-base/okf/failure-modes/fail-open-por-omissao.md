---
type: Failure Mode
title: O caminho de validação não previsto roda SEM a restrição
description: Um filtro que valida a forma esperada e ignora a inesperada não falha — ele executa sem o filtro, que é o pior desfecho possível.
tags: [seguranca, fail-closed, validacao]
timestamp: 2026-07-30T00:00:00Z
---

# O caminho de validação não previsto roda **sem** a restrição

## O caso — M120

Filtro estruturado de `ai.hybrid_search`: `[{col,op,value}]` com allowlist de operadores, `quote_identifier`,
`quote_literal`. O desenho era **fail-closed** — operador fora da allowlist, `IN` vazio, `filter` + `filter_sql`
juntos → `SQLSTATE 22023`.

O review de segurança achou o buraco: **`filter` não-array era fail-OPEN** — a consulta rodava **sem filtro
algum**. O caminho não previsto não recusava; ele seguia adiante desprotegido.

## Por que é a pior forma de falhar

| Desfecho | Consequência |
|---|---|
| fail-closed | erro tipado, usuário corrige |
| crash | ruidoso, alguém investiga |
| **fail-open** | **silencioso, e devolve dados que o chamador acredita estarem filtrados** |

Num filtro de tenant, fail-open é vazamento entre tenants sem nenhum sinal.

## Como evitar

- A validação enumera o que é **aceito** e recusa **todo o resto** — nunca enumera o que é rejeitado.
- Todo ramo de validação precisa de caso negativo no teste: forma errada, tipo errado, vazio, nulo.
  `rules/testing.md` § 4.1 chama isso de lente negativa, e ela é a metade que costuma faltar.
- `filter_sql` cru continua existindo como escape-hatch **documentado como caller-privilege** — o honesto é
  nomear o que não é seguro, não fingir que é.

## Relacionados

- [failure-mode/gate-desligado-em-silencio](gate-desligado-em-silencio.md)
