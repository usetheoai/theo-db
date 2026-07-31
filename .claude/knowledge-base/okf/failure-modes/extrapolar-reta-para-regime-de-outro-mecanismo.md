---
type: Failure Mode
title: Extrapolar uma reta medida para um regime onde o MECANISMO é outro — a reta continua certa, a previsão não
description: Medi que o pico do GROUP BY é linear a ~92 B/grupo e previ, por aritmética, que a q32 (~10⁸ grupos ≈ 9 GB) não teria como completar. Ela completou em 290 s. A reta valia só enquanto o estado tinha de caber em memória; sob streaming o DataFusion passa a derramar para disco.
resource: .claude/knowledge-base/okf/measurements/pico-do-groupby-e-linear-na-cardinalidade.md
tags: [medicao, extrapolacao, previsao, mecanismo, streaming, spill, honestidade]
timestamp: 2026-07-31T00:00:00Z
---

# Extrapolar uma reta para um regime governado por **outro mecanismo**

## O erro, em uma frase

Uma reta medida descreve o comportamento **dentro do mecanismo em que foi medida**. Extrapolá-la para um regime
onde o mecanismo muda produz uma previsão que parece quantitativa — com números, unidades e R² — e é
qualitativamente errada.

## A instância que custou (2026-07-31, M169)

Medi no T3.2, com rigor: o pico da pool do `GROUP BY` é **linear na cardinalidade**, ~92 B por grupo, cinco
pontos, box ociosa. A reta está correta e continua valendo.

Daí derivei, e **escrevi como fato**:

> A q32 (`GROUP BY WatchID, ClientIP`, ~10⁸ grupos) precisaria de ordem de 9 GB de estado contra uma pool de
> 192 MiB. **O estouro não é hipótese; é aritmética a partir da reta medida.**

No T4.1 a **q32 completou em 290,5 s**.

O que eu não vi: a aritmética pressupunha que o estado **tem de caber na pool**. Esse pressuposto era verdade
no caminho *eager* — um único batch gigante, sem ponto de parada onde derramar. O próprio fix do milestone
trocou o regime de entrega para *streaming*, e com isso o DataFusion ganhou a capacidade de **derramar para
disco**. O teto deixou de ser a pool e passou a ser o disco.

A confirmação de que o mecanismo é esse vem de ele ter **dois sinais opostos na mesma corrida**: o spill
*salva* a q32 e *quebra* q08/q09 (`COUNT(DISTINCT)`), que passam a esgotar o soft limit de 1024 descritores
criando arquivos de partição. Uma explicação que só serve para o caso favorável é suspeita; uma que prevê o
dano colateral é evidência.

## Por que a forma engana

A previsão tinha todos os sinais externos de rigor — número medido, unidade, cinco pontos, linearidade
verificada. O que faltava não era medição: era a pergunta **"o mecanismo do regime-alvo é o mesmo do regime
medido?"**. Nenhuma quantidade de rigor no eixo errado responde a essa pergunta.

Agrava que **eu mesmo mudaria o regime** — a extrapolação foi feita sobre um sistema que o milestone estava,
naquele momento, alterando. Prever o comportamento futuro de um sistema usando uma reta medida no sistema
anterior à mudança é o caso mais fácil de cometer e o mais fácil de evitar.

## Como não repetir

Antes de transformar uma reta medida em previsão de falha:

1. **Nomeie o mecanismo** que produz a reta ("o estado tem de caber na pool"), não só a forma dela.
2. **Pergunte se ele sobrevive** ao regime alvo. Se o alvo é um código que você está mudando, a resposta padrão
   é *não sei* — não *provavelmente sim*.
3. **Prefira medir o caso extremo** a extrapolar até ele, quando medir é possível. A q32 era **executável**: o
   custo de rodá-la era 300 s, contra a publicação de uma conclusão errada.
4. Se a extrapolação for mesmo necessária, publique-a como **previsão com pressuposto explícito**
   ("assumindo que o estado precise caber em memória"), nunca como "não é hipótese, é aritmética".

O item 4 é o que teria bastado: a frase que escrevi fechava a porta para a dúvida em vez de registrar o
pressuposto que a sustentava.

## Relacionados

- [measurement/pico-do-groupby-e-linear-na-cardinalidade](../measurements/pico-do-groupby-e-linear-na-cardinalidade.md) — a reta, que continua válida, e a correção da previsão
- [measurement/delta-medido-m169-28-para-30](../measurements/delta-medido-m169-28-para-30.md) — a corrida que falsificou
- [failure-mode/correcao-nao-propagada-pelo-grafo](correcao-nao-propagada-pelo-grafo.md) — o que fazer DEPOIS de descobrir que um número publicado está errado
- [failure-mode/instrumento-cego-a-arquitetura](instrumento-cego-a-arquitetura.md) — o irmão: lá o instrumento não enxerga o mecanismo; aqui o raciocínio não enxerga que o mecanismo mudou
- [technique/medir-antes-de-filar](../techniques/medir-antes-de-filar.md)
