---
type: Decision
title: ADR-0061 — Todo pilar mensurável tem benchmark oficial público, rodado contra concorrentes na mesma máquina
description: Nenhuma alegação de capacidade de um pilar entra em documento público sem uma corrida em arnês de terceiros, com os concorrentes medidos na mesma máquina e na mesma corrida.
tags: [decision, benchmark, metodologia, vectordbbench, adr]
status: stable
date: 2026-08-13
generated: { by: claude-code/opus-5, at: 2026-08-13T02:00:00Z }
---

# Contexto

Até 2026-08-12 o projeto tinha 164 medições publicadas em `wiki/benchmarks/` e **nenhum arnês** — a suíte
`benchmarks/` foi removida em `7cd157d` e o caminho de reprodução saiu com ela. Os ciclos B-035 e B-040
devolveram um instrumento: um cliente próprio no [VectorDBBench](https://github.com/zilliztech/VectorDBBench)
(MIT), em fork de diff mínimo, cobrindo o pilar vetorial e o lexical.

As duas primeiras corridas — [b035 — TheoDB × pgvector](../benchmarks/b035-theodb-vs-pgvector-pg18.md) e
[b040 — BM25 no MS MARCO](../benchmarks/b040-theodb-fts-msmarco.md) — ensinaram três coisas que motivam
esta decisão:

1. **A configuração "igual dos dois lados" pode ser a armadilha, não a comparação justa.** Com
   `ef_search=64` em ambos os motores, o TheoDB parecia 26% mais rápido — enquanto entregava recall 0,96
   contra 0,9835. A recall casado, o pgvector é 16,3% mais rápido.
2. **Números de arnês próprio não são comparáveis com números de arnês alheio.** O [artefato do pilar lexical](../benchmarks/b040-theodb-fts-msmarco.md)
   deliberadamente **não** cita o leaderboard público da Zilliz, porque aquelas corridas foram feitas em
   outras máquinas, versões e datas.
3. **Uma métrica sozinha esconde a outra.** QPS sem recall, ou NDCG sem o analisador declarado, permitem que
   um motor pareça melhor por estar fazendo menos trabalho.

# Decisão

**Todo pilar que faz alegação de capacidade tem benchmark oficial público**, e essa alegação só existe com:

1. **Arnês de terceiros, não caseiro.** O instrumento é o VectorDBBench enquanto ele cobrir o pilar. Um arnês
   próprio mede o que escolhemos medir; um de terceiros mede o que a indústria escolheu.
2. **Concorrentes na mesma corrida e na mesma máquina.** Não se cita número publicado de outro ambiente. Se
   o concorrente não rodou aqui, o artefato diz que **não há comparação** — como o `b040` faz.
3. **Métrica de qualidade ao lado de toda métrica de velocidade.** Recall junto de QPS no vetorial; NDCG, MRR
   e recall junto de QPS no lexical. Latência sozinha não é resultado.
4. **Ponto de operação casado, não parâmetro casado.** Comparação de velocidade só a qualidade equivalente.
5. **Máquina de referência declarada.** `g-16vcpu-64gb` — o `16c64g` que é o rótulo de referência do próprio
   upstream. Droplet efêmero, IP citado no artefato, destruído ao fim.
6. **O que a corrida não cobre é publicado com o que ela cobre**, e antes dos números quando for handicap
   (o `b040` abre declarando a ausência de stemming, porque os motores comparáveis stemmizam).

# Consequências

**Aceitas:**

- Cada corrida custa tempo e dinheiro — entre US$ 1 e US$ 2 e de 15 a 30 minutos por droplet efêmero, medido.
  É barato o bastante para ser rotina.
- Publicar contra concorrentes significa publicar quando estamos atrás. O
  [b035](../benchmarks/b035-theodb-vs-pgvector-pg18.md) já fez isso, e o
  [veredito M73](0035-m73-northstar-vector-verdict.md) é o precedente de declarar um limite por medição.
- O arnês escolhido governa o que é mensurável: [[B-036]], [[B-037]] e [[B-038]] existem porque lacunas de
  compatibilidade cortam a superfície que o arnês alcança.

**Rejeitadas, e por quê:**

- *Reconstruir um arnês próprio.* Foi o que existia e foi removido. Um arnês caseiro não tem clientes dos
  concorrentes, e sem eles a comparação não acontece.
- *Citar o leaderboard público como comparação.* É o erro que esta decisão existe para barrar.
- *Compensar limitações no cliente do arnês* — stemmar a consulta em Python, reordenar resultados. Mediria o
  adaptador em vez do motor, e publicaria um número que a instalação real não reproduz.

# Limite honesto desta decisão

**Ela não é mecanizada.** Nenhum hook verifica que um artefato em `wiki/benchmarks/` tenha corrida de
concorrente, métrica de qualidade ou ponto casado. É contrato instruction-grade, como os degraus da parsimony
ladder — e afirmar mecanização inexistente seria o defeito `cobertura-alegada-sem-execucao` que o próprio
acervo documenta.

**E ela ainda não inclui significância estatística.** O VectorDBBench não tem teste pareado; o `theodb_bench`
removido tinha. Enquanto [[B-045]] estiver aberto, toda diferença publicada é **observada**, não demonstrada
— e os artefatos dizem isso. Fechar o B-045 é o que promove esta decisão de "medimos com rigor" para
"medimos com rigor estatístico".

# Itens que esta decisão governa

[[B-042]] (build vetorial) · [[B-043]] (saturação lexical) · [[B-044]] (stemming) · [[B-045]] (significância)
· [[B-046]] (paridade de QPS) · [[B-047]] (concorrentes na mesma máquina).
