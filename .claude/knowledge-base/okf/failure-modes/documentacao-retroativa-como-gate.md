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

Medido: `develop` 31 commits à frente de `origin/main`, 17 deles do M168. O M168 tem blueprint, plano e audit de
code-quality — **mas não tem log de implementação nem review**.

A tentação era reconstruir. A decisão foi **não**, por duas razões:

1. **Risco de fabricação** — escrever "o que foi feito" a partir de commits é reconstrução, e reconstrução é
   exatamente onde citações inventadas nascem.
2. **Zero valor de gate** — um review escrito depois do código não gateia nada.

O que **tem** valor de gate é garantir que o diff seja revisado **de fato**: a release do M169 carrega os 17
commits do M168, e `/review` usa `--diff-base main`, então o review do M169 **inclui** aquele diff. Nenhum código
não-revisado alcança `main`. Isso foi travado no contrato de implementação em vez de ser resolvido com prosa.

## Como evitar

Distinguir **lacuna de rastro** (ruim, documentável) de **lacuna de gate** (inaceitável). Corrija a segunda com
mecanismo; registre a primeira com honestidade.
