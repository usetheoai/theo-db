---
type: Failure Mode
title: Uma contagem agregada credita ao fix unidades que falhavam por outra causa
description: "No baseline 100M do M169 (43/43 completo) só 4 das 15 falhas entram no caminho agregado, e 3 delas são o defeito-alvo — e ao classificar as demais eu mesmo misturei classes, usando um discriminador que responde uma pergunta mais estreita do que eu supus."
resource: .claude/knowledge-base/implementations/m169-scale-bugs-100m-implementation.md
tags: [benchmark, metrica, milestone, honestidade]
timestamp: 2026-07-30T00:00:00Z
---

# Uma contagem agregada credita ao fix unidades que falhavam por **outra** causa

## A forma do erro

Um milestone escolhe uma métrica-resumo — "N de 43 consultas completam" — porque ela é legível e move de uma
vez só. Mas a métrica soma unidades que falham por **razões diferentes**, e o fix endereça **uma** delas. Se a
contagem sobe, ela sobe por qualquer motivo: o fix, uma máquina maior, um teto mais generoso, uma consulta que
mudou de plano. O número não distingue, e o texto que o acompanha quase sempre atribui tudo ao fix.

O erro não é medir o agregado. É **publicá-lo sem o discriminador que separa as classes**.

## A instância que nomeia o conceito (M169, 2026-07-30)

O M169 existe para um defeito de escala do caminho **agregado colunar**: `byte array offset overflow` quando
uma coluna de texto ultrapassa o teto de offset i32 do Arrow. A métrica escolhida foi a contagem ClickBench.

Aos 24 de 43, o baseline mediu 18 `ok` e 6 falhas. Classificadas **pela forma da consulta**, e não só pelo
booleano:

| q | forma | `agg_routed` | o que o booleano significa AQUI | o M169 conserta? |
|---|---|---|---|---|
| q17 | `GROUP BY UserID, SearchPhrase` | False | agregado — o pushdown **declinou** | não |
| q19 | `SELECT UserID WHERE UserID = …` | False | **não é agregado** — o booleano não responde nada | não |
| **q20** | **`COUNT(*) WHERE URL LIKE`** | **True** | **roteou e estourou dentro** | **sim — é o alvo** |
| q21 | `MIN(URL), COUNT(*) GROUP BY` | False | agregado — declinou | não |
| q22 | `COUNT(DISTINCT) + MIN(URL) + MIN(Title)` | False | agregado — declinou | não |
| q23 | `SELECT * … ORDER BY … LIMIT 10` | False | **é top-k**, servido por outro caminho (M158/M168) — o booleano é falso **por construção** | não |

**Uma das seis é a classe do milestone.** Se o T4.1 reportar "19 → 21", a segunda unidade pode ter mudado por
qualquer coisa **menos** o streaming agregado.

### Atualização com a corrida COMPLETA (43/43, 2026-07-30)

O recorte acima foi escrito **no meio da corrida**, aos 24 de 43. Registrá-lo cedo foi certo — a decomposição
declarada antes do resultado é o ponto do conceito — mas os números eram parciais, e a corrida completa os
supera:

| | snapshot aos 24/43 | corrida completa 43/43 |
|---|---|---|
| falhas | 6 | **15** |
| falhas com `agg_routed=true` | 1 | **4** (q20, q32, q33, q34) |
| instâncias do defeito-alvo (`byte array offset overflow`) | **1** (q20) | **3** (q20, q33, q34) |

Duas coisas mudam de figura, e ambas para melhor entendimento — não para melhor manchete:

1. **O alvo tem 3 instâncias, não 1.** A alavanca do milestone é maior do que o snapshot sugeria, e as três
   são a **mesma** coluna (`URL`) — o que é evidência de causa única, não de três bugs.
