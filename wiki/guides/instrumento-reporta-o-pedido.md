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
