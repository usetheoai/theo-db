---
type: Failure Mode
title: O sintoma nomeia a fase errada — o backtrace disse EXPLAIN onde o issue dizia planner
description: O #135 reportava 18 min de hang no planner sobre tabela larga e propunha guard por largura; o gdb mostrou recursão infinita no deparse do EXPLAIN, e a mesma query EXECUTA em 0,537 s.
resource: .claude/knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md
tags: [diagnostico, profiling, planner, explain]
timestamp: 2026-07-30T00:00:00Z
---

# O sintoma nomeia a **fase errada** — e o guard proposto teria sido no lugar errado

## O caso (#135)

**Reportado:** *"hang ininterruptível de 18 minutos no **planner**"* com `enable_columnar_agg=on` sobre o
ClickBench `hits` (105 colunas, 27 TEXT). Hipótese: laço **O(cols²)** no caminho de custo do CustomScan. Fix
sugerido: perfilar o hook de custo + **guard por largura/tipo**.

**Medido (gdb no backend travado, 3 amostras idênticas):**

```
#0  check_stack_depth / get_tle_by_resno   (parse_relation.c)
#1  resolve_special_varno                  (ruleutils.c:7699)
#2  resolve_special_varno                  (ruleutils.c:7674)   ← recursão em si mesma
#3  get_variable            #4  deparse_expression_pretty
#6  show_sort_group_keys ("Sort Key")      (explain.c:2794)
#8  ExplainNode → ExplainPrintPlan → EXPLAIN
```

**Não é o planner e não é a execução — é a IMPRESSÃO DO PLANO.** Três fatos que fecham:

| | |
|---|---|
| das 43 queries do ClickBench, **exatamente 2** travam | Q16 e Q33 — as duas com `ORDER BY` sobre o agregado |
| `GROUP BY userid` **sem** `ORDER BY` | planeja em **27 ms** |
| Q16 **executada** (sem EXPLAIN) | **0,537 s**, resultado correto |

A query nunca foi lenta. **Só descrevê-la era.**

## Por que a hipótese era plausível e mesmo assim errada

Tudo no relato apontava para o planner: acontece antes de qualquer linha sair, `statement_timeout` não dispara
durante o planejamento, e a tabela é larga — "O(cols²)" explica largura. A hipótese era **coerente com todo o
sintoma** e **falsa**.

O guard por largura/tipo teria sido implementado, teria "resolvido" o caso reportado ao declinar as tabelas
largas, e teria **desligado o roteamento colunar exatamente onde ele mais vale** — sem nunca tocar a recursão.

## A regra

1. **`EXPLAIN` não é observação passiva.** Ele *deparsa* a árvore; num nó custom com `scanrelid=0` os `Var`s não
   têm relação de origem para resolver, e `resolve_special_varno` pode recorrer. Um hang que só aparece sob
   `EXPLAIN` acusa a impressão, não o plano.
2. **Bisseccione por fase antes de teorizar:** planeja? (EXPLAIN) · executa? (sem EXPLAIN) · imprime? Se executar
   é rápido e explicar trava, a fase está identificada em dois comandos.
3. **Ataque o processo travado, não o código.** Um backtrace ao vivo — três amostras, idênticas — decide em
   minutos o que a leitura de código não decide em horas.

## Relacionados

- [failure-mode/explain-analyze-e-instrumento-assimetrico](explain-analyze-e-instrumento-assimetrico.md) — o outro jeito de o EXPLAIN mentir
- [failure-mode/diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md)
- [technique/instrumentar-em-vez-de-adivinhar](../techniques/instrumentar-em-vez-de-adivinhar.md)
- [technique/a-forma-da-curva-diagnostica-a-causa](../techniques/a-forma-da-curva-diagnostica-a-causa.md)
