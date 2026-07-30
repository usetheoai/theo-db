---
type: Invariant
title: SET de uma GUC inexistente no namespace de uma extensão SUCEDE — como placeholder silencioso
description: O comando devolve SET, o valor é lembrado e nada acontece; pg_settings é o único discriminador entre GUC real e placeholder.
resource: theodb_rs/src/am/columnar_agg.rs
tags: [postgres, guc, falso-verde, medicao]
timestamp: 2026-07-30T00:00:00Z
---

# `SET` de uma GUC inexistente no namespace de uma extensão **sucede**

## O invariante, medido

```sql
SET theodb.enable_columnar_agg_stream = off;
SET                                          -- <- sucesso. A GUC NÃO EXISTE.
```

Medido em 2026-07-30 contra PG18 com `theodb_rs` em `shared_preload_libraries`. O PostgreSQL aceita um parâmetro
desconhecido **dentro do prefixo de uma extensão** como *placeholder*: o comando retorna `SET`, o valor é
lembrado pela sessão, e **nada acontece**.

`pg_settings` é o discriminador — e o único:

```sql
SELECT name FROM pg_settings WHERE name LIKE 'theodb.%stream%';
 theodb.enable_columnar_topk_stream          -- a real
                                             -- a inexistente simplesmente não aparece
```

## Por que a direção do erro é a cara

Eu supunha o contrário: que um `SET` de GUC inexistente **falharia** (42704) e mataria a conexão. Se fosse
assim, o defeito seria barulhento e auto-corrigível na primeira execução.

O comportamento real é o pior dos dois: um erro de digitação no nome da GUC **passa verde**, o caminho de código
que ela governa fica **desligado**, e a medição conclui *"a mudança não teve efeito"*. Num milestone cujo
entregável É esse caminho, isso é um falso negativo sobre o próprio produto, produzido por um comando bem-sucedido.

O mesmo vale para uma GUC que ainda **não foi implementada**: escrever o `SET` dela num runner antes de o código
existir não quebra nada — e é exatamente por isso que ninguém percebe.

## O guard

Depois de aplicar as GUCs, **leia-as de volta de `pg_settings`** e registre o resultado no artefato:

```python
cur.execute("SELECT setting FROM pg_settings WHERE name = %s", (name,))
row = cur.fetchone()
effective[name] = row[0] if row else "PLACEHOLDER — o servidor não conhece esta GUC"
```

Um artefato que declara as GUCs pedidas mas não as **efetivas** não prova sob que configuração o número foi
medido.

## Relacionados

- [failure-mode/gate-desligado-em-silencio](../failure-modes/gate-desligado-em-silencio.md)
- [failure-mode/assert-que-e-uma-identidade](../failure-modes/assert-que-e-uma-identidade.md)
- [invariant/worker-nao-ve-set-de-sessao](worker-nao-ve-set-de-sessao.md) — o outro jeito de um SET não valer
- [technique/proveniencia-em-todo-artefato](../techniques/proveniencia-em-todo-artefato.md)
