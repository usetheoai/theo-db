---
type: Failure Mode
title: Uma contagem agregada credita ao fix unidades que falhavam por outra causa
description: "No baseline 100M do M169, 5 das 6 falhas eram agg=False (não roteiam) e só 1 era o defeito que o milestone conserta — mover a contagem 19→N mediria as duas classes juntas."
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

Aos 24 de 43, o baseline mediu:

| classe | n | veredito | `agg_routed` | o M169 conserta? |
|---|---|---|---|---|
| completam | 18 | `ok` | — | n/a |
| **não roteiam** | **5** | `timeout` ~300 s | **False** | **não** — caem no executor de linha |
| **defeito de escala** | **1** (q20) | `error:XX000` 52 s | **True** | **sim** — é o alvo |

**Cinco das seis falhas não são da classe do milestone.** Elas falham porque a consulta nunca entra no caminho
colunar — que é o assunto da série de cobertura de roteamento (M151…M163), não o desta. Se o T4.1 reportar
"19 → 21", duas das unidades podem ter mudado por qualquer coisa **menos** o streaming.

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
2. Publique o delta **por classe**, com o agregado como contexto e não como manchete.
3. Declare o recorte antes do número. Depois do resultado, a decomposição mais lisonjeira é a que se escreve
   sozinha — pela mesma razão que [[dod-compara-contra-o-oraculo-de-controle]] exige o braço de controle antes.

## Parentes

- [[medicao-vacuosa-aceita]] — ali a medição não exercita nada; aqui ela exercita **coisas demais**, misturadas.
- [[conflacao-ranker-com-candidate-set]] — a mesma raiz noutra métrica: o número mede um estágio e é lido como
  se medisse outro.
- [[o-sintoma-nomeia-a-fase-errada]] — o sintoma agregado ("a consulta falha") nomeia mal a fase responsável.
- [[braco-de-controle-inalterado]] — o instrumento que distingue "mudou por causa do fix" de "mudou".