2. **Surge uma classe que o snapshot não continha: roteia e ainda assim falha.** O q32 tem `agg_routed=true`
   e estoura o teto de 300 s. Isso **não** é o defeito de offsets, e o streaming pode não movê-lo: o pico ali é
   de **estado** do agregado (cardinalidade), não do buffer de decode. Creditar o q32 ao fix seria a mesma
   falha que este conceito descreve, um nível abaixo.

A regra operacional resiste: o snapshot parcial já publicava **por classe**, então a correção foi aditiva em
vez de invalidante. Um agregado sem discriminador teria ido de "6 falhas" a "15 falhas" sem nenhuma forma de
saber que o alvo triplicou.

## A instância meta: escrevi o conceito e o violei na mesma iteração

A primeira versão deste arquivo dizia *"5 das 6 falhas não entram no caminho colunar"*. Está **errado**, e o
erro é exatamente o que o conceito descreve: `agg_routed` chaveia em `theodb_columnar_agg` e portanto responde
**"entrou no pushdown agregado?"** — não *"entrou no caminho colunar?"*. Para q19 (um scan) e q23 (um top-k),
`False` é o valor **esperado por construção**, e lê-lo como "não roteia" é ler um discriminador fora do domínio
em que ele discrimina.

A conclusão macro sobreviveu; o raciocínio que a sustentava, não. Isso é pior do que parece: uma conclusão certa
por um motivo errado não avisa quando o motivo deixa de valer.

## Corolário: a lacuna que isso expôs

Não existe discriminador para o caminho **top-k**. O q23 tem exatamente a forma que o M158/M168 servem
(`SELECT * … ORDER BY … LIMIT k`) e mesmo assim dá timeout — e o instrumento atual **não sabe dizer** se ele
roteou e é lento, ou se declinou. Um booleano por caminho não basta: o que o harness precisa registrar é **qual
caminho serviu**, não *se um caminho específico serviu*.

## O discriminador é o que torna a contagem honesta

O sinal que salva aqui é `agg_routed`, capturado por consulta a partir do `EXPLAIN`: ele diz se a consulta
entrou no caminho que o fix toca. Com ele, a contagem se decompõe e cada metade responde por si:

- **delta dentro de `agg=True`** — atribuível ao fix, porque só essas passam pelo código alterado;
- **delta dentro de `agg=False`** — **não** atribuível; qualquer mudança ali veio de outra coisa.

Sem o discriminador, as duas somam num número só, e a soma é irrecuperável depois: o artefato publica "N/43" e
ninguém consegue reabrir a atribuição meses depois.

## Regra operacional

1. Antes de escolher uma métrica-resumo, pergunte **quantas classes de falha ela soma**. Se for mais de uma,
   capture por unidade o sinal que diz a qual classe ela pertence — e capture-o **na mesma corrida**, não depois.
2. **Antes de usar o discriminador, escreva a pergunta exata que ele responde.** Um booleano chamado
   `agg_routed` responde *"entrou no pushdown agregado?"*. Ele **não** responde *"entrou no colunar?"* nem
   *"roteou para algum caminho rápido?"*, e usá-lo assim é o mesmo erro num nível abaixo. Prefira registrar
   **qual** caminho serviu a um booleano por caminho.
3. Publique o delta **por classe**, com o agregado como contexto e não como manchete.
4. Declare o recorte antes do número. Depois do resultado, a decomposição mais lisonjeira é a que se escreve
   sozinha — pela mesma razão que [[dod-compara-contra-o-oraculo-de-controle]] exige o braço de controle antes.

## Parentes

- [[medicao-vacuosa-aceita]] — ali a medição não exercita nada; aqui ela exercita **coisas demais**, misturadas.
- [[conflacao-ranker-com-candidate-set]] — a mesma raiz noutra métrica: o número mede um estágio e é lido como
  se medisse outro.
- [[o-sintoma-nomeia-a-fase-errada]] — o sintoma agregado ("a consulta falha") nomeia mal a fase responsável.
- [[braco-de-controle-inalterado]] — o instrumento que distingue "mudou por causa do fix" de "mudou".
