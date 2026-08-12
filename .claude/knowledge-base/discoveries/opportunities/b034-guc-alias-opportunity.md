---
item: B-034
mode: bug
date: 2026-08-12
verdict: pending
---

# B-034 — Os GUCs de ajuste do pgvector são aceitos em silêncio e não fazem nada

## Corner 1 — Evidence

### O comportamento, medido

Contra a imagem construída neste ciclo:

```
SET hnsw.ef_search = 200;                          -- SET (sucesso)
SELECT current_setting('hnsw.ef_search');          -- 200
SELECT count(*) FROM pg_settings WHERE name LIKE 'hnsw%';  -- 0
```

O PostgreSQL aceita GUC de prefixo não registrado como **placeholder**: guarda o valor, não expõe em `pg_settings`, e nenhum código o lê. O TheoDB lê `theodb_hnsw.ef_search`.

### A superfície afetada

| GUC que o cliente pgvector emite | GUC que o TheoDB lê | Ponto de leitura |
|---|---|---|
| `hnsw.ef_search` | `theodb_hnsw.ef_search` (`guc.rs:361`) | `guc.rs:511` |
| `ivfflat.probes` | `theodb_ivfflat.probes` (`guc.rs:351`) | `guc.rs:506` |

Cada ponto de leitura é **uma linha**. Consumidores: `am/scan.rs:284,299` e `am/cost.rs:123`, mais `am/customscan.rs:444`.

### Por que isto é pior do que faltar

O shim já toma o nome `hnsw` para o access method — verificado hoje, `CREATE INDEX ... USING hnsw (e vector_l2_ops)` funciona. Então a app pgvector:

1. cria o índice com a sintaxe dela — **funciona**
2. ajusta o recall com `SET hnsw.ef_search` — **não acontece nada, sem erro**

Se o access method faltasse, a falha seria alta e o usuário saberia. Assim ele acredita ter ajustado e mediu outra coisa. **A forma da falha é uma curva recall×QPS plana**, indistinguível de "esse parâmetro não importa neste dataset".

### O que o pgrx oferece (medido, decide o desenho)

`pgrx-0.19.0/src/guc.rs` expõe `define_int_guc` (linha 302) e `define_int_guc_with_hooks` (456), esta com `check_hook` e `assign_hook`. `GucSetting` **não** expõe a origem do valor (`source`), então "foi setado explicitamente?" não é pergunta que a API responde diretamente.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `theodb_rs/src/am/guc.rs` | +2 registros de GUC, 2 pontos de leitura alterados |
| Comportamento existente | **nenhuma mudança** para quem usa `theodb_hnsw.*` / `theodb_ivfflat.*` |
| Sessões que hoje setam `hnsw.ef_search` | passam de inertes a **efetivas** — é a correção, e é mudança de comportamento observável |
| Sessões com placeholder fora de faixa | `SET hnsw.ef_search = 99999` hoje passa em silêncio; passará a **falhar** na conversão do placeholder |
| Índices existentes | intocados — GUC é de scan, não de build |

## Corner 4 — Verification

1. `SET hnsw.ef_search = N` produz o **mesmo efeito medido** que `SET theodb_hnsw.ef_search = N` — provado por recall diferente entre dois valores, não por o `SET` ter sido aceito.
2. Idem para `ivfflat.probes`.
3. A precedência entre os dois nomes é determinística e documentada.
4. Os GUCs passam a aparecer em `pg_settings` (hoje `count = 0`).
5. Nenhum teste existente que use `theodb_hnsw.ef_search` regride.

## Reclassificação

`suggested_mode: bug` mantido — é defeito de compatibilidade declarada (`ADR-0029 § D2` promete drop-in "sem mudança de código"), não evolução.
