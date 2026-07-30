---
type: Invariant
title: Dois parsers da mesma string discordam — e a divergência vira a vulnerabilidade
description: endpoint_host validava a URL segundo a RFC; o cliente HTTP não implementa userinfo e caía para a porta 80, então http://169.254.169.254:x@api.openai.com ia para o metadata service.
resource: .claude/knowledge-base/reviews
tags: [seguranca, ssrf, parser, http]
timestamp: 2026-07-30T00:00:00Z
---

# Dois parsers da mesma string **discordam** — e a divergência vira a vulnerabilidade

## O caso (BLOCKER, review de segurança)

A validação de destino chamava-se `endpoint_host` e parseava a URL **conforme a RFC**: em
`http://169.254.169.254:x@api.openai.com`, o trecho antes do `@` é **userinfo**, logo o host é `api.openai.com` —
e a allowlist aprovava.

O cliente HTTP (`minreq`) **não implementa userinfo**. Ele resolveu a mesma string para o host
`169.254.169.254` e caiu silenciosamente para a **porta 80** — o **metadata service** da nuvem.

> Validador e executor leram a mesma string e chegaram a **hosts diferentes**. A allowlist estava correta sobre
> uma URL que ninguém buscou.

## O invariante

**Quem valida tem de ser quem resolve.** Uma checagem de destino só vale se for feita sobre o **valor que o
cliente vai realmente usar** — não sobre uma segunda interpretação da mesma string, por mais correta que essa
segunda interpretação seja segundo a norma.

Estar "certo pela RFC" é irrelevante aqui: o atacante não escolhe qual parser vence, mas **escolhe a string em que
os dois discordam**.

## Como fechar

| | |
|---|---|
| **Melhor** | valide o objeto **já parseado pelo cliente** (o host que ele resolveu), não a string crua |
| **Se não der** | use o **mesmo** parser nos dois lados, literalmente a mesma função |
| **Sempre** | rejeite as formas que criam ambiguidade — userinfo, IP-literal, porta implícita, unicode/IDN, `..` no path |
| **Defesa em profundidade** | bloqueie o range do metadata service (`169.254.169.254`, `fd00:ec2::254`) por rede, não só por allowlist |

A classe é geral — **parser differential**. Aparece em URL, path (`../`), MIME, JSON duplicate-key, cabeçalho HTTP
(request smuggling), certificado. Onde houver dois componentes lendo o mesmo texto, há um atacante procurando o
ponto em que eles discordam.

## Relacionados

- [failure-mode/allowlist-por-regex-sobre-linguagem](../failure-modes/allowlist-por-regex-sobre-linguagem.md)
- [failure-mode/fail-open-por-omissao](../failure-modes/fail-open-por-omissao.md)
