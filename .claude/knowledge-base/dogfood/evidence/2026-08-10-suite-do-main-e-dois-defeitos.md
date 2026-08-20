---
scenario: theo-rag-sobre-theodb
date: 2026-08-10
operator: claude-code/opus-5
outcome: partial
summary: A suíte de integração do theo-rag adotado em main roda contra o TheoDB — 303 passam — e produziu duas HISTÓRIAS DE FALHA reais, que é a evidência que a golden rule diz não poder ser fabricada.
---

# O que foi feito

`theo-rag` no `main` — já com `ghcr.io/usetheoai/theo-db:0.140.0` — subido e exercitado pela sua própria
suíte de integração.

```
Test Files  5 failed | 42 passed | 5 skipped (52)
Tests       7 failed | 303 passed | 12 skipped (322)
```

Contêiner `Up (healthy)`, 24 tabelas do schema real aplicadas pelo `drizzle-kit` do produto.

# Duas histórias de falha, e elas são NOSSAS

Cinco das sete falhas são do lado do `theo-rag` (`ROADMAP-v8.md` ausente, `column c.text_search does not
exist`). **Duas são do TheoDB:**

**B-018 — o planner não alcança o HNSW no caminho de JUNÇÃO.** Mesmo com `enable_seqscan = off`, o plano é
`Limit → Sort → Nested Loop → Index Scan`. O `Sort` acima prova que o índice não serve a ordenação.

Isto é o mais instrutivo do dia: **a correção do planner que eu fiz hoje** ([m175](/benchmarks/m175-planner-cost-inversion-verdict.md))
resolveu a busca simples — que era o caminho que eu escolhi medir — e **não cobre a junção**, que é o caminho
que o produto de verdade usa. Eu havia declarado aquela correção verificada ponta a ponta, e era verdade para
o que medi.

**B-019 — `CREATE INDEX` de HNSW não é idempotente.** Recriar um índice existente estoura com
`duplicate key value violates unique constraint "pg_class_relname_nsp_index"` em vez de ser no-op. O
`theo-rag` chama `ensureHnswIndex` na inicialização; estourar quebra reinício de serviço.

# O que isto move no âncora, e o que não move

**Move:** a golden rule exige, entre os soft caps, *"failure stories present ≥ 1 — a dogfood without failures
is theatre"*. Agora há **duas**, e são falhas do produto sob a carga de outro produto — não de um harness que
eu escrevi.

**Não move:** o hard cap 2 continua sendo `Status: running` = *"ativamente usado pelo time em infraestrutura
real"*. Isto rodou na minha máquina, num contêiner efêmero, dirigido por uma suíte de teste. É o exercício
mais próximo de uso que existiu até agora, e ainda não é uso.

# O placar de defeitos achados por uso, no dia

| # | defeito | achado ao |
|---|---|---|
| 1 | planner rejeita índice vetorial (182 ms vs 6 ms) | verificar o drop-in |
| 2 | mount do PG 18 → contêiner em loop | rodar o compose real |
| 3 | workflow de publicação com org inexistente | tentar publicar |
| 4 | planner não alcança o HNSW na junção | rodar a suíte do `theo-rag` |
| 5 | `CREATE INDEX` não idempotente | idem |

**Nenhum foi detectado pelos 109 artefatos de benchmark do projeto.** O mecanismo é sempre o mesmo, e é o
argumento inteiro do dogfood: benchmark mede o caminho que se escolhe medir.
