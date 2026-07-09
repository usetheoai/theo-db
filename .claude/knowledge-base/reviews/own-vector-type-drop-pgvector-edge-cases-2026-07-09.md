# Discover Edge Case Review — own-vector-type-drop-pgvector

Date: 2026-07-09
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/own-vector-type-drop-pgvector-plan.md
Research questions analyzed: 6
Edge cases found: 6 (MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 3)

## MUST FIX

### EC-1: Nenhuma referência clonada define um TIPO `vector` próprio em pgrx — o padrão pgrx da definição-de-tipo não tem fonte local
- **Affected question:** Q2 (e Q1)
- **Family:** Reference path / Interpretation
- **Scenario:** Q2 pede "o análogo em pgrx 0.16.1" da ligação custom-type↔custom-AM. Mas a evidência confirma que **os dois AMs próprios in-scope REUSAM o tipo do pgvector** (`vectorchord/vchord.control` e `pgvectorscale` ambos `requires vector`); pgvector e postgres/cube definem o tipo em **C**, não em pgrx. Ou seja: `pgvectorscale/.../pg_vector.rs` mostra como um AM pgrx **consome** `vector`, mas **nenhum clone in-scope mostra como DEFINIR um tipo SQL `vector` em pgrx** (I/O in/out/recv/send via `#[pg_extern]`/`extension_sql! CREATE TYPE`, typmod).
- **Impact:** o /discover-execute vai buscar no código clonado um padrão pgrx de definição-de-tipo que não existe lá → Fase A exausta → risco de BLOCKED ou (pior) de fabricar. E o blueprint perderia o "como" central da implementação.
- **Suggested fix:** em Q2 (e Q1), declarar explicitamente que **o padrão pgrx de DEFINIÇÃO do tipo (I/O, typmod, `extension_sql! CREATE TYPE`) é R0-web-sourced** (pgrx book/examples + docs pg "User-defined Types"), enquanto os clones fornecem (a) o **contrato/semântica** a espelhar (pgvector C) e (b) o **consumo** do tipo por um AM pgrx (pgvectorscale). Este próprio fato — nenhum peer permissivo shipa tipo próprio em pgrx — é um **finding** do blueprint (reforça "território novo, custo real"), não só um gap.

## SHOULD TEST

### EC-2: Q6 (migração binary-compat) depende de Q1 (layout do struct) — ordem não declarada
- **Affected question:** Q6 (depende de Q1)
- **Suggested halt-loop checkpoint:** "Antes de responder Q6, Q1 deve estar `done` e a resposta de Q6 DEVE citar o layout do struct `Vector` de Q1 (varlena header + int16 dim + float4[]). Se o layout own-code for byte-idêntico ao do pgvector, a migração de tabelas existentes é reinterpretação binária (drop-in); senão, exige recast." — adicionar aos Halt-loop Checkpoints.

### EC-3: Q6 pode não achar precedente de ALTER-de-layout nas migrations do pgvector (o pgvector nunca mudou o layout binário)
- **Affected question:** Q6
- **Suggested halt-loop checkpoint:** "Se `pgvector/sql/vector--*.sql` não contém ALTER que mude o layout do tipo (provável — o layout é estável desde a v0.1), NÃO marcar Fase A exausta; fazer fallback para docs pg core (binary coercibility / `CREATE CAST ... WITHOUT FUNCTION`) via R0 web como a fonte do caminho de migração." — adicionar aos Halt-loop Checkpoints.

## DOCUMENT

### EC-4: nomes de função em `vector.c` podem não casar o grep verbatim
- **Accepted risk:** se `vector_in|vector_out|vector_recv|vector_send` não casarem literalmente, o fallback é ler `pgvector/src/vector.c` inteiro (arquivo pequeno, dentro do budget de 4h do pgvector). O plano já prevê "Read cada função"; o fallback é implícito e barato.

### EC-5: pgvector testa via pg_regress (.sql/.out); theodb_rs testa via pgrx `#[pg_test]`
- **Accepted risk:** Q4/Q5 extraem os **casos** de paridade (I/O round-trip, NaN/Inf, dim-mismatch, operadores, typmod, index-scan-sobre-o-tipo), não o **harness**. Os casos transferem para `#[pg_test]`; a diferença de mecanismo é irrelevante para o gate de correção. Documentado.

### EC-6: budget do pgvector (4h) cobre 5 questions que o tocam (Q1,Q2,Q4,Q5,Q6)
- **Accepted risk:** budgets são soft; o halt-loop para honestamente e marca BLOCKED "budget exhausted" se estourar. Q1 (vector.c) e Q2 (vector.sql+hnsw.c) são os fundos; Q4/Q5/Q6 são leituras mais rasas de test/ e migrations. Aceitável; se estourar, as remanescentes viram seed do próximo discovery.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 1 (EC-4) | 0 | 0 | 1 |
| Q2 | 1 (EC-1) | 1 | 0 | 0 |
| Q3 | 0 | 0 | 0 | 0 |
| Q4 | 1 (EC-5) | 0 | 0 | 1 |
| Q5 | 0 | 0 | 0 | 0 |
| Q6 | 3 (EC-2,EC-3,EC-6) | 0 | 2 | 1 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT — 1 MUST FIX (EC-1: declarar o padrão pgrx de definição-de-tipo como R0-web-sourced + finding) + 2 checkpoints (EC-2/EC-3). Ajustes cirúrgicos, não reescrita. Absorver em v1.1 e seguir para /discover-plan-confidence.
