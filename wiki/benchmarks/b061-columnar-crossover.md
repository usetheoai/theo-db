---
type: Measurement
title: crossover do theodb_columnar contra o heap — e o número que o pushdown desligado derrubou
description: Com enable_columnar_agg ligado, o colunar passa o heap entre 10 mil e 100 mil linhas (1,75× a 100k). Com o default off, a mesma medição dizia 14-20× mais lento e nenhum crossover — 13× de diferença decidida por um flag.
resource: .claude/knowledge-base/reviews/b061-analytical-suite-review-2026-08-17.md
tags: [benchmark, columnar, crossover, honest-negative, retratacao, b061]
generated: { by: claude-code/opus-5, at: 2026-08-17 }
sources:
  - id: b061
    resource: .claude/knowledge-base/reviews/b061-analytical-suite-review-2026-08-17.md
    title: B-061 — review com a retratação registrada
    last_modified: 2026-08-17
---

> **Este conceito carrega uma retratação preservada.** O primeiro número medido dizia que o
> `theodb_columnar` é 14–20× **mais lento** que o próprio heap em todos os tamanhos e que **não há
> crossover**. Está errado, e é preservado porque foi reportado.

**Com o pushdown de agregação ligado, o colunar passa o heap entre 10 mil e 100 mil linhas.** Medido
no droplet efêmero `138.197.22.192`, tabelas heap e colunar no mesmo banco, mesma sessão, 5 repetições
e mediana.

# Razão heap ÷ colunar — acima de 1 o colunar vence

| linhas | `sum_amount` | `filtered_sum` | `group_by_category` |
|---|---|---|---|
| 10 000 | 0,80× | 0,62× | 0,28× |
| 100 000 | **1,75×** | **1,37×** | 0,22× |
| 1 000 000 | **1,41×** | 0,84× | **0,07×** |

# O flag que decidia 13×

`theodb.enable_columnar_agg` vem **`off`**. Mesma tabela de 1M, mesma query:

```
off (DEFAULT)  →  Seq Scan                             1407 ms
on             →  Custom Scan (theodb_columnar_agg)     108 ms
```

O catálogo reporta tabela colunar nos dois casos — `pg_class.relam = theodb_columnar` é verdade com o
flag ligado ou desligado. A primeira medição, feita no default, media **armazenamento colunar sem o
pushdown**, que é precisamente o caminho que o [M184](/benchmarks/columnar-groupby-verdict.md) já
registrava como mais lento que heap.

É a forma descrita em [o instrumento reporta o pedido](/guides/instrumento-reporta-o-pedido.md), e o
gêmeo dela no concorrente é o `scann.enable_ah_quantizer` — também `off`, também decidindo o que se
mede.

# O `GROUP BY` não é coberto pelo pushdown

O 0,07× a 1M não é ruído. O plano:

```
GroupAggregate
  ->  Sort   Sort Method: external merge  Disk: 25456kB
        ->  Seq Scan on x_columnar_1000000  (actual rows=1000000)
```

Sem pushdown, ordenação externa derramando **25 MB** em disco, contra `HashAggregate` paralelo no
heap. **Um usuário que escreva `GROUP BY` numa tabela colunar recebe 14× de piora sem aviso** — vale
mais que o crossover, e é o achado a agir.

# Ressalvas

- **O heap corre paralelo e o colunar serial** (`Workers Planned: 2` contra execução serial no
  `Custom Scan`). Parte da razão é paralelismo, não formato de armazenamento.
- 5 repetições e mediana, **sem teste de significância**. Responde ao critério de DoD *"a partir de
  quantas linhas ele vence o heap"* e nada mais.
- **Medido por script direto contra os adapters, não por bundle do arnês** — logo não é reproduzível
  por terceiros. Registrado como B-069; a regra de que toda medição publicável sai do arnês passou a
  ser invariante do `theodb-bench`.
