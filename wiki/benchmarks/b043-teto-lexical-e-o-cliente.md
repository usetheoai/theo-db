---
type: Measurement
title: b043 — o teto de vazão lexical é o CLIENTE do arnês, não o servidor
description: RETRATADO. O colapso de 61% e os 6,5× eram artefato do MEU desenho de medição — contagem de operações fixa independente do número de clientes. Corrigido, a curva sobe e satura, e a razão é ~1,27×.
tags: [lexical, vazao, concorrencia, arnes, gap, b-043, honest-negative]
item: B-043
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Peça: [runbook do droplet](../runbooks/droplet-de-medicao.md).

> # ⚠ RETRATAÇÃO — 2026-08-21, poucas horas depois de publicar (e depois de lançar em `v0.166.0`)
>
> **A conclusão deste documento está errada, e o erro é meu.** Ele afirma que o arnês colapsa 61% e
> é 6,5× mais lento que o `pgbench`, e conclui que o teto de vazão lexical é o cliente. **O colapso
> era artefato do desenho da medição.**
>
> O `run_concurrent` que escrevi emitia um total **FIXO** de operações — 300 — independente do número
> de clientes. A 80 clientes isso são **3,75 consultas por cliente**, e a abertura de conexão domina
> a janela medida inteira. Medido lado a lado, no mesmo processo:
>
> | clientes | total **fixo** em 300 | 300 **por** cliente |
> |---|---|---|
> | 5 | 598,6 | 646,1 |
> | 20 | 570,2 | 801,8 |
> | **80** | **277,7** | **827,0** |
>
> Com a contagem escalando, a curva **sobe e satura** em vez de colapsar. Re-medido pelo benchmark
> corrigido: 354 · 1.020 · 1.262 · 1.225 · **1.302** · 1.281 — pico em 40 clientes e queda de 2% até
> 80. **Isso é saturação, e é a mesma forma que o `pgbench` mostra.**
>
> **O número correto da razão é ~1,27×**, não 6,5×: o `pgbench` satura em ~1.630 na mesma máquina.
>
> **O que isso faz com a pergunta do [[B-043]].** O item perguntava se a saturação em ~20 clientes
> era do servidor ou do cliente. A resposta que este documento deu — "do cliente" — não se sustenta:
> os dois geradores saturam, e a distância entre eles é de 27%. **O platô é real e é do servidor.**
>
> **Duas coisas que também caíram no caminho:**
>
> - **O GIL está REFUTADO como causa.** Processos não são mais rápidos que threads: 0,98× a 20
>   clientes, 1,00× a 40. Se o GIL fosse o teto, processos escalariam e threads não.
> - **Os "dois relógios" que acrescentei não discriminam em laço fechado.** `response` e `service`
>   saem IDÊNTICOS em todos os pontos, porque sem agendamento não há atraso contra o qual medir. Eu
>   afirmei que eles separariam fila de servidor lento; eles só o fazem em laço aberto.
>
> **Por que isto fica escrito em vez de reescrito.** O documento foi citado no `BACKLOG.md`, no
> `CHANGELOG.md` e lançado em `v0.166.0`; apagar esconderia que a alegação circulou. E o defeito é
> instrutivo: eu fixei a contagem de operações para "comparar o mesmo trabalho" entre populações de
> cliente — e comparar o mesmo trabalho total entre populações diferentes é justamente o que **não**
> se pode fazer em laço fechado.
>
> O corpo abaixo fica como estava. Os números do `pgbench` nele são válidos; os do arnês, não.



# A pergunta que o item fez, e por que ela era boa

O [[B-043]] mediu que o QPS lexical satura em ~20 clientes e recusou-se a publicar a curva sem
investigar, porque *"há pelo menos três candidatas que exigem instrumentos diferentes: saturação de
CPU real, contenção no índice lexical compartilhado, ou **teto do cliente Python do arnês**"*.

