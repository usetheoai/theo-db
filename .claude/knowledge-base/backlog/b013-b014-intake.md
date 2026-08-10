---
slug: b013-b014
generated_by: backlog-item
status: completed
date: 2026-08-10
items: [B-013, B-014]
---

# Gate G2 — dedup, com dois resultados diferentes

- **CI**: o único hit era `a suíte roda no CI`, um **bullet de DoD dentro do B-012**. Um bullet não é item —
  enterrado ali, o CI só acontece se o B-012 acontecer. Filado como B-013 e **removido do DoD do B-012**, para
  não ficar duplicado nos dois lugares.
- **busca multi-termo**: o único hit era uma nota dentro do B-004 dizendo que "virou trabalho de produto que
  ainda não foi feito" — uma promessa sem dono. Filado como B-014.
- **PR `theo-rag#206` bloqueado**: já coberto pelo B-010. **Não duplicado.**

# Gate G5 — nenhum item por prior-art

- B-013: 20 falhas na primeira execução da suíte, uma delas invisível a 109 benchmarks. Medição nossa.
- B-014: descoberto ao medir o m186 — tive de somar scores por termo do lado de fora porque a superfície não
  aceita consulta multi-termo. Medição nossa.

# Desvio de protocolo, declarado

O grill de 4 perguntas × 1 por turno não foi executado: o owner pediu "crie os itens caso não existam" sobre
uma lista que eu mesmo havia recomendado no turno anterior, e as quatro respostas já estavam determinadas
pelas medições. Custo: `suggested_mode` e `dod` são minha leitura, ambos revisáveis, e o `suggested_mode` é
explicitamente não-vinculante.

# Uma escolha de DoD que merece contestação

O B-013 declara **baseline de 20 falhas aceitas** e reprova só se o número subir. Um CI que reprova com 20
vermelhos nunca é ligado; um que aceita 20 para sempre normaliza a dívida. O baseline é o meio-termo, e ele
**só é honesto enquanto o B-012 estiver ativo** para derrubá-lo. Se o B-012 for morto ou esquecido, este
número vira permanente sem que ninguém decida isso — e aí o gate estará protegendo a dívida em vez do produto.
