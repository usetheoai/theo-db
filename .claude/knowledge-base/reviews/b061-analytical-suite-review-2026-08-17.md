---
slug: b061-analytical-suite
item: B-061
repo: theodb-bench
date: 2026-08-17
base: 9be30b1
head: 4055420
verdict: READY_TO_MERGE_WITH_FOLLOWUPS
measured_on: droplet theo-b059-bench · 138.197.22.192 · s-8vcpu-16gb · nyc3
---

# Review — B-061 · o portão que recusa quatro mentiras, e o número que ele me obrigou a retratar

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte | **700 passed; 2 skipped; 0 failed** (era 673) |
| 2 | `mypy --strict` | **limpo**, 37 arquivos — e **pegou uma violação de LSP** antes de ela subir (§ R-6) |
| 3 | `ruff check` / `ruff format` | **All checks passed** (venv do projeto, 0.16.2) |
| 4 | Prova por reprovação | 4 conjuntos — cada estado do portão tem um teste que reprova sem o conserto |
| 5 | Segredos commitados | **0** |
| 6 | Idioma do repo | inglês em código, docstrings, CHANGELOG e commit |
| 7 | `CHANGELOG.md` atualizado | sim |
| 8 | Schemas versionados | **nenhum bump** — nada de novo entra em artefato |

## Cross-validation

| # | Afirmação | Verificação | Resultado |
|---|---|---|---|
| C1 | o colunar do TheoDB é **armazenamento** | `pg_am` → `theodb_columnar` com `amtype='t'`; `CREATE TABLE … USING`; 50 000 linhas; agregação correta | confirmado |
| C2 | o do Omni é **cache** com quatro estados | os quatro reproduzidos contra o servidor real | 4 de 4 |
| C3 | `g_columnar_columns` reporta **registro, não residência** | 4 colunas reportadas com `Memory Used = 0 MB` e plano `Seq Scan` | confirmado — o achado central |
| C4 | a causa do estado 3 é memória compartilhada | `could not resize shared memory segment … No space left on device` + `HINT: increase the shared memory for the container`; com `--shm-size=4g` → `Memory Used = 42 MB` | confirmado |
| C5 | residência não basta | store carregado a 50 000 linhas, planner escolheu `Seq Scan` | confirmado |
| C6 | `theodb.enable_columnar_agg` decide 13× | mesma tabela de 1M, mesma query: `off` → `Seq Scan` 1407 ms; `on` → `Custom Scan (theodb_columnar_agg)` 108 ms | confirmado |
| C7 | a cobertura do pushdown varia por **shape** | `sum(amount)` empurra; `GROUP BY` cai para `Seq Scan → Sort` externo com 25 456 kB em disco | confirmado |
| C8 | o portão prova por reprovação | 4 testes reprovam com o conserto removido | 4 de 4 |

## Achados

### R-1 — CRÍTICO · A prova de residência que o SOTA recomenda não prova residência

A avaliação independente de AlloyDB (2026-08-15) recomenda `g_columnar_columns` como a prova de que as colunas
estão residentes — o avaliador perdeu uma corrida inteira por não a ter checado. Medido aqui, contra o mesmo
produto:

```
google_columnar_engine.enabled = on
google_columnar_engine_add('big_probe')  →  ok
select count(*) from g_columnar_columns  →  4
select value from g_columnar_engine_summary where name='Memory Used (MB)'  →  0
EXPLAIN …                                →  Parallel Seq Scan
```

**Quatro colunas registradas, zero bytes carregados, plano em varredura sequencial.** A view reporta
**registro**. Um portão construído sobre ela passaria e a corrida publicaria heap sob o nome do colunar do
AlloyDB — a mesma falha que o avaliador documentou, um nível mais fundo.

É a forma exata do `current_setting` vs `pg_settings` do [[B-060]]: **o instrumento óbvio reporta o pedido, não
o efeito.** Terceira vez que essa forma aparece neste ecossistema, e a primeira em que a encontrámos antes de
alguém a publicar.

A causa é ambiental e silenciosa: `google_columnar_engine_refresh` falha com
`could not resize shared memory segment … No space left on device`, porque o `/dev/shm` default do Docker é
64 MB. Sem `--shm-size`, o store **nunca** carrega — e nada no caminho normal avisa.

