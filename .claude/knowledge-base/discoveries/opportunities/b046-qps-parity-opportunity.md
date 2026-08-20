---
item: B-046
mode: evolve
date: 2026-08-13
verdict: pending
---

# B-046 — a decomposição é medível, e quase toda ela sem droplet

## Corner 1 — Evidence

### O que o item pede, e por que estava travado

O DoD manda **decompor** o déficit de 16,3% de QPS: quanto vem de qualidade de grafo (recall menor no mesmo
`ef`) e quanto de eficiência de varredura (custo por candidato). Até o [[B-036]] isso era impossível — `m` e
`ef_construction` eram constantes de compilação, então não havia como variar a qualidade do grafo mantendo a
varredura fixa.

O B-036 destravou o motor. **O arnês continua travado**, e isso é o primeiro achado.

### Achado 1 — o cliente do arnês recusa o experimento, medido

Executado em 2026-08-13 contra o fork `usetheoai/VectorDBBench@theodb` (`4a15939`), venv limpa:

```python
TheoDBHNSWConfig(metric_type=MetricType.COSINE, m=32, ef_construction=200)
```
```
UnsupportedBuildParameterError: TheoDB cannot honour m=32: the build is fixed at m=16
(theodb_rs/src/am/build.rs:22-23), so running with m=32 would report a parameter that was
never applied. Use m=16 or omit it. Tracking item: B-036
```

O guard está em `clients/theodb/config.py:191-213`, e `index_param()` (`:227-236`) devolve `"options": {}`
com um comentário que diz textualmente que o AM "rejeita `m` / `ef_construction` como reloptions".

**As duas afirmações eram verdadeiras quando foram escritas e as duas são falsas desde `cecd388`.** A defesa
construída no [[B-035]] — corretamente, porque relatar um parâmetro não aplicado é medição errada com cara de
certa — virou o obstáculo do item que ela ajudou a justificar. É a segunda vez neste ciclo que uma defesa de
um item anterior encontra o item seguinte; no [[B-045]] ela **salvou** a corrida, aqui ela a **bloqueia**.

A remoção não pode ser cega: contra um servidor anterior ao b036, emitir `WITH (m=32)` faz o `CREATE INDEX`
falhar **depois da carga do dataset**, que é o custo caro. O guard precisa deixar de consultar uma constante e
passar a consultar **o servidor**.

### Achado 2 — o instrumento da decomposição existe, e é o MESMO nos dois motores

Medido em 2026-08-13, `theodb:b036` e `pgvector/pgvector:pg18` lado a lado (mesma versão de PG, mesmo corpus
sintético de 2.000 × 8d, `m=16, ef_construction=64` dos dois lados, `ef_search=64`):

| Motor | instrumento | por consulta |
|---|---|---|
| TheoDB | `EXPLAIN (ANALYZE, BUFFERS)` | `shared hit=139` |
| pgvector | `EXPLAIN (ANALYZE, BUFFERS)` | `shared hit=182` |
| TheoDB | `theodb.explain_scan` | 133 páginas, **52 candidatos** |

Duas coisas decorrem disso, e a segunda muda o custo do item:

1. **Páginas tocadas por consulta é um sinal comparável entre os dois** — os dois são PostgreSQL, e
   `shared hit` conta a mesma unidade. É o denominador que a DoD chama de "custo por candidato" no único
   formato que os dois motores sabem produzir: o pgvector **não tem contador de candidatos**, e inventar um
   exigiria forkar o pgvector, o que é escopo de outro projeto.
2. **Do nosso lado há o contador real** (`candidates_seen`: 39 / 52 / 81 a `ef` 32 / 64 / 128). Ele permite
   dizer *páginas por candidato* para o TheoDB — e comparar essa razão com o `shared hit / ef` do pgvector,
   declarando que o denominador do segundo é uma aproximação, não uma medição.

**Os números acima não são resultado do item.** São corpus de brinquedo (2.000 × 8d): servem para provar que
o instrumento existe e responde, não para dizer quem toca menos página no caso real.

### Achado 3 — a maior parte da decomposição NÃO precisa de droplet

Isto é o que reduz o tamanho do item. Das três grandezas em jogo:

| Grandeza | Depende da máquina? | Onde medir |
|---|---|---|
| **recall@k** a `m`/`ef_construction`/`ef_search` dados | **não** — é função do grafo e da consulta | local |
| **páginas tocadas** por consulta | **não** — é função do layout e do caminho percorrido | local |
| **QPS** | **sim** — é tempo de parede | droplet de referência |

O experimento que **decide** onde está o custo é o primeiro: varrer `ef_construction` e `m` e ver se o recall
do TheoDB a `ef_search=64` sobe dos 0,9600 medidos para os 0,9835 do pgvector. Se subir, o déficit é qualidade
de grafo. Se não subir em nenhum ponto da varredura, é a varredura — e aí o segundo (páginas por consulta a
recall casado) diz quanto.

Só o número final que entra no artefato precisa da máquina de referência, e só depois de a decomposição já ter
apontado onde mexer. **Medir a decomposição num droplet seria pagar hora de nuvem por um número que não
depende dela.**

### O que já se sabe e não precisa ser remedido

Do [b035](../../../wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md), droplet `g-16vcpu-64gb`,
`Performance1536D50K`, 1536d COSINE:

| | TheoDB | pgvector |
|---|---|---|
| recall @ `ef=64` | 0,9600 | **0,9835** |
| recall casado | 0,9829 (`ef=128`) | 0,9835 (`ef=64`) |
| QPS a recall casado | 3.086,1 | **3.590,6** (+16,3%) |

Reprodutibilidade entre duas corridas independentes em `ef=64`: 1,3% de QPS, 0,06% de recall.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| fork do VectorDBBench | `clients/theodb/config.py` — o guard passa a consultar o servidor; `index_param()` passa a emitir o `WITH`. Fora deste repositório, sem gate de CI aqui |
| `benchmarks/vectordbbench/` | +1 runner para a varredura; o `docker-compose.yml` já foi repontado para `theodb:b036` |
| motor (`theodb_rs`) | **nenhuma mudança neste item** — B-046 mede; o que a medição mandar mudar vira trabalho seguinte, com seu próprio ciclo |
| `wiki/benchmarks/b035-*.md` | **atualizado, não duplicado** (exigência do DoD) |
| [[B-042]] | se a decomposição mostrar causa única, um dos dois fecha como duplicata — e isso será dito |
| Consumidores externos | nenhum: o cliente do arnês não é dependência de produto |

## Corner 4 — Verification

1. O cliente aceita `m=32, ef_construction=200` contra um servidor b036+ **e continua recusando** contra um
   servidor que não os suporta — as duas metades testadas, porque só a primeira reintroduz o defeito que o
   guard existia para impedir.
2. O `CREATE INDEX` emitido de fato carrega o `WITH` — verificado em `pg_class.reloptions`, não no código.
3. A varredura produz uma tabela recall × (`m`, `ef_construction`) a `ef_search` fixo, no caso real de 1536d.
4. A decomposição declara um número para cada metade, e diz qual instrumento produziu cada um.
5. O ganho, se houver, passa por teste pareado ([[B-045]]) antes de virar afirmação.
6. `b035-theodb-vs-pgvector-pg18.md` é atualizado por acréscimo.

## Reclassificação

`suggested_mode: evolve` mantido. O que a descoberta mudou é onde o trabalho está: o item parecia ser
"otimizar o scan" e é, antes disso, **destravar o arnês e rodar um experimento que quase todo cabe no host**.
