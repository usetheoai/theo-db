---
type: Failure Mode
title: Medição degenerada aceita como dado
description: Um resultado que só poderia sair de um setup quebrado (zero linhas, tempo impossível, zero divergências num oráculo que nunca rodou) é registrado como se fosse observação.
tags: [medicao, falso-verde, oraculo]
timestamp: 2026-07-30T00:00:00Z
---

# Medição degenerada aceita como dado

## Assinatura

O número é *bom demais* ou *rápido demais* para o tamanho do problema. `(0 rows)` em 10 ms sobre 100 milhões de
linhas. `diverged=0` num oráculo cuja tabela de comparação está vazia. Cobertura 100% num detector que não rodou.

## Casos pagos

| Caso | O que foi aceito | A verdade |
|---|---|---|
| M169 | `SELECT … FROM hits_heap GROUP BY … LIMIT 10` → `(0 rows)` em **10,354 ms** | `hits_heap` é `UNLOGGED` e tinha sido truncado por crash recovery — comparei contra tabela vazia |
| M168 | oráculo de cancelamento "passando" | passava **também com a safepoint removida** — não era diferencial |
| M162 | "100M carregados" | `_ensure_sample` reusou um cache de 1M sem conferir a contagem |

## Custo

No M169, uma rodada inteira de A/B (30 min) produzida contra tabela vazia. No M168, dois gates que pareciam
proteger e não protegiam.

## Como evitar

## A variante SILENCIOSA: o oráculo roda o caminho errado (M169, 2026-07-31)

Aqui não há número absurdo — e é o que a torna pior. O verificador de byte-identidade das 4 consultas
consertadas abria a conexão definindo só `statement_timeout`. Faltava `SET theodb.enable_columnar_agg = on`, e
essa GUC é **default OFF** (`columnar_agg.rs:24`). O oráculo compararia colunar vs heap **sem o pushdown
agregado** — ou seja, provaria byte-identidade de um caminho que o milestone não toca, e teria estampado
"idêntico" com toda a razão e nenhuma relevância.

O sintoma que denunciou não foi o resultado: foi o **tempo**. A q20 levava 4m45s no oráculo contra 59,5 s na
corrida que se queria provar. Cinco vezes mais lenta é grande demais para cache frio. A tentação era explicar a
diferença ("restart, page cache") e seguir; a diferença ERA o defeito.

Lição operacional: **um oráculo tem de reproduzir a SESSÃO da corrida que ele prova**, não só a consulta. GUC,
`work_mem`, ordem — tudo que muda o plano. E a defesa mecânica é barata: o gate agora roda `EXPLAIN` antes de
comparar e recusa a comparação quando o pushdown não está no plano, deixando `identical = None`. **"Não provei"
tem de ser um estado distinto de "está certo"** — colapsar os dois é como esta classe entra.

Precedente da mesma forma: no M161, uma chave de texto só roteava `HASHED`, então um A/B sem `ORDER BY` era
falso-verde até forçar `enable_sort=off` e confirmar o plano no `EXPLAIN`.

Todo oráculo precisa de **controle positivo**: um caso deliberadamente divergente que ele **tem** de reprovar. Se
o controle positivo passa, o oráculo está quebrado e a corrida aborta — ver
[technique/controle-positivo](../techniques/controle-positivo.md).

E todo gate precisa de **não-vacuidade explícita**: "10 linhas **ou** um erro; qualquer outra coisa é `[INVALIDO]`".
Sem isso, silêncio e sucesso são indistinguíveis.

## Relacionados

- [technique/gate-de-nao-vacuidade](../techniques/gate-de-nao-vacuidade.md)
- [invariant/unlogged-truncado-por-recovery](../invariants/unlogged-truncado-por-recovery.md)
