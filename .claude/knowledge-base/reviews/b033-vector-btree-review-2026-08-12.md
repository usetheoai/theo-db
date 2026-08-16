---
slug: b033-vector-btree
items: [B-033]
date: 2026-08-12
base: 515afa2
head: 931242f
verdict: READY_TO_MERGE
---

# Review — igualdade e ordenação para o tipo `vector`

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Testes verdes na branch | **451 passed, 0 failed** |
| 2 | Segredos commitados | **0** — gitsafety passou em todos os commits |
| 3 | Commit direto em `main` | não — branch `workspace` |
| 4 | Trailer de coautoria | **0** |
| 5 | `CHANGELOG.md` atualizado | sim |

Nenhum gate duro disparou. `/code-quality`: `PASS_WITH_CAVEATS`, Rust auditado, **0 achados HARD**.

## Cross-validation — cada afirmação do Goal contra a realidade

| # | Afirmação | Verificação | Resultado |
|---|---|---|---|
| G1 | `vector_cmp` idêntico ao upstream | `pg_cmp_matches_upstream_semantics` | ok |
| G2 | Ordem total | `pg_cmp_is_a_total_order` | ok |
| G3 | 6 operadores no catálogo | 6 `CREATE OPERATOR` em `dtype.rs` + `surface_contains_public_api` | ok |
| G4 | Opclass btree DEFAULT | `DEFAULT FOR TYPE vector USING btree` declarado; índice criado sem nomeá-la no teste | ok |
| G5 | `WHERE e = …` | `pg_pgvector_query_patterns_work` | ok |
| G6 | `SELECT DISTINCT e` | idem | ok |
| G7 | `GROUP BY e` | idem | ok |
| G8 | `ORDER BY e` | idem | ok |
| G9 | `UNIQUE` constrói **e rejeita** | `pg_unique_index_rejects_duplicate` | ok |
| G10 | Caminho ANN não regride | `pg_ann_path_still_uses_the_ann_index` | ok |
| G11 | Superfície no contrato | `pg_surface_contains_public_api` com os 7 operadores + opclass | ok |

**11 de 11 verificadas.**

## Achados

### R-1 — MÉDIO · Duas tarefas do plano não haviam sido implementadas

Declarei IMPLEMENT verificado com 4 testes; o plano tinha **5 tarefas**. Faltavam:

- **T1.4 (G10)** — a regressão do caminho ANN. O risco que ela cobre não é erro: é uma consulta de similaridade passar a resolver por outro caminho e ficar lenta **sem sintoma**.
- **T1.5 (G11)** — a superfície nova no contrato. Sem ela, os 7 operadores poderiam sumir num refactor sem nenhum teste reclamar.

Encontrado ao percorrer a Coverage Matrix linha a linha, e é a **segunda vez consecutiva** que essa varredura acha algo que a fase anterior deu por concluído — no ciclo B-030/B-031 foi o `docker build` afirmado sem medição.

**O agravante:** o CHANGELOG já afirmava, em texto público, que *"a busca por similaridade não muda"*. Era garantia sem nada por trás. Só virou verificação quando o T1.4 foi implementado — e sem esta fase teria saído numa release apoiada em afirmação.

Corrigido em `931242f`.

### R-2 — BAIXO · Três correções no mesmo teste, todas por supor em vez de ler

`unique_index_rejects_duplicate` reprovou **duas vezes com o produto correto**:

1. `assert!(result.is_err())` — em pgrx um `ERROR` do PostgreSQL faz longjmp e aborta a transação; o `assert` nunca é avaliado. O idioma certo, `#[pg_test(error = …)]`, **já era usado sete vezes no mesmo arquivo**.
2. Passei só o prefixo da mensagem. Lendo `pgrx-tests-0.19.0/src/framework.rs:174` — `Some(received) == expected` — a comparação é por **igualdade exata**. Corrigido com a mensagem completa e o índice nomeado à mão, para que a string fique sob controle do teste em vez de depender da convenção de nomes do PostgreSQL.

