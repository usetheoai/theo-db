---
type: Failure Mode
title: Uma allowlist por regex sobre uma LINGUAGEM é bypassável — a mesma defesa caiu duas vezes seguidas
description: O allowlist de relações do NL→SQL foi furado por vírgula-join e depois por identificador entre aspas; regex não conhece a gramática que tenta restringir.
resource: .claude/knowledge-base/reviews
tags: [seguranca, sql, validacao, parser]
timestamp: 2026-07-30T00:00:00Z
---

# Uma allowlist por **regex** sobre uma **linguagem** é bypassável

## Duas BLOCKER consecutivas na MESMA defesa

O NL→SQL (camada L4) restringia quais relações a consulta gerada podia tocar, casando `FROM`/`JOIN` por regex.

| # | Bypass | Por quê passou |
|---|---|---|
| 1 | **vírgula-join** — `FROM documents, secret` | a regex só capturava a **primeira** relação; `secret` nunca era conferida |
| 2 | **identificador entre aspas** — `FROM "secret"` | a regex exigia `[a-zA-Z_]` depois de `FROM`; a aspa fez capturar **zero** relações → **allowlist virou no-op** |

O segundo é o pior dos dois: a defesa não deixou passar *uma* relação — ela **se desligou inteira**, e em silêncio.
Read-exfil de `pg_*`/`secret` num caminho que existia justamente para impedir isso.

## Por que a classe se repete

Uma regex casa **texto**. SQL é uma **gramática** com aliases, aspas, comentários (`/**/`), CTEs, subqueries,
`schema.tabela`, `LATERAL`, `UNION`, sinônimos de junção implícita. Cada uma dessas é uma superfície que a regex
não modela — e **cada correção pontual só fecha a variante que alguém lembrou**. Foi exatamente o que aconteceu:
o patch do bypass 1 não previu o 2.

## O que fazer

1. **Valide pela árvore, não pelo texto.** Peça ao próprio motor a lista de relações — `EXPLAIN (FORMAT JSON)`,
   o parser da plataforma, ou `pg_depend` sobre uma view preparada. O oráculo certo é quem executa.
2. **Fail-closed no desconhecido.** Se a extração devolve **zero** relações numa consulta que obviamente lê algo,
   isso é **erro**, nunca "nada a checar" — ver [fail-open-por-omissao](fail-open-por-omissao.md).
3. **Autorização de verdade fica no banco.** `REVOKE`/RLS/role dedicada continuam valendo quando a camada de
   parsing falha; a allowlist é defesa em profundidade, não a única.
4. Ao corrigir um bypass, **procure a próxima variante da mesma gramática** antes de fechar. Uma correção pontual
   num filtro textual quase nunca é a última.

## Relacionados

- [failure-mode/fail-open-por-omissao](fail-open-por-omissao.md) — a variante "zero capturas = tudo permitido"
- [technique/controle-positivo](../techniques/controle-positivo.md) — um bypass conhecido no conjunto de teste prova que o filtro morde
