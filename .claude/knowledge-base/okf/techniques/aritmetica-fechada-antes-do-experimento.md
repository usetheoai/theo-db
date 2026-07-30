---
type: Technique
title: Prever o número com uma conta antes de medir
description: Uma previsão fechada transforma a medição em teste da hipótese, e um erro de ordem de grandeza denuncia o modelo mental antes de gastar horas de máquina.
tags: [metodo, medicao, modelagem]
timestamp: 2026-07-30T00:00:00Z
---

# Prever o número com uma conta antes de medir

## Por que

Medir sem previsão só produz um número. Medir **com** previsão testa o modelo — e quando bate, a causa-raiz está
demonstrada, não sugerida.

## Casos em que fechou

| Previsão | Medido | Consequência |
|---|---|---|
| `flush_pending` consome ≈ `mwm × 7` → 16 GB com `mwm=2GB` | OOM com **23,4 GB** de `anon-rss` (estimativa de 700 B/linha é grosseira) | causa-raiz do #221 demonstrada; e a recarga com `mwm=128MB` (previsão ~510 MB) **completou** |
| offsets `i32` estouram acima de 21,5 B/linha em média sobre 100M | `ERR:byte array offset overflow` no q20 | confirma que qualquer corpus de URLs estoura, com ordens de folga |
| `ChunkDirEntry` = 48 B em memória × 10.000 cg × 105 col | 48,1 MiB por scan | termo O(N) do EC-1 quantificado sem instrumentar |

## Caso em que a previsão **evitou** trabalho

Antes do controle de deriva do M168, registrei a expectativa por escrito: *"o único delta de código no caminho
quente entre A e F são duas leituras de `i32` por chunk-group, que a ~100 chunk-groups não produzem 5% de uma
consulta de 140 ms — então eu **espero** que a diferença seja da box"*. Registrar a expectativa **antes** impede
que o resultado seja lido como confirmação retroativa.

## Relacionados

- [measurement/amplificacao-maintenance-work-mem](../measurements/amplificacao-maintenance-work-mem.md)
- [measurement/teto-offsets-i32](../measurements/teto-offsets-i32.md)
