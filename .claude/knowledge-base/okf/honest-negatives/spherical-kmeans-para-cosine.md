---
type: Honest Negative
title: Spherical k-means é no-op provado para distância cosseno
description: Implementado, medido, revertido: para cosine a normalização já ocorre, e o k-means esférico não muda nada.
tags: [vetorial, ivf, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# Spherical k-means é **no-op provado** para distância cosseno

## O veredito (M121)

Implementado e medido: para distância **cosseno**, a normalização já acontece no caminho, e o k-means esférico
**não muda o resultado**. Revertido.

## Por que registrar

É uma ideia que reaparece em toda revisão de literatura de IVF, porque é verdadeira em geral — só não neste
caminho. Sem o registro, o custo de implementar-medir-reverter se repete.

## Padrão que este caso ilustra

Uma técnica correta na literatura pode ser no-op **na sua composição específica**. A pergunta não é "a técnica
funciona?" e sim "**o que no meu caminho ela mudaria?**" — e essa pergunta às vezes se responde antes de
implementar.
