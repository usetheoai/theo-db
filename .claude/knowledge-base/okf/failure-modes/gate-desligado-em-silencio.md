---
type: Failure Mode
title: Um gate que não reconhece a entrada e pula sem avisar
description: O gate procura um literal exato; o artefato usa outro; ele não falha — ele SKIPa, e a ausência de reclamação é lida como aprovação.
tags: [gate, falso-verde, ciclo]
timestamp: 2026-07-30T00:00:00Z
---

# Um gate que não reconhece a entrada e pula sem avisar

## Assinatura

Nenhum erro, nenhum aviso, nenhuma execução. O gate simplesmente não tinha o que fazer — e ninguém percebe,
porque a saída de "passou" e a de "não rodou" são idênticas.

## Casos pagos

| Caso | O literal esperado | O que o artefato tinha |
|---|---|---|
| M169 | `check_phase_completeness.py:41` → `^##\s+Phase\s+(\d+)` | `## Fase N` (em português) — o mini review de fronteira pularia nas **quatro** fronteiras |
| M169 | `check_tdd_shape.py` exige forma executável | T4.1 não tinha seção `#### TDD` **alguma** |
| M168 | gate `_reject_fallback` casando `ARM=stream` — string que o harness **nunca emitia** | o gate procurava um literal inexistente; corrigido emitindo `RAISE NOTICE 'ARM=%'` + controle positivo in-tree |
| M156/M161 | A/B do ClickBench como oráculo de tipos | os dados do benchmark não exercitam o espaço de tipos — bugs de classe de tipo sobreviviam ao A/B e só caíam no review |

## Como evitar

- Um gate que pode SKIPar precisa **dizer** que skipou, e o consumidor precisa tratar SKIP como diferente de PASS.
  O `cycle-implement` documenta o SKIP gracioso do Step 4.7 — e é justamente por isso que o cabeçalho errado é
  perigoso: o comportamento é *documentado*, então ninguém investiga.
- Ao escrever artefato que um gate consome, **rode o gate** e confirme que ele reconheceu — não presuma pelo nome.
- Quando um oráculo tem cobertura estrutural conhecida (ex.: dados de benchmark não cobrem tipos), escreva o
  oráculo complementar: `benchmarks/columnar_type_ab.py` nasceu desta lição (`rules/testing.md` § 5.1).

## Relacionados

- [technique/controle-positivo](../techniques/controle-positivo.md)
