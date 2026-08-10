---
scenario: theo-rag-sobre-theodb
date: 2026-08-10
operator: claude-code/opus-5
outcome: partial
summary: O schema de produção do theo-rag — 24 tabelas — aplicado no TheoDB pelo drizzle-kit real do produto, com o banco healthy; a ingestão do corpus parou numa lacuna de seeding do próprio theo-rag.
---

# O que foi feito

Terceira evidência, e a que mais se aproxima de uso: **o schema de produção do `theo-rag` aplicado no TheoDB
pelo `drizzle-kit push` do próprio produto** — não um `CREATE TABLE` que eu escrevi.

```
Up (healthy) · accepting connections
24 tabelas em public
```

24 tabelas com as constraints, índices parciais e foreign keys reais do produto — incluindo
`webhook_deliveries_orphan_scan`, um índice parcial com `WHERE ... IS NULL` triplo, e as FKs de workspace.
**Tudo aceito pelo TheoDB sem uma adaptação.**

# Onde parou, e por que não é defeito nosso

A ingestão do corpus do golden set falhou:

```
error: insert or update on table "collections" violates foreign key constraint
       "collections_workspace_id_workspaces_id_fk"
```

O teste insere em `collections` sem criar o `workspace` pai. **A foreign key está fazendo exatamente o seu
trabalho** — é lacuna de seeding do `theo-rag`, e o comportamento correto do banco é justamente recusar.

Registro como achado do `theo-rag`, não do TheoDB, e resisto à tentação de contá-lo como evidência a nosso
favor: um teste que nunca rodou contra banco nenhum não prova nada sobre o nosso.

# O estado do âncora, sem maquiar

| exigência do DoD | estado |
|---|---|
| defeito achado por uso, não por benchmark | ✅ **dois** — o planner ([m175](../../../../wiki/benchmarks/m175-planner-cost-inversion-verdict.md)) e o mount do PG 18 |
| `theo-rag` servindo consultas reais na infraestrutura do time | ❌ |
| âncora `running`, ≥ 3 evidências, ≥ 1 história de falha | ❌ — 3 evidências, todas `partial`, zero falha em operação |

O que foi provado hoje: o TheoDB **aceita o schema real, sobe healthy, e passa 197 testes de integração do
produto**. O que não foi: que ele aguenta carga real, dados reais e falhas reais ao longo do tempo.

**Três evidências `partial` não somam uma `running`.** A diferença não é de quantidade — é de natureza:
nenhuma delas é uso, todas são verificação. O status permanece `planned` porque a golden rule mede a coisa
certa.
