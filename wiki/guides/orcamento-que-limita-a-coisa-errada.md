---
type: Guide
title: um orçamento que limita a coisa errada aborta a medição e acusa o inocente
description: A carga de 20M abortou em COPY linha 4 569 000 porque rodava sob o orçamento de consulta. O arnês classificou certo — budget_exceeded, não crash — e é essa distinção que separa "nosso limite" de "o sistema falhou".
resource: theodb-bench @ workspace 2026-08-17
tags: [arnes, metodologia, escala, honestidade, timeout, b073]
generated: { by: claude-code/opus-5, at: 2026-08-17 }
sources:
  - id: bench
    resource: theodb-bench @ workspace 2026-08-17
    title: theodb-bench — orçamento de carga separado do de consulta
    last_modified: 2026-08-17
---

**Um `statement_timeout` protege contra uma consulta desgovernada. Aplicado a uma carga de dados, ele
limita a coisa errada** — e a corrida morre por decisão nossa, não por falha do sistema medido.

Medido em 2026-08-17 carregando 20 000 000 de vetores: a corrida abortou em
`COPY bench_vectors, line 4569000`.

# Por que é a coisa errada

| | orçamento de consulta | orçamento de carga |
|---|---|---|
| protege contra | uma busca que não termina | nada — a duração **é** a escala |
| duração esperada | milissegundos | minutos a horas |
| está na janela medida? | sim | **não** |

A duração de uma carga é **propriedade do tamanho, não sintoma**. Cortá-la em 60 s não detecta patologia
nenhuma; só impede escala. O build de índice já tinha orçamento próprio pela mesma razão — e a razão
estava escrita duas vezes, aplicada uma. Agora é **um mecanismo só** (`_under_bulk_budget`), porque build
e carga são a mesma classe de trabalho: não medido, em massa, duração proporcional ao tamanho.

O orçamento estreito volta no `finally` das duas. Deixar a hora larga no lugar tornaria a **próxima
consulta medida** efetivamente ilimitada — que é exatamente a propriedade emprestada, não descartada.

# O que salvou a leitura: a classificação do aborto

O veredito não foi "o sistema caiu". Foi:

```
run aborted (budget_exceeded): QueryCanceled: canceling statement due to statement timeout
CONTEXT:  COPY bench_vectors, line 4569000
a statement was cancelled by a time budget the harness itself set. Raise the
budget for this phase or reduce the scale; the system under test did not fail
```

**Essa última frase é o valor inteiro.** As três classes de aborto (`CRASHED`, `REFUSED`,
`BUDGET_EXCEEDED`) existem porque colapsá-las produz a falha mais cara que um benchmark pode ter:
publicar *"o TheoDB não aguentou 20M"* quando o que não aguentou foi um timeout que nós escrevemos.

O log do PostgreSQL confirmou: **nenhum crash**, nenhum OOM, nada. O servidor estava vivo e obediente —
cancelou porque mandamos cancelar.

E a ordem de classificação é carregada: em psycopg, `QueryCanceled` **é subclasse** de
`OperationalError`. Checar perda-de-conexão antes de cancelamento reportaria este aborto como
`CRASHED` — o veredito falso, com a mesma aparência de rigor.

# A lição, e ela não é sobre timeouts

É a mesma de [o instrumento reporta o pedido](/guides/instrumento-reporta-o-pedido.md), do outro lado:
lá, o instrumento respondia sobre o pedido em vez do efeito; aqui, **o instrumento interrompeu o
experimento e a pergunta era de quem foi a culpa**. Nos dois casos o número saía com cara de válido.

Um arnês que vai ser publicado precisa distinguir três coisas que se parecem no console:

1. o sistema medido falhou → achado sobre o produto;
2. **nós** o interrompemos → achado sobre o arnês, e nenhum sobre o produto;
3. o arnês se recusou a medir → nem um nem outro, e é a recusa que impede o número falso.

Colapsar 2 em 1 é caluniar o sistema sob teste. Colapsar 3 em qualquer coisa é publicar assim mesmo.

# A terceira classe apareceu na corrida seguinte, e custou 31 minutos

A mesma escala de 20M, com o orçamento já corrigido, terminou assim:

```
run aborted (refused): ConfigError: this benchmark streams its corpus of
20000000 vectors and cannot hand it over as one array
the harness refused to measure: a precondition it checks was not met, so no
number was taken. This is the harness working, not a fault of the system under test
```

A recusa é do próprio guarda que impede materializar 10,2 GB — e ele disparou **depois** da carga, do
aquecimento e das consultas, porque a validação de recall conferia os ids devolvidos contra
`self.corpus.shape[0]` em vez do `row_count` do binding. Correção de uma linha, encontrada em 31 minutos.

**O que vale mais que a correção é por que a suíte não pegou.** O teste ponta-a-ponta construía o
benchmark, conferia o oráculo, e parava. Ele afirmava o **setup** e *lia* como se cobrisse o caminho — o
recall é computado dentro de `measure`, que nada exercitava sobre corpus em streaming.

É a mesma família de `cobertura-alegada-sem-execucao`: um teste verde que prova menos do que seu nome
sugere. E a assimetria é a de sempre — a suíte roda em milissegundos, a corrida real em meia hora.
