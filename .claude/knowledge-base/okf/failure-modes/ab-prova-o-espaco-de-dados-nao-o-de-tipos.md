---
type: Failure Mode
title: O A/B do benchmark prova o espaço de DADOS; o review prova o espaço de TIPOS
description: Cinco milestones seguidos tiveram HIGH/BLOCKER invisíveis ao diverged=0, porque os dados do ClickBench não exercitam o espaço de tipos.
resource: benchmarks/columnar_type_ab.py
tags: [oraculo, cobertura, review, colunar]
timestamp: 2026-07-30T00:00:00Z
---

# O A/B do benchmark prova o espaço de **dados**; o review prova o espaço de **tipos**

## A recorrência — cinco milestones, sempre igual

| Milestone | `diverged=0` disse | O review achou |
|---|---|---|
| M151 | cobertura 6→14 limpa | HIGH temporal/float **cross-type** que o ClickBench-agg não exercita |
| M154 | +4 consultas | HIGH float IEEE (`-0.0 != +0.0`) → declinar float |
| M156 | +10 consultas | pânico UTF-8 no planner **e** `LIKE` com escape pendente (erro 22025 vs devolver linhas) |
| M157 | q42 roteada | CRITICAL de epoch de calendário |
| M161 | +3 consultas | 1 BLOCKER + 1 HIGH que o A/B `int4-int4` nunca dispararia |

Cinco vezes o A/B ficou verde e o **conselho de review** achou o defeito.

## Por que é estrutural, não azar

O A/B roda sobre os **dados do ClickBench**. Esses dados têm um espaço de tipos **estreito e fixo** — não têm
`int2` no limite, não têm `-0.0`, não têm colação não-C, não têm texto não-UTF-8, não têm data em fronteira de
mês. **Um oráculo só prova o espaço que os dados percorrem.**

Isso não é defeito do A/B; é o alcance dele. O defeito é ler `diverged=0` como "está correto" em vez de
"está correto **nas formas que estes dados produzem**".

## O que resolve

`benchmarks/columnar_type_ab.py` (M163) nasceu exatamente disto — um oráculo **por tipo**, com `EDGE_CATALOG` de
valores de borda por classe e **controle positivo** obrigatório. A regra `testing.md` § 5.1 o torna gate antes de
`/review` para qualquer mudança nos admit-paths de roteamento.

**Regra derivada:** ao declarar cobertura, diga **o que o oráculo estruturalmente não pode ver**, não só o que
ele cobre.

## Relacionados

- [failure-mode/oraculo-que-nao-compara-a-chave](oraculo-que-nao-compara-a-chave.md)
- [technique/controle-positivo](../techniques/controle-positivo.md)
- [failure-mode/cobertura-alegada-sem-execucao](cobertura-alegada-sem-execucao.md)
