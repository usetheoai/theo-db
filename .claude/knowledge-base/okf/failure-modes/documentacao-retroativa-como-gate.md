---
type: Failure Mode
title: Reconstruir rastro de ciclo depois do fato
description: Um milestone que shipou sem log de implementação nem review tenta 'regularizar' escrevendo os documentos a partir dos commits — alto risco de fabricação, zero valor de gate.
tags: [ciclo, rastro, honestidade]
timestamp: 2026-07-30T00:00:00Z
---

# Reconstruir rastro de ciclo depois do fato

## Assinatura

"Vamos escrever o log de implementação a partir do `git log` para fechar o ciclo."

## Caso — M168

Um milestone shipou 17 commits em `develop` com blueprint, plano e audit de code-quality — **mas sem log de
implementação nem review**. (O inventário completo do estado é rastro de ciclo e vive em
`knowledge-base/implementations/`, não aqui — § 4.2 do contrato; comprimido em 2026-07-30 após review.)

A tentação era reconstruir. A decisão foi **não**, por duas razões:

1. **Risco de fabricação** — escrever "o que foi feito" a partir de commits é reconstrução, e reconstrução é
   exatamente onde citações inventadas nascem.
2. **Zero valor de gate** — um review escrito depois do código não gateia nada.

O que **tem** valor de gate é garantir que o diff seja revisado **de fato**. Quando o review seguinte usa
`--diff-base main`, ele **inclui** o diff órfão — nenhum código não-revisado alcança `main`. A correção é travar
isso no contrato do próximo ciclo, não escrever prosa retroativa.

## Como evitar

Distinguir **lacuna de rastro** (ruim, documentável) de **lacuna de gate** (inaceitável). Corrija a segunda com
mecanismo; registre a primeira com honestidade.