Somando a terceira (adivinhar o formato do `pg_describe_object` para opclass, pega antes de commitar por medição contra as opclasses existentes), o padrão é um só: **supor o comportamento de uma ferramenta em vez de ler o que ela faz**.

Vale o contraste: a única suposição que peguei **antes** de escrever código foi a da semântica do `vector_cmp` — e só porque a Regra 8 obriga a buscar referência primária antes de decidir semântica. Onde a regra não me obrigou, errei três vezes.

### R-3 — INFORMATIVO · O cap do `/code-quality`

`symbol_fab_unverifiable_rust`: 149 símbolos em `SOFT_FLOOR`, **0 em HARD**. São crates locais do workspace e módulos internos que não existem em crates.io. Verificado que a rede alcança o registro, então não é falha de ambiente — é o detector não conseguindo confirmar o que, por construção, não está lá.

## O que este review NÃO cobriu

- **Não houve revisão por agentes independentes.** Mesmo agente que implementou. O R-1 é a evidência do custo — duas tarefas faltantes passaram por ponto cego próprio.
- **Não foi medido o impacto no planejador em escala.** O T1.4 prova que uma consulta ANN de 200 linhas continua usando o índice. Não diz nada sobre tabelas grandes, onde a estimativa de seletividade dos operadores novos poderia pesar diferente.
- **A semântica de igualdade exata não foi validada com usuário real.** É paridade deliberada com o upstream (ADR D1), mas ninguém confirmou que é o que os consumidores do TheoDB esperam.
- **O CI continua vermelho** (B-029). Esta mudança não conserta isso.

## Verificação no produto

`docker build` exit 0. Imagem exercitada com banco rodando — não é leitura de catálogo, é consulta.

| Verificação | Resultado |
|---|---|
| Operadores do tipo `vector` | `< <#> <-> <= <=> <> = > >=` — os 3 de distância **e** os 6 de ordem |
| Opclass btree | `vector_ops`, `default=true` |
| `WHERE e = '[1,2,3]'` | **2** linhas |
| `SELECT DISTINCT e` | **2** |
| `GROUP BY e` | **2** |
| `ORDER BY e` | `[1,2,3] [1,2,3] [9,9,9]` |
| `CREATE UNIQUE INDEX` sobre `vector` | criado |
| Duplicata sob índice único | `ERROR: duplicate key value violates unique constraint "u_e_idx"` |
| `ORDER BY e <-> …` (ANN) | `Index Scan using ax on emb` |

Paridade de semântica, no produto:

| Caso | Resultado | Significado |
|---|---|---|
| `cmp('[1,2]','[1,3]')` | `-1` | o elemento decide |
| `cmp('[1,2]','[1,2,0]')` | `-1` | prefixo igual: o mais curto vem antes |
| `cmp('[1,3]','[1,2,9]')` | `1` | **o elemento decide ANTES da dimensão** |
| `'[1]' < '[1,2]'` | `t` | dimensões diferentes comparam sem erro |

O terceiro caso é o que separa a implementação correta da suposição com que este trabalho começou. Se a dimensão fosse chave primária, o resultado seria `-1` — e nenhum outro teste pegaria a divergência.

## Veredito

**`READY_TO_MERGE`.**

Nenhum gate duro disparou; as 11 afirmações verificadas contra os testes **e** contra o produto em execução; os cinco padrões que motivaram o item funcionam; o caminho ANN não regrediu.

**Ressalvas que acompanham o veredito:**

- Review conduzido pelo mesmo agente que implementou. O R-1 (duas tarefas faltantes) é a evidência direta do custo disso, e é a segunda vez consecutiva que a varredura da matriz encontra algo dado por concluído.
- O impacto no planejador **em escala** não foi medido. O T1.4 prova que uma consulta ANN de 200 linhas continua usando o índice; nada foi medido sobre tabelas grandes, onde a seletividade dos operadores novos pesaria diferente.
- **B-029 permanece:** o CI está vermelho e esta mudança não o conserta.
