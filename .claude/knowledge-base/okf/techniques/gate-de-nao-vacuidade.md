---
type: Technique
title: Todo gate declara o que conta como resultado — e recusa o resto
description: Um gate sem definição explícita de desfecho observável não distingue 'passou' de 'não rodou'.
tags: [gate, oraculo, medicao]
timestamp: 2026-07-30T00:00:00Z
---

# Todo gate declara o que conta como resultado — e recusa o resto

## O padrão

Antes de rodar, o gate declara **quais desfechos são válidos**. Qualquer outra coisa é `[INVALIDO]` — nunca um
número silencioso.

```bash
ROWS=$(echo "$OUT" | grep -oE '\([0-9]+ rows?\)' | tail -1)
ERR=$(echo  "$OUT" | grep -iE '^(ERROR|FATAL|server closed)' | head -1)
if [ -z "$ERR" ] && ! echo "$ROWS" | grep -qE '\(10 rows?\)'; then
  echo "[INVALIDO] nem 10 linhas nem erro — esta medicao NAO conta"
else
  echo "[VALIDO] a medicao tem desfecho observavel"
fi
```

Note que **um erro é desfecho válido**. O gate não exige sucesso; exige *observabilidade*. Um OOM esperado é
resultado; silêncio não é.

## Onde provou valor

Na quarta tentativa de medir o pico de memória do q17 (M169), este gate **recusou** a medição em vez de deixar
"1800 s, 4,57 GB" virar número comparável — o braço tinha sido cortado por timeout e não produzira linha alguma.
Nas tentativas anteriores, sem o gate, `(0 rows)` em 10,354 ms passou como dado.

## Variantes por domínio

| Domínio | Desfecho válido |
|---|---|
| A/B de resultado | `diverged=0` **com** controle positivo tendo dado `diverged>0` |
| baseline de N consultas | as N têm veredito explícito (`ok`/`error:<sqlstate>`/`timeout`/`oom`) |
| medição de pico | ≥ K amostras colhidas **durante**, e um desfecho (linhas ou erro) |
| gate de roteamento | `EXPLAIN` contém o nó esperado, **ou** prova da ausência |

## Relacionados

- [technique/controle-positivo](controle-positivo.md)
- [failure-mode/medicao-vacuosa-aceita](../failure-modes/medicao-vacuosa-aceita.md)
