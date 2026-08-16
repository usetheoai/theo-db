---
item: B-033
mode: bug
date: 2026-08-12
verdict: pending
---

# B-033 — O tipo `vector` não tem igualdade, e cinco padrões de app pgvector quebram

## Corner 1 — Evidence

Medido em 2026-08-12 contra a imagem `theodb:arrumacao`, num banco que herdou o shim de `template1`.

### O que existe e o que falta

```sql
SELECT string_agg(oprname,' ') FROM pg_operator o
JOIN pg_type t ON t.oid=o.oprleft WHERE t.typname='vector';
-- <#> <-> <=>

SELECT count(*) FROM pg_operator o JOIN pg_type t ON t.oid=o.oprleft
WHERE t.typname='vector' AND o.oprname='=';
-- 0
```

Três operadores de distância; **zero** de igualdade ou ordenação.

### O que quebra, com tabela real de 3 linhas

| Padrão | Erro |
|---|---|
| `WHERE e = '[1,2,3]'::vector` | `operator does not exist: vector = vector` |
| `SELECT DISTINCT e` | `could not identify an equality operator for type vector` |
| `GROUP BY e` | `could not identify an equality operator for type vector` |
| `ORDER BY e` | `could not identify an ordering operator for type vector` |
| `CREATE UNIQUE INDEX ON emb (e)` | `data type vector has no default operator class for access method "btree"` |
| `ORDER BY e <-> '[1,2,3]'::vector` (ANN) | **funciona** |

### O contrato do pgvector, de fonte primária

Consultado em 2026-08-12 (o acervo local não está no disco — `CLAUDE.md` § acervo declara isso —, então a fundamentação subiu para o degrau seguinte: a fonte upstream).

`pgvector/src/vector.c`, `vector_cmp_internal` — **e isto derruba a suposição com que eu comecei**:

```c
int dim = Min(a->dim, b->dim);
for (int i = 0; i < dim; i++) {
    if (a->x[i] < b->x[i]) return -1;
    if (a->x[i] > b->x[i]) return 1;
}
if (a->dim < b->dim) return -1;
if (a->dim > b->dim) return 1;
return 0;
```

**Não chama `CheckDims`.** Compara valores até o menor comprimento e só então desempata por dimensão — ordem total sobre todos os vetores, sem nunca levantar erro. Eu havia assumido o contrário (que erraria em dimensão diferente, como as funções de distância deste projeto fazem); a fonte corrigiu.

`pgvector/sql/vector.sql` — a opclass e os operadores:

```sql
CREATE OPERATOR CLASS vector_ops DEFAULT FOR TYPE vector USING btree AS
    OPERATOR 1 < , OPERATOR 2 <= , OPERATOR 3 = , OPERATOR 4 >= , OPERATOR 5 > ,
    FUNCTION 1 vector_cmp(vector, vector);

CREATE OPERATOR = (LEFTARG=vector, RIGHTARG=vector, PROCEDURE=vector_eq,
    COMMUTATOR = = , NEGATOR = <> , RESTRICT = eqsel, JOIN = eqjoinsel);
-- < <= > >= usam scalarltsel/scalarlesel/scalargtsel/scalargesel
```

As sete funções são `IMMUTABLE STRICT PARALLEL SAFE`.

### Por que a ordem é total aqui (pré-condição de btree)

`theodb_rs/src/dtype.rs:199,202,302,305` **rejeita NaN e infinito na entrada**. Sem isso, comparação de `f32` não formaria ordem total (NaN não é comparável a nada, nem a si mesmo) e um índice btree corromperia em silêncio. Como são rejeitados, a comparação elemento a elemento é uma ordem total sobre os valores representáveis.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `theodb_rs/src/dtype.rs` | onde o tipo e seus operadores são declarados |
| Superfície SQL | **adiciona** 6 operadores + 1 opclass + 7 funções; não altera nem remove nada |
| Índices ANN existentes | intocados — `<->`/`<=>`/`<#>` e as opclasses `theodb_hnsw_*` não mudam |
| Shim `vector` | intocado — ele declara aliases para o AM `hnsw`, outro método de acesso |
| Repos irmãos | nenhum consome a extensão |

**Risco de colisão avaliado:** o shim já cria `vector_l2_ops`/`vector_cosine_ops`/`vector_ip_ops` para o AM `hnsw`. Nomes de opclass são únicos **por método de acesso**, então `vector_ops` em `btree` não colide.

## Corner 4 — Verification

1. Os cinco padrões medidos acima passam a funcionar — os mesmos cinco, verificados um a um.
2. `ORDER BY e <-> ...` continua usando o índice ANN (o `=` não pode roubar o caminho de distância).
3. `vector_cmp` respeita o contrato do pgvector: valores primeiro, dimensão como desempate, sem erro.
4. Ordem total provada por teste: antissimetria, transitividade e reflexividade sobre um conjunto de vetores com dimensões diferentes.
5. `CREATE UNIQUE INDEX` sobre coluna `vector` constrói e **rejeita duplicata**.

## Reclassificação

`suggested_mode: bug` mantido. É defeito de compatibilidade declarada, não evolução: o `ADR-0029 § D2` promete *"sem mudança de código"*, e a consulta falha.