### R-2 — CRÍTICO · Um número meu, produzido neste mesmo ciclo, retratado por medição

Primeira medição do crossover, com o default do motor:

| linhas | `sum_amount` | `filtered_sum` | `group_by_category` |
|---|---|---|---|
| 10 000 | 0,22× | 0,16× | 0,43× |
| 100 000 | 0,15× | 0,16× | 0,21× |
| 1 000 000 | **0,05×** | 0,06× | 0,07× |

Reportei, a partir disto, que *"o colunar do TheoDB é 14–20× mais lento que o próprio heap"* e que *"não há
crossover no intervalo testado"*. **As duas afirmações estavam erradas**, e o que as derrubou foi ler o plano
em vez de confiar no tempo:

```
theodb.enable_columnar_agg = off  (DEFAULT)  →  Seq Scan             1407 ms
theodb.enable_columnar_agg = on              →  Custom Scan
                                                (theodb_columnar_agg)  108 ms
```

**13×, decidido por um GUC que vem desligado**, com o catálogo reportando tabela colunar nos dois casos. Eu
tinha medido armazenamento colunar **sem** o pushdown — que é precisamente o caminho que a wiki do projeto já
registra como mais lento que heap (M184: *"o ganho do colunar vive no pushdown, não no seqscan plano"*).

E é o **nosso** `scann.enable_ah_quantizer = off`: eu havia sinalizado essa armadilha no concorrente algumas
horas antes, no [[B-059]], e caí na nossa versão dela.

Segunda medição, com o pushdown ligado e **verificado em vigor**:

| linhas | `sum_amount` | `filtered_sum` | `group_by_category` |
|---|---|---|---|
| 10 000 | 0,80× | 0,62× | 0,28× |
| 100 000 | **1,75×** | **1,37×** | 0,22× |
| 1 000 000 | **1,41×** | 0,84× | **0,07×** |

**O crossover existe e está medido:** para `sum_amount` o colunar passa o heap entre 10 000 e 100 000 linhas
(0,80× → 1,75×). Para `filtered_sum`, vence a 100 000 (1,37×) e volta a perder a 1M (0,84×).

Duas ressalvas que a medição obriga:

1. **O heap corre paralelo e o colunar serial.** `Workers Planned: 2` no heap contra execução serial no
   `Custom Scan`. Parte da razão é paralelismo, não formato de armazenamento, e chamar isto de "o crossover do
   colunar" sem dizer isso seria medir uma coisa e rotular outra.
2. **São 5 repetições e mediana, sem teste de significância.** `papers/rigorous-perf-eval-georges-2007.pdf` é
   o que a regra 5 exige antes de qualquer claim, e isto não é claim: é o critério de DoD "a partir de quantas
   linhas ele vence o heap", respondido, e nada mais.

### R-3 — ALTO · O `GROUP BY` não é coberto pelo pushdown, e é 14× mais lento que heap

O 0,07× a 1M não é ruído. O plano:

```
GroupAggregate
  ->  Sort   Sort Method: external merge  Disk: 25456kB
        ->  Seq Scan on x_columnar_1000000  (actual rows=1000000)
```

Sem pushdown, ordenação externa derramando **25 MB** em disco, contra `HashAggregate` paralelo no heap. É um
buraco real de cobertura do nosso colunar, medido, e vale mais que o crossover: um usuário que escreva
`GROUP BY` numa tabela colunar recebe 14× de piora sem aviso.

### R-4 — ALTO · O portão que eu escrevi não pegaria o R-3, e isso é a lição do `assert_index_used` outra vez

A primeira versão sondava **uma** query (`filtered_sum`) e generalizava para a tabela. Como `filtered_sum`
empurra e `group_by_category` não, o portão diria "pushdown em vigor" e a corrida mediria o fallback.

A prova de plano passou a ser **por query**. É o mesmo defeito do [[B-063]] noutro eixo — provar o caminho uma
vez e assumir que vale para tudo que corre depois.

### R-5 — MÉDIO · Dois testes anteriores ficaram obsoletos por uma razão legítima

