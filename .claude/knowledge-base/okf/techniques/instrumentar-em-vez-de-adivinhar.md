---
type: Technique
title: Quando a pergunta é 'por que não roteia?', instrumente o caminho de decisão
description: Deduzir cobertura por leitura de SQL produz hipóteses; um trace das razões de declínio produz o mapa.
tags: [instrumentacao, cobertura, roteamento]
timestamp: 2026-07-30T00:00:00Z
---

# Quando a pergunta é "por que não roteia?", instrumente o caminho de decisão

## O caso — M152

A pergunta era quais consultas do ClickBench não usavam o pushdown colunar e por quê. A abordagem por leitura de
SQL gerava palpites. A abordagem que funcionou foi um trace no ponto de admissão (`THEODB_ADMIT_TRACE`) que
registra **a razão do declínio** por consulta.

Resultado: mapa completo, zero lacunas — e o achado estrutural de que **os bloqueios são compostos**. Uma
consulta declinava por 2-3 razões simultâneas, então a cobertura marginal de destravar uma fatia isolada é
**pequena (2-4 consultas)** — o artefato `m152-routing-map.md:14` diz exatamente isso, e conclui que "o modelo
'cada fatia adiciona K' do blueprint era **otimista**".

> **CORRIGIDO 2026-07-30 (round 3).** A frase anterior dizia "não +11" e "~3× otimista". Nem o `+11` nem o `~3×`
> aparecem no artefato — ele registra o modelo ingênuo de forma **abstrata**, sem valor de K. Dois números sem
> âncora numa Technique que ensina a instrumentar em vez de adivinhar.

## Onde se repetiu

- **M157:** o declínio do `date_trunc` só foi diagnosticável com o trace ligado — pós-planning a chave de grupo é
  `Var`, não `FuncExpr`. Impossível de deduzir do SQL.
- **M148:** o flamegraph mostrou que ~80% do custo do scan colunar era **materialização linha-a-linha**, não
  decode — invertendo a hipótese de trabalho e reordenando três milestones (M149 → M151 → M150).

## Regra

Quando a pergunta é sobre **decisão interna** (roteou? por quê não? onde gastou?), a resposta vem de
instrumentação, não de leitura. Leitura de código gera a hipótese; o trace a testa.

## Relacionados

- [technique/aritmetica-fechada-antes-do-experimento](aritmetica-fechada-antes-do-experimento.md)
