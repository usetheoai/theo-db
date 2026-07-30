---
type: Failure Mode
title: Aceitar um diagnóstico bem-argumentado sem refazer a conta
description: Um revisor (ou eu mesmo) apresenta uma explicação coerente; ela entra no documento sem que a medição que a sustentaria seja reproduzida. As que mais escapam são as que favorecem quem escreve.
tags: [metodo, honestidade, revisao]
timestamp: 2026-07-30T00:00:00Z
---

# Aceitar um diagnóstico bem-argumentado sem refazer a conta

## Assinatura

Uma explicação entra em plano, ADR, issue ou verdict porque é **convincente**, não porque foi **verificada**.
O sinal de alerta mais forte: a explicação resolve um problema seu.

## Casos pagos — todos de uma única sessão

| Alegação | Como caiu |
|---|---|
| "#219: o script não tem `CREATE FUNCTION` algum" | tem **60**; e o arquivo é gerado, com cabeçalho proibindo a edição que eu sugeri |
| "#220: o detector roda `udeps` com `cwd=repo_root`" | `run_code_quality.py:230` já passa `manifest_dir`. A causa real era ambiental (`~/.pgrx/config.toml` ausente) |
| "#220 (2ª): então não há defeito de `cwd`" | **há** — na cópia do umbrella (`:235`). Eu conferira a cópia certa para a invocação e a errada para o issue |
| "EC-2: tabela vazia é comportamento indeterminado" | já tratado em `df_executor.rs:1132-1134`, com comentário explícito |
| "a falha do q20 a 100M nunca foi observada" | está em artefato cru desde 2026-07-26 (`theodb-100m-partial.jsonl:21`) |
| "o pré-check do ADR-5 é uma expressão" | `StripePlan` é privado e `ScanPlan.plans` é campo privado — custa um acessor |
| "o pushdown agregado é regressão de memória" | isolado, pico **4,58 GB** vs 4,57 GB sem pushdown. A causa era o oráculo do harness |

E o inverso também: no M168, o revisor alegou "ordem e efeito perfeitamente confundidos"; eu escrevi isso em
negrito no verdict **antes** de calcular o rho da razão, que deu **+0,71** com três quebras — negando a alegação.

## Custo

Quatro rodadas consecutivas do M168 em que a *correção* introduziu o defeito seguinte. Um issue (#219) publicado
com diagnóstico falso e sugestão de fix proibida pelo próprio arquivo.

## Como evitar

[technique/nenhuma-alegacao-sem-medicao](../techniques/nenhuma-alegacao-sem-medicao.md) — a regra explícita, e a
metade dela que falha na prática é *"vale para as que me favorecem"*.

## Relacionados

- [technique/medir-antes-de-filar](../techniques/medir-antes-de-filar.md)
