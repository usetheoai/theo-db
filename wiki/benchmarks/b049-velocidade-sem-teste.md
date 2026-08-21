---
type: Measurement
title: b049 — as diferenças de velocidade que publicamos, medidas contra o rigor que exigimos da qualidade
description: O b035 (+16,3%) sobrevive, mas com intervalo de ±8,2% em vez da precisão que o número sugere. O b047 (4,3×) tem UMA corrida por configuração e não é testável com o dado que existe. E N=2 é quase inútil — a tabela diz qual N compra qual precisão.
tags: [metodologia, significancia, velocidade, qps, honest-negative, b-049]
item: B-049
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Conceito irmão — o teste **pareado**, que serve para qualidade e não para isto:
[b047 — paridade lexical](b047-lexical-headtohead.md).

# A assimetria que este item existe para fechar

O [[B-045]] gastou um ciclo inteiro para poder dizer *"demonstrado"* em vez de *"observado"* num
**empate de terceira casa decimal**: a paridade lexical do `b047` tem p=0,477 sobre 6.980 consultas,
com IC de [−0,0011, +0,0025].

E as duas maiores diferenças que o projeto publica seguiam sem teste nenhum:

- **Elasticsearch a 4,3× o nosso QPS** no lexical (`b047`)
- **pgvector a +16,3%** a recall casado no vetorial (`b035`)

Exigir rigor onde a diferença é minúscula e dispensá-lo onde ela é de 4× é indefensável — e morde
na direção errada quando o número **melhorar**: sem teste, um ganho de 8% será tão inafirmável
quanto o déficit atual.

## Por que o pareado não servia

Ele precisa de valor **por consulta**. QPS não tem: é uma taxa agregada sobre a corrida inteira.
Aplicar o pareado a taxas inventa uma correlação que não existe e **estreita o intervalo sem razão**
— produz "significativo" onde não há nada.

O teste apropriado é para amostras **independentes**: Welch (variâncias desiguais, que é o caso
comum — o sistema mais rápido costuma ser o mais estável) mais um bootstrap sobre a **razão**, porque
é a razão que a frase publica.

# O retroativo, e ele é o achado

| alegação | corridas | CV implícito | IC a esse N | veredito |
|---|---|---|---|---|
| `b035` **+16,3%** | 2 (concordantes a 1,3%) | 0,91% | **±8,2%** | **sobrevive**, com precisão muito pior que o número sugere |
| `b047` **4,3×** | **1** por configuração | — | **não calculável** | **não testável** com o dado que existe |

O `b035` não é retratado: o intervalo não cruza zero, então a direção está estabelecida. O que muda
é o que se pode dizer — algo em torno de [8%, 24%], não "+16,3%" com a precisão que três algarismos
sugerem.

O `b047` é o caso desconfortável. Uma corrida por configuração não tem dispersão, e um intervalo
sobre uma amostra de tamanho 1 teria largura zero — que se leria como certeza absoluta. **O 4,3× não
está errado; ele está sem teste**, e a diferença importa exatamente porque é grande o bastante para
ninguém duvidar.

# Qual N compra qual precisão

Meia-largura do IC relativa à média, por `t(n−1, 0,05) × CV / √n`:

| CV entre corridas | N=2 | N=3 | N=5 | N=10 | N=20 |
|---|---|---|---|---|---|
| 2% | ±18,0% | ±5,0% | ±2,5% | ±1,4% | ±0,9% |
| 5% | ±44,9% | ±12,4% | ±6,2% | ±3,6% | ±2,3% |
| 10% | ±89,8% | ±24,8% | ±12,4% | ±7,2% | ±4,7% |
| 20% | ±179,7% | ±49,7% | ±24,8% | ±14,3% | ±9,4% |

**N=2 é quase inútil**: ±18% mesmo com CV de 2%, porque o `t` de um grau de liberdade é 12,7. **N=5
é o joelho** — de lá em diante cada corrida compra pouco.

Custo, medido e não estimado: cada corrida do caso FTS levou **~7 min** no droplet depois do dataset
em cache. Então N=5 são **~35 min por motor**, e N=10 são 70. A tabela é o que torna essa conta uma
decisão em vez de um hábito.

# Reprodução

```bash
theodb-bench throughput --a theodb --a-runs 100 102 98 101 99 \
                        --b elasticsearch --b-runs 430 435 425 432 428
```

A implementação é NumPy puro — reamostragem é um laço, e um laço não justifica uma dependência. O
`_welch_p_value` e o `_t_critical` são próprios e **validados contra referência externa**: o p bate
com o `scipy.stats.ttest_ind(equal_var=False)` a ~1e-15, e o t crítico bate com as tabelas clássicas
na quarta casa. Os dois testes de validação pulam com a razão dita quando o SciPy não está
instalado, em vez de passar em silêncio.
