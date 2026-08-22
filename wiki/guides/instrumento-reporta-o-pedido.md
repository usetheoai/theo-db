---
type: Guide
title: O instrumento óbvio reporta o pedido, não o efeito
description: Quatro instrumentos deste ecossistema respondem o que foi pedido em vez do que está em vigor, e nenhum deles falha ao fazê-lo. Como verificar cada um, e por que a verificação pertence ao arnês e não ao motor.
resource: .claude/knowledge-base/reviews/b061-analytical-suite-review-2026-08-17.md
tags: [guia, metodo, medicao, portao, honest-negative, arnes]
generated: { by: claude-code/opus-5, at: 2026-08-17 }
sources:
  - id: b060
    resource: .claude/knowledge-base/reviews/b060-knob-gate-review-2026-08-16.md
    title: B-060 — o portão do knob de busca
    last_modified: 2026-08-16
  - id: b061
    resource: .claude/knowledge-base/reviews/b061-analytical-suite-review-2026-08-17.md
    title: B-061 — portão de residência colunar
    last_modified: 2026-08-17
  - id: b057
    resource: .claude/knowledge-base/discoveries/opportunities/b057-scann-am-headtohead-opportunity.md
    title: B-057 — head-to-head contra o scann AM
    last_modified: 2026-08-17
---

Quatro vezes, em dois dias, um instrumento respondeu **o que foi pedido** em vez de **o que está em
vigor**. Nenhum deles errou; nenhum deles falhou. Todos os quatro produziram um número que parecia
certo, e três produziram um bundle `VALID` com fronteira plausível.

O padrão vale a pena nomear porque ele reaparece em eixos que nada têm em comum — GUC de sessão,
cache colunar, plano de execução, configuração de build — e porque **a forma da falha é sempre a
mesma**: uma curva plana, um número redondo, e nenhum erro.

# A tabela

| Eixo | Instrumento que engana | Instrumento correto |
|---|---|---|
| GUC de busca | `current_setting` — ecoa o valor escrito | `pg_settings` — lista só GUC **registrado** |
| Residência colunar | `g_columnar_columns` — reporta **registro** | `g_columnar_engine_summary` → `Memory Used > 0` |
| Caminho de execução | residência provada | **o plano**, e **por query** |
| Configuração do motor | o default | o flag ligado **e verificado em vigor** |

# Por que cada um engana

**`current_setting`.** No PostgreSQL, `SET namespace.opcao = valor` para namespace não registrado
sucede: é tratado como placeholder. `current_setting` devolve o valor escrito. Medido: `SET
nao.existe = 999` → `SET`; `current_setting` → `999`; `pg_settings` → **0 linhas**. Um portão sobre
`current_setting` confirmaria 200 enquanto o motor busca no default.

O corolário que importa: a extensão só registra seus GUCs **depois** do `LOAD`. O TheoDB tem 0
entradas `theodb%` em sessão nova e 38 depois de `LOAD 'theodb_rs'`; o
[AlloyDB](/technologies/alloydb.md) tem 1 entrada `scann%` e 111 depois de `LOAD 'alloydb_scann'`.

**`g_columnar_columns`.** Reporta as colunas **registradas** com o engine colunar, não as carregadas.
Medido com o engine ligado e a tabela registrada: **4 colunas** enquanto `Memory Used = 0 MB` e o
plano é `Seq Scan`. A causa é ambiental e silenciosa — o refresh falha com `could not resize shared
memory segment` porque o `/dev/shm` default do Docker é 64 MB.

**Residência.** Necessária e não suficiente, e a cobertura do pushdown depende do **shape** da query.
Mesma tabela colunar de 1M com pushdown ligado: `sum(amount)` planeja como
`Custom Scan (theodb_columnar_agg)`; `GROUP BY category` cai para `Seq Scan → Sort` externo com
25 456 kB em disco e roda **14× mais lento que heap**. Um portão que sonde uma query e generalize
chama a segunda de "pushdown em vigor".

**O default.** Dois motores, mesma forma. `theodb.enable_columnar_agg` vem `off` e vale **13×** na
mesma tabela e mesma query (1407 ms → 108 ms). `scann.enable_ah_quantizer` vem `off` e é exigido **no
build**, então o default constrói `SQ8` sob o rótulo `AH`. `scann.pre_reordering_num_neighbors` vem
`-1` e limita o recall a **0,6568** onde `100` dá **0,9964**.

# Como verificar

1. **Todo knob pedido é lido de volta de `pg_settings`**, e o `source` tem de ter saído de `default`.
   Ausente da view = biblioteca não carregada na sessão = o `SET` foi placeholder.
2. **Todo knob pedido que o adapter não sabe mapear é recusado**, não ignorado. Um mapeamento vazio
   não tem o que verificar e passa por vacuidade — foi assim que três pontos rotulados
   `ef_search=16/64/256` saíram com recall **0,7820 nos três**.
3. **Residência é provada pelo que ocupa memória**, não pelo que está catalogado.
4. **O plano é conferido por query**, porque a cobertura varia com o shape.
5. **Flags de qualidade são declarados e verificados**, e o artefato os registra. Um flag de *build*
   aplicado depois do build não muda o índice já escrito.

# Onde a verificação pertence

**No arnês, não no motor.** Os motores estão corretos: é assim que o PostgreSQL registra GUC de
extensão, e é assim que um cache populado por política se comporta. O que pode medir a coisa errada é
a corrida.

# A assimetria que torna isto sério

Medir-nos num default aleijado custa um número. **Medir o concorrente num default aleijado produz
alegação falsa sobre o produto de outra pessoa — e que nos favorece.** O resultado que estava na mão
era *"o scann do AlloyDB teto em 0,66 de recall enquanto o nosso chega a 0,9956"*.

