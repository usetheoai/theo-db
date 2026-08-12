---
type: Measurement
title: B-015 — os cinco contadores em zero eram DUAS causas, e a hipótese registrada estava errada nas duas
description: Três falhas eram o fixture do teste (pg_test roda em transação, o colunar nunca materializa stripe) e duas eram instrumentação perdida por um caminho de scan novo; a hipótese de paralelismo que o item carregava não explica nenhuma delas.
resource: BACKLOG.md
tags: [benchmark, b-015, colunar, autotune, hnsw, instrumentacao, honest-negative, wiring]
generated: { by: claude-code/opus-5, at: 2026-08-11T13:00:00Z }
---

Cinco testes falhavam com **contador em zero** — `pages_read`, `chunks_scanned`, `chunks_skipped` — em três
módulos. A assinatura compartilhada sugeria causa comum, e o item registrou uma hipótese explícita:
os contadores são `thread_local`, o seed usa 50 000 linhas, o PostgreSQL escolheria varredura **paralela**, e
o líder leria zero porque a poda aconteceu nos workers.

**A hipótese está refutada, e não havia causa comum.** São duas causas independentes, e nenhuma é
paralelismo.

# Por que a hipótese de paralelismo não se sustenta

Duas razões, ambas verificáveis sem rodar nada de novo:

1. O plano medido é `Custom Scan (theodb_columnar_project)`, e ele **não paraleliza** — os contadores são
   idênticos com `max_parallel_workers_per_gather` em 0 e em 4.
2. O próprio `seed_clustered` **já executava** `SET max_parallel_workers_per_gather = 0`
   (`columnar_project.rs:840`), de modo que os testes nunca correram sob paralelismo desde que foram escritos.

A hipótese foi lida no código e registrada como plausível; a medição a derrubou. Fica aqui em vez de ser
apagada porque ela baseou a prioridade do item.

# Causa A — os três testes colunares: o fixture, não o produto

`#[pg_test]` roda cada teste dentro de **uma transação**, revertida ao fim. O escritor colunar segura linhas
no *pending set* e só materializa um stripe durável quando o buffer excede `maintenance_work_mem` ou no
pre-commit (M104). Sob os 64 MB default, 50 000 linhas estreitas não alcançam o limite, e o commit nunca
chega — então **não existe chunk-group para o zone-map podar**.

Medido no binário shipado (`0.140.0`), mesma sessão, mesmas linhas:

| condição | `skipped` | `scanned` |
|---|---|---|
| dentro da transação (o que o `pg_test` faz) | 0 | 0 |
| após `COMMIT` | 4 | 5 |
| dentro da transação, com `maintenance_work_mem = '64kB'` | **30** | **31** |

**O zone-map estava podando o tempo todo.** O produto está correto; o fixture é que nunca produzia o que ele
poda. A correção baixa o `maintenance_work_mem` no seed, para que o flush incremental que o produto já
implementa de fato ocorra no teste — e **não** afrouxa a asserção, rota que o DoD do item proibia
explicitamente por esconder uma feature funcionando atrás de um teste verde.

# Causa B — os dois testes de autotune: instrumentação perdida por um caminho novo

`theodb.explain_scan` devolvia `pages_read=0` e `candidates_seen=0` **enquanto o plano usava o índice e a
consulta devolvia as linhas certas** — `results=5`, `lat_us` não-trivial, `Index Scan using ... Order By` no
plano, em 2 000 e em 20 000 linhas, com e sem paralelismo.

A causa é uma bifurcação em `am/scan.rs:318-349`: o `amrescan` tenta `resumable_init` (M118) e só cai em
`gather_hnsw_candidates` → `traverse` quando aquele devolve `None`. O caminho *resume* **duplica** a descida
gulosa do `traverse` — o comentário do próprio código diz *"byte-identical to `traverse`"* — acumula seu
próprio `reads` e **nunca chamou os dois `bump_*`**. Ele é o **default para todo índice V1 exact-f32**.

O experimento que isolou a causa é uma variável só, o kill-switch do M118:

| `theodb_hnsw.resume` | `pages_read` | `candidates_seen` |
|---|---|---|
| `off` | 112 | 38 |
| `on` (default) | **0** | **0** |

`ResumableGround::candidates_seen()` (`ann/scan_core.rs:227`) já existia, documentado como *"observability
parity with `candidates_seen`"* — **um acessor público com zero callers**. A paridade foi prevista, escrita, e
nunca ligada.

# O que isto custou além dos dois testes

`theodb.explain_scan`, `theodb.scan_stats` e o coletor `theodb._index_scan_stats` reportaram **zero** para o
scan mais comum do produto, e **o recomendador de `ef_search` consumiu esses zeros**. Qualquer recomendação
de autotune emitida enquanto o defeito existiu foi calculada sobre dado falso — o alcance disso não foi
medido e merece item próprio.

# A lição, e ela não é sobre este defeito

Os dois testes de autotune asseriam `pages > 0` **sem fixar qual caminho** produziu o número. Com dois
caminhos de scan e a asserção satisfeita por qualquer um, o caminho default pôde ficar cego sem que nenhum
teste percebesse. A correção ship com um teste que faz o A/B explícito sobre o kill-switch: revertido o fix,
a metade `on` falha e a metade `off` continua passando — que é exatamente a assimetria que o teste anterior
não conseguia ver.

Vale para além do vetorial: **uma métrica de wiring que um caminho novo pode deixar de emitir não está
protegida por um teste que aceita qualquer caminho.**

# Links

* [m188 — as 20 falhas da suíte, classificadas](m188-suite-18-falhas-classificadas.md)
* [m175 — a inversão de custo do planner](m175-planner-cost-inversion-verdict.md)
