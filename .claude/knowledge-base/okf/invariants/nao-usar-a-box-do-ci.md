---
type: Invariant
title: A e2e-runner é o runner do CI — medir nela satura o pipeline e contamina o número
description: 165.227.121.20 hospeda o runner do GitHub Actions e k3s; usá-la para benchmark degrada o CI de todo o time e produz deriva.
tags: [infra, benchmark, operacao]
timestamp: 2026-07-30T00:00:00Z
---

# A e2e-runner é o runner do CI — medir nela satura o pipeline e contamina o número

## O invariante operacional

A `theo-e2e-runner` (`165.227.121.20`, 8 vCPU / 31 GB) hospeda **o runner do GitHub Actions e k3s**. Duas
consequências:

1. **Rodar benchmark lá satura o CI** de todo o time. Já aconteceu.
2. **Os números de lá não são confiáveis** — foi essa box que produziu rho = **+1,00** de deriva monotônica no
   M168, com o mesmo binário variando 2,9 pontos entre coletas.

## O que fazer em vez disso

Box dedicada e efêmera para a janela de medição. No M169: `c2-16vcpu-32gb` (16 vCPU / 32 GB / 400 GB), tag
`ephemeral`, ~US$ 0,56/h — e destruir ao fim.

E a box dedicada só é dedicada se o **operador** também sair de cima dela.

## Exceção legítima

Auditores que exigem toolchain pesado (`cargo-udeps` precisa compilar um crate pgrx, que exige
`~/.pgrx/config.toml`) precisam de **alguma** box com pgrx. Preferir a dedicada; a e2e-runner só se não houver
outra, e nunca durante janela de medição.

## Relacionados

- [failure-mode/contaminacao-por-concorrencia](../failure-modes/contaminacao-por-concorrencia.md)
