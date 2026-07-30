---
type: Failure Mode
title: Um assert que é uma IDENTIDADE passa sempre e não prova nada — e o gate parece verde
description: O parity-gate assertava memória com uma expressão algebricamente equivalente aos dois lados, e o gate de recall não isolava o quantizador porque o carrier f32 + rerank dominavam.
resource: .claude/knowledge-base/reviews
tags: [teste, gate, oraculo, vies]
timestamp: 2026-07-30T00:00:00Z
---

# Um assert que é uma **identidade** passa sempre e não prova nada

## As duas formas que apareceram no mesmo review (BLOCKER)

**(1) A identidade algébrica.** O parity-gate de memória comparava dois lados que eram, depois de substituir as
definições, **a mesma expressão**. O assert não podia falhar — não porque a propriedade valia, mas porque não
havia propriedade sendo testada. Verde permanente.

**(2) O gate que não isola o que diz medir.** O gate de recall pretendia validar o **quantizador**, mas rodava com
o **carrier f32 + rerank** no caminho, e esses dominavam o resultado. Qualquer quantizador — inclusive um quebrado
— passaria, porque o rerank f32 conserta a lista antes de o recall ser medido.

As duas falham do mesmo jeito: **o gate está verde e o verde não é evidência**.

## Como detectar antes de confiar

| Pergunta | Se a resposta for essa, o gate é vazio |
|---|---|
| Existe um valor de entrada que faria este assert **falhar**? | "não consigo construir" |
| Se eu **quebrar de propósito** o componente sob teste, o gate fica vermelho? | fica verde |
| O que domina a métrica: o componente ou algo **downstream** dele? | o downstream |

O segundo é o **positive control**, e é o único que responde às duas formas de uma vez —
[controle-positivo](../techniques/controle-positivo.md). Um gate que nunca foi
visto falhando é uma hipótese sobre o gate, não uma medição.

## A ablação certa

Para isolar um componente, tire do caminho tudo que possa mascará-lo, ou meça **na mesma configuração** com só
aquele componente variando — a ablação mesmo-índice. O precedente medido: o kernel FastScan 1-bit parecia dar
2,8× em comparação cross-box e deu **1,2×** quando isolado no mesmo índice.

## Terceira forma: o gate casa por SUBSTRING de nome e passa numa colisão (M169, 2026-07-30)

O `check_wiring.py` — cujo pilar (a) o contrato declara **não-negociável** — procura o chamador de produção
fazendo `grep` do **nome do símbolo** nos arquivos-fonte. Medido na mesma invocação, sobre o mesmo módulo:

| símbolo | pilar (a) | realidade |
|---|---|---|
| `attest` | **FAIL** | chamador existe (`m169_box_attest.py:203`) |
| `collect` | **PASS** | **nenhum** chamador de produção existe |

O `collect` passou porque `collect` é o nome do método de iterador da stdlib do **Rust**, e o repo tem dezenas de
`.collect()` em `theodb_rs/src/`. Substring casada, linguagem ignorada, módulo ignorado, escopo ignorado.

Consequência: **qualquer símbolo com nome genérico** — `collect`, `next`, `get`, `run`, `build`, `new`, `insert`,
`scan` — passa o pilar não-negociável **sem chamador algum**. Um gate que não pode ficar vermelho para esses
nomes não é evidência sobre eles.

E o falso FAIL do `attest` tem causa independente: o filtro `--exclude='*test*'` é glob de substring, e o módulo
de **produção** `m169_box_attest.py` contém `test` em "a**test**" — o arquivo com o chamador é classificado como
teste e excluído.

> Um falso FAIL custa tempo. Um **falso PASS** custa cobertura — e nenhum dos dois é visível de dentro do gate.

O padrão dos três casos desta página é o mesmo: **pergunte o que faria este verde ficar vermelho.** Se a resposta
depende de uma coincidência de nome, de uma expressão algebricamente fechada, ou de um componente downstream que
domina a métrica, o verde não mede o que diz medir.

## Relacionados

- [technique/controle-positivo](../techniques/controle-positivo.md)
- [failure-mode/cobertura-alegada-sem-execucao](cobertura-alegada-sem-execucao.md)
- [failure-mode/oraculo-que-nao-compara-a-chave](oraculo-que-nao-compara-a-chave.md)
