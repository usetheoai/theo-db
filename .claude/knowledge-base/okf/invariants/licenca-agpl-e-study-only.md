---
type: Invariant
title: Peers AGPL são estudo, nunca fonte de código
description: A distribuição é Apache-2.0 com gate fail-closed contra AGPL; técnica se aprende, código se reimplementa do zero.
resource: ../../../.claude/rules/reference-provenance.md (umbrella)
tags: [licenca, compliance, d1]
timestamp: 2026-07-30T00:00:00Z
---

# Peers AGPL são estudo, nunca fonte de código

## O invariante (D1 do PRD)

A distribuição do TheoDB é **Apache-2.0**, com gate de licença fail-closed. Só entram dependências
Apache-2.0 / MIT / BSD / PostgreSQL License.

No acervo há peers copyleft — `vectorchord`, `paradedb`, `citus`, `hydra` (AGPL) e `FlameGraph` (CDDL). São
**study-only**: ler para entender a técnica é legítimo; copiar código para a distribuição é proibido.

## A distinção operacional

> **Técnica aprende-se; código reimplementa-se do zero.**

Ler como o VectorChord organiza a quantização e depois escrever a sua própria implementação é engenharia. Adaptar
o arquivo deles é obra derivada — e adaptação de cópia continua sendo cópia.

## Mecanismo, não confiança

`reference-provenance.md` — que vive no **umbrella** (`theo-cloud/.claude/rules/`), não no theo-db — define quatro camadas: guard de escrita na zona, guard de exportação por comando,
guard de mensagem de commit, e detector de vazamento por shingles. As três primeiras **bloqueiam**; a quarta é
**advisory** — casamento exato é evidência forte, não prova, e um BLOCK falso numa heurística é pior que um WARN.

> **CORRIGIDO 2026-07-30 após review.** O conceito citava `rules/reference-provenance.md` como se fosse local.
> **Não existe no theo-db** — só no umbrella. As outras seis citações `rules/*` do bundle resolvem contra
> `.claude/rules/` do theo-db, então a base implícita era inequívoca e a citação estava quebrada. O **conteúdo**
> afirmado (4 camadas, as 3 primeiras bloqueiam, a 4ª é advisory) confere com o arquivo do umbrella.

## Relacionados

- [invariant/acervo-local-antes-da-web](../techniques/acervo-local-antes-da-web.md)