`test_theodb_declares_only_the_vector_surface_it_can_exercise` e o equivalente do Omni asseriam que `columnar`
**não** era declarado. Estavam certos quando escritos e deixaram de estar quando a superfície passou a existir.
Atualizados mantendo a intenção: `columnar` entra porque é exercitado e provado; `hybrid`, `lexical`, `parquet`,
`graph` e `vectorizer` continuam fora.

A atualização expôs um buraco que registrei com teste: `columnar` e `parquet` compartilham
`load_analytical`/`execute_analytical` no `_CAPABILITY_METHODS`, então a guarda estrutural existente não os
distingue — um adapter poderia declarar `parquet` sem ter o caminho. Agora um teste parametrizado exige que a
capability declarada tenha entrada em `ANALYTICAL_PATHS`.

### R-6 — MÉDIO · O `mypy --strict` pegou uma violação de LSP antes de ela subir

Ao tornar a prova de plano por-query, mudei a assinatura de `assert_analytical_path` na base e esqueci o
override do Omni. O mypy:

```
error: Signature of "assert_analytical_path" incompatible with supertype "PostgresAdapter"  [override]
```

Subtipo que não é substituível pelo supertipo é exatamente o 13.3 dos princípios, e aqui a ferramenta o
enunciou melhor que eu teria.

### R-7 — MÉDIO · Um `S608` real que eu quase silenciei

O `EXPLAIN` do portão do Omni interpolava `table.name` **sem** `_identifier`. O `ruff` marcou; o arquivo vizinho
tem um per-file-ignore documentado para `S608`, e a tentação era estendê-lo. Consertei passando pelo
`_identifier`, que é a fronteira de confiança que o próprio plano do [[B-059]] declarou. Dois `RUF100` companheiros
eram `noqa` inúteis (a regra já está ignorada por arquivo) e saíram.

### R-8 — INFORMATIVO · O estado 1 do Omni exige restart, e isso é fato de desenho para o [[B-058]]

`google_columnar_engine.enabled` tem `context = postmaster`: `SET` de sessão é **erro**
(`cannot be changed without restarting the server`), e ligar exige `ALTER SYSTEM` + restart. O B-058 quer medir
"Omni off vs on" — não é um flag que o arnês possa alternar dentro de uma corrida. São duas corridas contra dois
servidores, e o artefato tem de dizer qual foi qual.

## O que este ciclo NÃO fez, e está registrado

| DoD do B-061 | Estado |
|---|---|
| suíte analítica registrada, **shape TPC-H** | shape TPC-H → [[B-065]] (exige contrato multi-tabela; a Q5 junta seis). Estar **registrada** → [[B-067]] (o orquestrador de 11 fases só despacha workload vetorial) |
| **portão de residência antes de qualquer número** | **FEITO** — quatro estados, cada um com mensagem própria |
| crossover do nosso colunar medido | **FEITO** — por script direto contra os adapters, não por bundle validado (o bundle depende do B-067) |
| contenção escrita×scan nos dois regimes | → [[B-066]] (não existe executor concorrente no repositório) |

Dois dos quatro bullets ficaram fora, e ambos por razão estrutural medida, não por escolha de conveniência.
Registrei três itens ([[B-065]], [[B-066]], [[B-067]]) em vez de os deixar como texto num relatório.

Outras ressalvas:

- **Review do próprio implementador**, sem agente independente.
- **O portão do Omni não foi exercitado ponta a ponta contra o servidor real** — os quatro estados foram
  medidos à mão e reproduzidos em duplos; não há teste de integração que rode o portão contra o contêiner.
- **`parquet` continua sem caminho** em nenhum adapter PostgreSQL.
- O crossover tem 5 repetições e mediana. Não é claim de performance e não vai para `wiki/benchmarks/`.

## Veredito

**`READY_TO_MERGE_WITH_FOLLOWUPS`.**

8 de 8 afirmações verificadas por execução; 700 testes; mypy strict e ruff limpos; quatro consertos provados
por reprovação. Dois dos quatro bullets do DoD ficaram fora por razão estrutural, e cada um é agora um item
registrado com DoD próprio — que é o que o `cycle-review` exige deste veredito: o débito é real, nomeado e tem
dono.

O resultado que mais importa não é o código: é que **um número produzido neste ciclo foi retratado por medição
dentro do mesmo ciclo**, e o mecanismo que o retratou (ler o plano em vez de confiar no tempo) virou portão.
