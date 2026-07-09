# Edge Case Review — drop-pgvector-totally (M70)

Date: 2026-07-09
Plano: `.claude/knowledge-base/plans/drop-pgvector-totally-plan.md`
Tasks: 6 (T1.1–T6.1). Casos: 3 (MUST FIX: 1, SHOULD TEST: 2).

## MUST FIX

### EC-1: Testes do AM podem referenciar funções do umbrella (`theodb.*`) → CREATE EXTENSION theodb_rs sozinho falha
- **Affected task:** T5.1 (gate) — já é Unresolved Question do plano
- **Kind:** NEGATIVE (dependência oculta)
- **Scenario:** se algum dos 55 pg_tests do AM chama `theodb.embed`/`theodb.hybrid_*` (umbrella), `CREATE EXTENSION theodb_rs` sem o umbrella falha no setup.
- **Suggested fix:** T5.1 verifica empiricamente; se falhar, os testes do AM que dependem do umbrella instalam `theodb` também (CASCADE, que agora puxa theodb_rs). O gate CRÍTICO (recall) só precisa de tipo+AM (auto-contido). Adicionar à T5.1: "se um teste do AM precisa do umbrella, rodar com `CREATE EXTENSION theodb CASCADE` (sem pgvector); o gate de recall em si é auto-contido."

## SHOULD TEST

### EC-2: A ordem de criação no CASCADE (theodb→theodb_rs) com o schema `theodb` criado por ambos
- **Affected task:** T2.1
- **Suggested test:** `CREATE EXTENSION theodb CASCADE` — o theodb_rs cria `CREATE SCHEMA IF NOT EXISTS theodb` (idempotente) ANTES do umbrella adicionar objetos nele. O `IF NOT EXISTS` evita conflito. Testar a ordem.

### EC-3: `DROP EXTENSION vector` na migração falha se colunas ainda usam o tipo antigo
- **Affected task:** T4.1
- **Suggested test:** o playbook deve ordenar: (1) ALTER COLUMN TYPE (migra as colunas para public.vector) ANTES de (2) DROP EXTENSION vector. Senão o DROP falha (dependência). Documentar a ordem no playbook.

## Summary

| Task | MUST FIX | SHOULD TEST |
|------|----------|-------------|
| T2.1 | 0 | 1 (EC-2) |
| T4.1 | 0 | 1 (EC-3) |
| T5.1 | 1 (EC-1) | 0 |

**Verdict:** PLAN OK com 1 MUST-FIX (EC-1, já é Unresolved Question — resolver empiricamente na T5.1) + 2 SHOULD-TEST (ordem de CASCADE + ordem de migração). Cirúrgicos. Absorver e seguir.
