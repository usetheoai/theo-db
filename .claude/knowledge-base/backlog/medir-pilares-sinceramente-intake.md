---
slug: medir-pilares-sinceramente
generated_by: backlog-item
date: 2026-08-08
status: completed
verdict: ITEM_REJECTED
gates_fired: [G2, G3]
---

# Intake — "medir de forma extremamente sincera cada pilar do banco"

Primeiro uso do `BACKLOG.md` deste repositório, criado minutos antes. **Nenhum id foi alocado.**

## Gate G2 — dedup

`BACKLOG.md` está vazio (0 itens), mas o dedup não se limita a ele: **o trabalho já existe** como
`M184` em `ROADMAP.md:3748` — *"Medir cada pilar com rigor, e apurar quanto a avaliação de maturidade
errou"* —, com DoD completo, risco de viés de confirmação nomeado e exigência de declarar o que ficou
sem medir.

O próprio `BACKLOG.md` estabelece a fronteira, escrita antes deste intake: *"um item que já tem escopo
de milestone nasce no roadmap, não aqui"*. Registrar de novo criaria **dois donos para o mesmo
trabalho** — o que o G2 existe para impedir.

## Gate G3 — domínio único

"Medir **cada** pilar" abrange **oito**: vetorial, hot-path, concorrência, colunar, lexical,
ai-surface, engine-pgrx, acervo. A regra é explícita — um item que abrange dois domínios é dois itens.
Como está, não é item de backlog válido em nenhuma hipótese; só cabe como milestone, que **pode**
abranger pilares.

## O que tornaria isto um item aceitável

Não é "reescrever melhor" — é mudar a natureza do pedido. Duas formas passariam:

1. **Um pilar por item.** `B-001: medir o pilar lexical com rigor` roteia para `theo-lexical`, tem DoD
   próprio e fecha sozinho. Oito itens desses são válidos; um item com os oito não é.
2. **Uma hipótese ainda sem escopo.** O backlog é para o que precisa ser **medido antes de virar
   escopo**. O M184 já passou desse ponto — tem DoD, dependências e riscos escritos.

## Decisão

Owner escolheu **manter só o M184** (2026-08-08). A entrega real daquele milestone — a comparação
*nota atribuída × nota medida*, com a divergência nomeada — perde sentido espalhada por oito itens
independentes, porque é justamente a visão de conjunto que calibra a régua.

Nada foi escrito em `BACKLOG.md`. Este log existe para que a próxima pessoa que tiver a mesma ideia
encontre a razão da recusa em vez de refilá-la.
