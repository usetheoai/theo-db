# Edge Case Review — own-vector-type (M69)

Date: 2026-07-09
Plano: `.claude/knowledge-base/plans/own-vector-type-plan.md`
Tasks analisadas: 5 (T1.1–T5.1)
Casos encontrados: 5 (EDGE: 3, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 3, DOCUMENT: 1)

> Contexto: o spike (7/7 pg_test, ADR-D3) já retirou os grandes riscos e validou a mecânica.
> Este review foca nos edges da IMPLEMENTAÇÃO completa (recv/send, casts, memória varlena).

## MUST FIX

### EC-1: `into_datum` de um `TheoVec` que veio de `from_datum` (detoast_copy) — quem libera o quê?
- **Affected task:** T1.1 (datum plumbing)
- **Kind:** NEGATIVE (uso de memória)
- **Family:** State / Resource
- **Scenario:** um `#[pg_extern]` que recebe `TheoVec` (via `from_datum` = detoast_copy, own ptr) e o RETORNA (ex.: a length-coercion cast `theodb_vector(v, typmod, _)` retorna `v`). O `into_datum` faz `into_raw()` (forget → não libera), mas o Drop libera se NÃO for retornado. Se o mesmo ptr for usado após `into_raw`, é use-after-free; se `Drop` rodar num ptr já retornado, é double-free.
- **Impact:** corrupção de memória / crash no path do cast (que a T2.1 exercita muito).
- **Suggested fix:** garantir que `into_datum` consome `self` via `into_raw()` (mem::forget) — exatamente como o spike (`into_raw` faz `mem::forget(self)`); o Drop só roda no path onde o valor NÃO é retornado. Adicionar 1 pg_test que faz `SELECT ('[1,2,3]'::theodb.vector)::theodb.vector(3)` (cast que recebe E retorna) rodando 1000× num loop SQL p/ pegar double-free sob repetição.

## SHOULD TEST

### EC-2: dimensão no boundary exato (1 e 16000)
- **Affected task:** T1.1 / T4.1
- **Kind:** EDGE (extremo válido)
- **Suggested test:** `test_dim_boundary` — `'[1]'::theodb.vector` (dim=1, mínimo válido) e um vetor de dim=16000 (máximo válido) round-trip OK; dim=16001 → erro "cannot exceed 16000". Paridade com `vector.c:88-100`.

### EC-3: `unused` != 0 no recv binário (input binário adversário/corrompido)
- **Affected task:** T2.1 (recv)
- **Kind:** NEGATIVE (input inválido)
- **Suggested test:** `test_recv_rejects_nonzero_unused` — construir um wire binário com `unused=1` e assere ERROR "expected unused to be 0" (paridade `vector.c:378-388`). Protege contra COPY BINARY de fonte corrompida.

### EC-4: cast binário com pgvector quando o vetor tem dim grande (o layout tem que bater em TODO tamanho)
- **Affected task:** T3.1 (binary_compat)
- **Kind:** EDGE
- **Suggested test:** o `binary_compat_with_pgvector` deve testar com dim=1, dim=3 E dim grande (ex. 128) — o layout `8+4·dim` tem que ser byte-idêntico em qualquer dim, não só dim=3. Um off-by-one no header só apareceria em dim variado.

## DOCUMENT

### EC-5: `f32::to_string()` (Rust) vs `float_to_shortest_decimal` (pgvector) no `_out`
- **Kind:** EDGE (já em Unresolved Questions)
- **Accepted risk:** já documentado no plano (§ Unresolved Questions + Drawbacks). O gate de paridade T4.1 pega qualquer divergência (ex. `0.1`, subnormais). Se divergir, o fix é replicar o algoritmo do pg (ryu/grisu) — bounded. Não bloqueia o design.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 | 1 | 1 (EC-1) | 1 (EC-2) | 0 |
| T2.1 | 0 | 1 | 0 | 1 (EC-3) | 0 |
| T3.1 | 1 | 0 | 0 | 1 (EC-4) | 0 |
| T4.1 | 1 | 0 | 0 | 0 | 1 (EC-5) |
| T5.1 | 0 | 0 | 0 | 0 | 0 |

**Verdict:** PLAN NEEDS MINOR ADJUSTMENT — 1 MUST FIX (EC-1, memória varlena no path de cast — o mais perigoso) + 3 SHOULD TEST (boundaries + recv adversário + cast dim-variado). Todos cirúrgicos (testes + a disciplina de `into_raw`/`Drop` do spike). Absorver na v1.1 e seguir para /implement.
