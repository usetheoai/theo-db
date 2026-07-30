---
type: Failure Mode
title: 109 artefatos de benchmark e nenhum inicializa a aplicação
description: Uma suíte de benchmark mede algoritmos; ela é estruturalmente incapaz de descobrir que o produto não inicializa para um consumidor real.
tags: [dogfood, benchmark, cobertura]
timestamp: 2026-07-30T00:00:00Z
---

# 109 artefatos de benchmark e nenhum inicializa a aplicação

## O que aconteceu — M141

Por meses o dogfood foi tratado como "só falta o humano rodar 30 dias". Ao **de fato** tentar apontar uma
capability real (`theo-memory`, `theo-rag`) para o TheoDB, a descoberta foi outra: **nenhuma conseguia sequer
inicializar**.

> Os 109+ artefatos de benchmark nunca achariam isso — **nenhum deles inicializa uma aplicação.**

## A classe de cegueira

Benchmark mede **algoritmo sob condições controladas**. Ele não exercita: instalação, `CREATE EXTENSION`,
compatibilidade de client, ordem de bootstrap, GUCs de instância, ou a suposição que a aplicação faz sobre o
schema. Um número excelente de recall convive perfeitamente com um produto que não sobe.

## Por que a ausência de sinal enganou

Não havia teste vermelho. Havia **ausência de teste** — e ausência de sinal foi lida como ausência de problema,
que é o mesmo mecanismo de `cobertura-alegada-sem-execucao`, num nível acima: não é o auditor que não rodou, é a
categoria de verificação que não existia.

## Como evitar

- O gate de "produção" é **um consumidor real subindo**, não um número. É por isso que `dogfood-golden-rule.md`
  exige `Status: running` — uso sustentado em infra própria — e não evidência sintética.
- Ao declarar cobertura, enumere **o que a suíte estruturalmente não pode ver**, não só o que ela cobre.

## Relacionados

- [failure-mode/cobertura-alegada-sem-execucao](cobertura-alegada-sem-execucao.md)
- [failure-mode/instrumento-cego-a-arquitetura](instrumento-cego-a-arquitetura.md)
