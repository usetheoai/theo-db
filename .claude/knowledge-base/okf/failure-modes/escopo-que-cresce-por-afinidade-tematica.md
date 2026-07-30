---
type: Failure Mode
title: Absorver um achado no milestone só porque é do mesmo tema
description: Um defeito real, descoberto durante o milestone, é puxado para dentro dele por parentesco conceitual — e o milestone passa a ter dois Goals.
tags: [escopo, planejamento, yagni]
timestamp: 2026-07-30T00:00:00Z
---

# Absorver um achado no milestone só porque é do mesmo tema

## Assinatura

"Já que é a mesma família de bug, faz sentido consertar aqui." O milestone perde a métrica única.

## Caso — M169 / #221 (a decisão certa, registrada como ADR-7)

O #221 (`flush_pending` consome ≈ `mwm × 8`) foi descoberto durante o baseline do M169, é da **mesma família**
(termo O(N) onde deveria haver O(chunk-group)) e tem o **mesmo padrão de fix**. Tentação forte.

O critério que decidiu **não** foi mecânico, não estético: **ele bloqueia o milestone?** A medição respondeu —
com `maintenance_work_mem = 128MB` a carga de 99.997.497 linhas completou. Logo **não bloqueia**, e vira
milestone próprio.

## Como evitar

A pergunta é sempre "**bloqueia?**", nunca "**combina?**". Se bloqueia, absorva com ADR explícito. Se não,
file o issue com fix verificado e siga.

## Relacionados

- [technique/medir-antes-de-filar](../techniques/medir-antes-de-filar.md)