É a mesma classe que o `bm25_search` devolvendo zero em silêncio e que o `SET hnsw.ef_search`
aceito-e-ignorado: **superfície que responde onde deveria recusar**. A diferença é que aqui ela
aponta para fora.

# A quinta ocorrência é a imagem espelhada, e passou dez meses invisível

*Acrescentado em 2026-08-22 ([[b102-configuracao-nao-declarada]]). Nada acima foi reescrito.*

As quatro acima têm a mesma forma: **o instrumento respondeu o pedido em vez do efeito**. A quinta
inverte a forma e produz o mesmo dano — o instrumento verificou o efeito **e não reportou nada**.

O arnês liga `theodb.enable_columnar_agg` antes de medir o caminho colunar. Isso está **certo**, é
deliberado, e a razão está escrita no próprio código: medir o colunar sem o pushdown é um caminho que
já se sabe perder para o heap, e publicá-lo como "nosso colunar" seria o mesmo erro que medir o ScaNN
com o quantizador AH desligado. O adapter inclusive **verificava** a GUC, com a mesma função que
verifica os botões de busca.

**E descartava a resposta.** O caminho vetorial atribuía o valor verificado a
`_effective_search_parameters` e o levava até `points[].parameters`; o caminho analítico chamava a
mesma função e ignorava o retorno. Resultado: o `system.json` de uma corrida publicada traz 14 GUCs de
servidor — `shared_buffers`, `work_mem`, `fsync` — e **nenhuma de sessão**. Dos 53 conceitos que
publicam número colunar em `wiki/benchmarks/`, **3** mencionavam a GUC.

## O número que separa as duas configurações

Medido em 2026-08-22, mesma tabela, mesmo servidor, mesma sessão:

| Configuração | `count(*)` a 2M linhas |
|---|---|
| `theodb.enable_columnar_agg = off` — **o default do produto** | **911 ms** |
| `theodb.enable_columnar_agg = on` — o que o arnês mede | **74 ms** |

**12×.** Consistente com os 1407 ms → 108 ms já medidos a 1M.

**Portanto: o default do TheoDB não é a configuração em que os números colunares publicados foram
obtidos.** Quem instala o produto e roda uma agregação recebe o caminho de 911 ms até ligar a GUC.
Isto fica dito aqui porque não estava dito em lugar nenhum.

## Por que isto é a mesma classe, e não um item de higiene

A regra que as quatro primeiras produziram foi *"o arnês declara e verifica"*. A quinta mostra que a
regra estava pela metade: **declarar e verificar sem registrar não deixa rastro nenhum.** Um leitor do
bundle não distingue "a GUC foi ligada e confirmada" de "a GUC nunca foi tocada" — os dois artefatos
são byte a byte iguais no que ele consegue ler.

E o custo já apareceu, antes de alguém procurar por ele: o [[B-101]] foi registrado sobre uma premissa
errada e morreu, porque duas medições minhas rodaram em configurações diferentes e **nenhum artefato
dizia qual era qual**. A ausência não é neutra — ela já produziu uma hipótese falsa e o trabalho de
matá-la.

## A sexta está DENTRO do detector, e custou um dia de campanha

*Acrescentado em 2026-08-22, algumas horas depois da quinta ([[B-043]]). Nada acima foi reescrito.*

O arnês tem um `doctor` cujo trabalho é dizer o que o host pode fazer. Ele respondia
`perf_events: False` no droplet — e a nota do [[B-043]] registrou isso como **o bloqueio da campanha de
perfilamento** que responderia por que o QPS lexical satura.

**Medido no próprio droplet, como root, no estado exato que ele chamava de indisponível
(`perf_event_paranoid = 4`):**

```
perf stat -e task-clock -- sleep 0.05   →  1.19 msec task-clock
perf record -e cpu-clock                →  perfil com símbolos de userspace E de kernel
```

A regra era `perf_event_paranoid <= 2`. Esse número é a **política**, não o efeito — e a política
restringe usuário **sem privilégio**. Root a contorna, e o arnês roda como root no host de medição.

**Duas coisas fazem desta ocorrência a mais séria da lista.** A primeira: ela vive *dentro do
instrumento que existe para detectar esta classe*. O `doctor` é a peça que responde "o que dá para
medir aqui", e ele respondia lendo um valor de configuração em vez de tentar.

A segunda: **as anteriores produziram números errados; esta impediu uma medição de acontecer.** Um
número errado é encontrável — alguém o refaz e discorda. Uma campanha que não foi feita porque o
instrumento disse que não dava **não deixa rastro nenhum**, e a nota do backlog registrou o bloqueio
como fato de plataforma por um dia inteiro.

**Um terceiro erro, menor e junto:** a capacidade era *uma*, e são *duas*. Contador de **hardware**
(cycles, cache-misses) e amostragem por **software** (cpu-clock) falham por razões diferentes — uma VM
tipicamente não expõe PMU e amostra por software sem problema. A campanha só precisava da segunda, e
recebeu a resposta da primeira.

O conserto foi trocar a dedução por um `perf stat` de verdade, e separar as duas capacidades. Custo:
um subprocesso de poucos milissegundos por captura.

# A regra, corrigida

> **Não basta declarar e verificar. O que foi verificado tem de sair no artefato.**
> Uma configuração aplicada e confirmada que não chega ao bundle descreve, para quem lê, a
> configuração que **não** foi usada.

E, da sexta:

> **Uma capacidade é medida tentando, não lendo a política que a governa.**
> Um instrumento que responde "não dá" sem ter tentado não erra um número — ele impede a medição de
> existir, e isso não deixa rastro que alguém possa refazer e contestar.
