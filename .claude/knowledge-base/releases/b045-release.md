---
slug: b045-significance
items: [B-045]
date: 2026-08-13
base: 1a02e66
head: 82b620e
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — o projeto volta a poder dizer se uma diferença é real

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

Nenhum gate reprovou. O merge espera aprovação humana — gate **LOCKED** do `cycle-release`, e Regra 4.

## Por que não há corte de versão novo

`cycle-release` manda não disparar com PR de release aberto. **#227** e **#228** seguem abertos; o B-045
entra na `[0.160.0]`, que passa a cobrir **oito** itens.

## O que foi entregue

| | |
|---|---|
| `benchmarks/significance/significance.py` | **recuperado byte a byte** de `7cd157d^` — permutação pareada + bootstrap + t |
| `benchmarks/significance/per_query.py` | o avaliador que nunca existiu para o VectorDBBench |
| `benchmarks/significance/compare.py` | alinhamento por `qid`, comparação N-ária, arrays persistidos |
| `benchmarks/significance/run_lexical_significance.py` | aplica aos motores reais **e verifica contra o agregado publicado** |
| Testes | **22**, sem banco e sem rede |
| Alteração no fork | **zero** |
| Alteração na extensão | **zero** |

## O resultado

Permutação pareada, 100.000 reamostragens, **n = 6.980 consultas**:

| comparação | diff médio | IC 95% | p | V/D/E |
|---|---|---|---|---|
| TheoDB vs Elasticsearch | +0,00066 | [−0,0011, +0,0025] | **0,477** | 233 / 263 / 6.484 |
| TheoDB vs OpenSearch | +0,00068 | [−0,0011, +0,0025] | **0,466** | 235 / 268 / 6.477 |

**A paridade do b047 sobreviveu**, e do jeito forte: IC estreito centrado em zero, com 6.484 das 6.980
consultas empatando exatamente.

## O que este ciclo produziu além do código

**Uma defesa anterior salvou este item.** O guard construído no B-044 (e registrado como [[B-041]]) recusou
buscar num índice não construído quando apontei o avaliador para a coleção errada. Sem ele: 6.980 buscas
vazias, NDCG 0 para o TheoDB, e um `p` dizendo que o Elasticsearch é dramaticamente superior.

**A verificação contra o agregado é o que torna o `p` confiável.** Os três motores reproduziram o número
publicado antes de qualquer `p` sair. Um `p` correto sobre números que não são os da tabela teria a aparência
de rigor e a substância de outra medição.

**A ferramenta distingue os dois significados de um `p` alto** — IC estreito é equivalência, IC largo é falta
de poder. Tratá-los como iguais é como se afirma paridade sem tê-la medido.

## Followups

- **Significância para velocidade.** QPS não tem valor por consulta; o pareado não se aplica. O caminho é N
  corridas repetidas por configuração, e é item próprio — os 4,3× do b047 seguem sem teste.
- **O +5,6% do stemming** segue observado: os arrays do lado sem stemming não foram preservados.
- **[[B-046]]** — paridade de QPS vetorial, bloqueado por [[B-036]].
- **B-047** — falta o eixo vetorial (fronteira de Pareto dos dois lados).
- **B-029** — CI vermelho.

## O que NÃO foi feito

Nenhuma tag. Nenhum release publicado. `develop` e `main` intocados. O droplet efêmero foi destruído com a
chave SSH (verificado: listagem por tag vazia).