O DoD exigiu o experimento que separa as três: **um gerador de carga que não seja o arnês**, no mesmo
corpus e na mesma máquina.

# O resultado

Droplet `g-16vcpu-64gb` (nyc1), TheoDB 1.5.0, SciFact (5183 documentos, 300 consultas julgadas),
`max_connections=300`. Arnês: `retrieval/scifact/concurrency`, laço fechado. Externo: `pgbench` com a
mesma função `bm25_search`, 20 s por ponto.

| clientes | **pgbench** TPS | **arnês** QPS | o arnês é |
|---|---|---|---|
| 1 | 555,8 | 338,5 | 1,64× mais lento |
| 5 | 2.340,1 | 1.151,4 | 2,03× |
| 10 | 3.559,1 | **1.617,7** (pico) | 2,20× |
| 20 | **4.159,5** | 965,7 | **4,31×** |
| 40 | 4.135,7 | 792,5 | 5,22× |
| 80 | 4.071,7 | 623,4 | **6,53×** |

**O servidor não colapsa.** O `pgbench` estabiliza em ~4.150 TPS a partir de 20 clientes e **fica lá**
— 4.159 → 4.136 → 4.072, queda de 2% até 80 —, com a latência média crescendo de forma limpa e
linear: 1,8 ms · 2,1 · 2,8 · 4,8 · 9,7 · 19,6. Isso é **fila contra capacidade fixa**, que é
exatamente o que um servidor saudável faz quando os clientes passam dos núcleos.

**O arnês colapsa.** Pico em 10 clientes e queda de **61%** até 80.

# A conclusão, e ela é sobre nós

**A hipótese do cliente Python está CONFIRMADA.** O teto que o item mediu não era do banco — era do
instrumento que o mediu. E a diferença não é pequena: **6,5× a 80 clientes**, e já **1,64× com um
cliente só**.

Isso reenquadra o número original do item. A "saturação em ~20 clientes" medida no MS MARCO em
2026-08-13 é, muito provavelmente, a mesma curva do cliente — e não uma propriedade do pilar lexical.
O teto real do servidor, neste corpus, é **~4.150 TPS com enfileiramento limpo**.

# Ressalvas declaradas, e uma delas é séria

- **O `pgbench` emite uma consulta FIXA; o arnês varia entre 300 consultas julgadas.** Consulta fixa
  aquece cache de plano e de dado de um jeito que consulta variada não aquece. Isso infla o
  `pgbench` por uma fração que **não está medida**. O que a ressalva **não** explica é a *forma*: um
  cliente que colapsa 61% enquanto o outro fica plano não é diferença de variedade de consulta, e é a
  forma que sustenta a conclusão.
- Corpus **SciFact (5183 docs)**, não o MS MARCO 100K do item. Os absolutos não se comparam com os de
  2026-08-13; a comparação que vale é a **interna**, arnês contra pgbench na mesma máquina e corpus.
- Veredito do arnês `EXPLORATORY`; todos os pontos marcados `(unstable)`.
- `-j 8` no `pgbench` (8 threads de cliente). Com `-j 1` o próprio pgbench seria o gargalo acima de
  ~10 clientes — o que é a mesma classe de defeito que este documento reporta, no outro instrumento.

# O que isto abre

O arnês é o produto que este projeto publica (`docs/methodology/PUBLICATION.md` diz que medição fora
dele não é publicável). **Um arnês que é 6,5× mais lento que o `pgbench` sob carga não consegue medir
o teto de vazão de nada** — ele mede a si mesmo. Ver [[B-094]].

Artefato: `benchmarks/artifacts/20260821T130817Z-retrieval-scifact-concurrency-theodb-51325815/`.

# Reprodução

```bash
theodb-bench run retrieval/scifact/concurrency --system theodb --profile research --dataset beir-scifact
# e, na mesma maquina:
pgbench -n -f bm25.sql -c $C -j 8 -T 20 -U postgres postgres
```
