---
slug: b033-vector-btree
items: [B-033]
date: 2026-08-12
branch: workspace
---

# Igualdade e ordenação para o tipo `vector`

## Goal

Dar ao tipo `vector` own-code a opclass btree que o pgvector define — `=`, `<>`, `<`, `<=`, `>`, `>=` e `vector_cmp` — com **semântica byte-a-byte idêntica à do upstream**, de modo que os cinco padrões de app pgvector hoje quebrados passem a funcionar e a promessa de *"sem mudança de código"* do `ADR-0029 § D2` deixe de ser falsa.

## Baseline Context

**Base:** `515afa2` (workspace). Working tree limpa.

### Files that will be touched

| Arquivo | Linhas | Papel hoje | Mudança |
|---|---|---|---|
| `theodb_rs/src/dtype.rs` | ~430 | define o tipo `vector`, seus 3 operadores de distância, casts e o bootstrap de schema | **+7 funções Rust, +1 bloco `extension_sql!`** |
| `theodb_rs/src/surface_contract.rs` | ~250 | contrato da superfície instalada (presença, ACL, linguagem) | **+1 teste** de presença dos operadores |
| `CHANGELOG.md` | — | contrato público | **+1 entrada** em `[0.160.0]` |

Nenhum arquivo é removido. Nenhuma assinatura existente muda.

### Current callers / dependents

| Chamador | Referência | Efeito |
|---|---|---|
| `theodb_vector_l2_distance` e irmãs | `dtype.rs:331,337,343` | **nenhum** — o caminho de distância não é tocado |
| Opclasses `theodb_hnsw_*` / `theodb_ivfflat_*` | AMs próprios | **nenhum** — outro método de acesso |
| Shim `vector` (`vector/vector--0.6.0.sql`) | aliases para o AM `hnsw` | **nenhum** — nomes de opclass são únicos por AM |
| Apps pgvector | `WHERE e = …`, `DISTINCT`, `GROUP BY`, `ORDER BY`, `UNIQUE` | **passam a funcionar** |

### Domain glossary

| Termo | Significado neste plano |
|---|---|
| opclass btree | conjunto de operadores + função de comparação que ensina o PostgreSQL a ordenar um tipo |
| ordem total | relação em que quaisquer dois valores são comparáveis, e a comparação é transitiva e antissimétrica — pré-condição do btree |
| desempate por dimensão | quando dois vetores coincidem nos primeiros `Min(dim_a,dim_b)` elementos, o mais curto vem antes |
| seletividade | as funções `eqsel`/`scalarltsel` que o planejador usa para estimar quantas linhas um filtro devolve |
| caminho ANN | `ORDER BY e <-> …`, servido pelas opclasses dos AMs próprios; **não** é btree |

### Architecture boundaries affected

```
tipo public.vector  (theodb_rs/src/dtype.rs)
  ├── operadores de distância  <-> <=> <#>   ──> opclasses dos AMs ANN   [INTOCADO]
  └── operadores de ordem      = <> < <= > >= ──> opclass btree vector_ops [NOVO]
```

A fronteira nova é **aditiva e ortogonal**: distância e ordem são famílias de operadores distintas, servidas por métodos de acesso distintos. Nenhum caminho existente muda de resolução.

## Prior Art

- **`pgvector/src/vector.c`, `vector_cmp_internal`** — consultado na fonte upstream em 2026-08-12 (o acervo local não está no disco, conforme `CLAUDE.md § acervo`). Define a semântica: elementos até `Min(dim_a,dim_b)`, depois desempate por dimensão, **sem `CheckDims`**. Derrubou a suposição com que este trabalho começou.
- **`pgvector/sql/vector.sql`** — a declaração exata da opclass e dos 6 operadores, com `COMMUTATOR`/`NEGATOR` e as funções de seletividade.
- **`theodb_rs/src/dtype.rs:396-430`** — o bloco `extension_sql!` que já declara o tipo e os 3 operadores de distância. O novo bloco segue este padrão, não inventa outro.
- **`.claude/rules/parsimony-ladder.md`** — degrau 2 (a plataforma resolve?): a comparação lexicográfica é `slice::cmp` da stdlib sobre `f32` ordenados por `partial_cmp`; não há motivo para escrever laço próprio.
- **`.claude/rules/testing.md § 4.1`** — a distinção entre caso de borda e caso negativo é o que estrutura o plano de teste: dimensão diferente é **borda** (valor válido no extremo), não negativo.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa | Verificação |
|---|---|---|---|
| G1 | `vector_cmp` com semântica idêntica ao upstream | T1.1 | teste unitário sobre a função pura, com os casos do `vector_cmp_internal` |
| G2 | Ordem total (pré-condição do btree) | T1.1 | teste de antissimetria, transitividade e reflexividade |
| G3 | Os 6 operadores existem no catálogo | T1.2 | `pg_operator` contém `= <> < <= > >=` para `vector` |
| G4 | Opclass btree é a DEFAULT do tipo | T1.2 | `CREATE INDEX ON t (e)` sem nomear opclass |
| G5 | `WHERE e = …` funciona | T1.3 | consulta devolve a contagem correta |
| G6 | `SELECT DISTINCT e` funciona | T1.3 | deduplica corretamente |
| G7 | `GROUP BY e` funciona | T1.3 | agrupa corretamente |
| G8 | `ORDER BY e` funciona | T1.3 | ordena conforme `vector_cmp` |
| G9 | `UNIQUE` sobre coluna `vector` funciona **e rejeita duplicata** | T1.3 | índice constrói; segundo insert idêntico falha |
| G10 | Caminho ANN não regride | T1.4 | `EXPLAIN` de `ORDER BY e <-> …` ainda usa o índice ANN |
| G11 | Superfície declarada no contrato | T1.5 | `surface_contract` assere os operadores |

Cobertura: **11 de 11 afirmações mapeadas (100%)**. Tarefas T1.1–T1.5, todas presentes acima.

## ADRs

### D1 — Paridade byte-a-byte com o upstream, não uma semântica "melhor"

**Decisão:** `vector_cmp` replica exatamente `vector_cmp_internal` do pgvector — compara até `Min(dim_a,dim_b)`, desempata por dimensão, nunca levanta erro.

**Alternativas consideradas:**

1. *Comparar dimensão primeiro, depois elementos.* **Rejeitada.** Foi a minha proposta inicial e ela é *defensável isoladamente*, mas produz ordenação **diferente** da do pgvector para vetores de dimensões distintas — e o objetivo declarado deste item é drop-in. Ser diferente-e-razoável cria uma incompatibilidade nova enquanto conserta a antiga.
2. *`check_dims` e erro em dimensão diferente*, como fazem `theodb_vector_l2_distance` e irmãs. **Rejeitada por medição:** eu assumi que era isso que o pgvector fazia, e a fonte mostrou que não. Além de divergir, erro dentro de função de comparação faria `ORDER BY` e construção de índice falharem sobre colunas sem typmod fixo.
3. *Igualdade com tolerância* (`|a-b| < ε`). **Rejeitada, e é a mais perigosa das três.** Quebra transitividade: `a≈b` e `b≈c` não implica `a≈c`. Um btree construído sobre relação não-transitiva corrompe **silenciosamente** — busca deixa de encontrar linhas que existem.
4. *Paridade exata (escolhida).*

**Consequência aceita, e ela é contraintuitiva:** `'[0.1]'::vector = '[0.1]'::vector` é verdadeiro, mas igualdade exata de ponto flutuante raramente é o que o usuário *quer* — dois vetores calculados por caminhos diferentes quase nunca são bit-idênticos. Isso é herdado do pgvector de propósito: o usuário que migra recebe o comportamento que já conhecia.

### D2 — A opclass vive no tipo, não no shim de compatibilidade

**Decisão:** o bloco vai em `theodb_rs/src/dtype.rs`, junto do `CREATE TYPE vector`.

**Alternativas consideradas:**

1. *Declarar no shim `vector/vector--0.6.0.sql`.* **Rejeitada:** igualdade é propriedade **do tipo**, não da compatibilidade. Um usuário que instale só `theodb_rs`, sem o shim, precisa de `=` — hoje `theodb_rs` sozinho já entrega o tipo e as distâncias, e deixar a ordem para o shim faria o tipo próprio ser menos capaz que ele mesmo sob outro nome.
2. *Módulo Rust novo `vector_btree.rs`.* **Rejeitada:** são 7 funções finas sobre uma comparação; separá-las do `CREATE TYPE` que elas completam espalharia a definição do tipo por dois arquivos sem ganho. O `dtype.rs` tem uma razão para mudar — o tipo `vector` — e isto é o tipo.
3. *No `dtype.rs`, junto do tipo (escolhida).*

### D3 — Uma função de comparação, seis operadores derivados

**Decisão:** `theodb_vector_cmp` contém **toda** a lógica; `eq`/`ne`/`lt`/`le`/`gt`/`ge` são derivações de uma linha sobre o resultado dela.

**Alternativas consideradas:**

1. *Implementar cada operador independentemente.* **Rejeitada:** seis implementações da mesma regra é duplicação de **conhecimento**, não de linhas — mudar a semântica exigiria acertar seis lugares, e divergência entre `=` e `cmp` produziria um btree incoerente com os operadores que o consultam. É exatamente o que a Regra 12 (DRY) protege.
2. *Uma função com parâmetro de modo* (`cmp(a,b,op)`). **Rejeitada:** o PostgreSQL exige assinaturas distintas por operador; um despachante interno só acrescentaria indireção.
3. *Uma comparação + seis derivações (escolhida).* Fonte única de verdade para a ordem.

## Tasks

### T1.1 — A comparação, e a prova de que é ordem total

#### Why this step

Toda a corretude do btree depende desta função. Se ela não for uma ordem total, o índice não fica "um pouco errado" — ele passa a **não encontrar linhas que existem**, silenciosamente. É o único ponto do plano onde um erro é invisível em vez de ruidoso, então vem primeiro e com a prova junto.

#### TDD

```
test_cmp_matches_upstream_semantics
  arrange: pares dos casos do vector_cmp_internal
  act:     theodb_vector_cmp(a,b)
  assert:  [1,2] vs [1,3]   -> -1   (elemento decide)
           [1,2] vs [1,2]   ->  0
           [1,2] vs [1,2,0] -> -1   (prefixo igual, dimensão desempata)
           [1,3] vs [1,2,9] ->  1   (elemento decide ANTES da dimensão)
  estado hoje: NÃO COMPILA — a função não existe

test_cmp_is_a_total_order
  assert:  reflexividade  cmp(a,a)=0 para todo a do conjunto
           antissimetria  sign(cmp(a,b)) = -sign(cmp(b,a))
           transitividade cmp(a,b)<=0 e cmp(b,c)<=0  =>  cmp(a,c)<=0
  sobre um conjunto que inclui dimensões DIFERENTES e valores negativos
```

O quarto caso (`[1,3]` vs `[1,2,9]` → `1`) é o que distingue a semântica correta da minha suposição inicial: dimensão **não** decide primeiro.

#### Acceptance criteria

- `cargo pgrx test pg18 -- cmp` termina em exit code 0
- os quatro casos de `test_cmp_matches_upstream_semantics` passam
- `test_cmp_is_a_total_order` cobre as três propriedades sobre dimensões mistas

### T1.2 — Os operadores e a opclass no catálogo

#### Why this step

Sem a declaração SQL, as funções Rust existem e o PostgreSQL não as encontra. É o passo que transforma código em superfície.

#### TDD

```
test_vector_has_btree_opclass
  act:    SELECT count(*) FROM pg_opclass oc JOIN pg_am am ON am.oid=oc.opcmethod
          WHERE am.amname='btree' AND oc.opcname='vector_ops' AND oc.opcdefault
  assert: 1
  estado hoje: FALHA (devolve 0)

test_vector_comparison_operators_exist
  act:    operadores de vector em pg_operator
  assert: contém = <> < <= > >=
  estado hoje: FALHA (só <-> <=> <#>)
```

#### Acceptance criteria

- `pg_opclass` tem `vector_ops` em `btree`, marcada `opcdefault`
- os 6 operadores existem com `COMMUTATOR`/`NEGATOR` conforme o upstream
- as 7 funções são `IMMUTABLE STRICT PARALLEL SAFE`

### T1.3 — Os cinco padrões que o item reporta

#### Why this step

São a razão do item existir. Cada um foi medido falhando; cada um precisa ser medido passando — **os mesmos cinco**, não um teste genérico que "verifica igualdade".

#### TDD

```
test_pgvector_query_patterns_work
  arrange: tabela emb(id int, e vector(3)) com [1,2,3],[1,2,3],[9,9,9]
  assert:  WHERE e = '[1,2,3]'          -> 2 linhas
           SELECT DISTINCT e            -> 2 valores
           GROUP BY e                   -> 2 grupos
           ORDER BY e                   -> [1,2,3],[1,2,3],[9,9,9]
           CREATE UNIQUE INDEX ON (e)   -> FALHA (há duplicata) — e é o resultado CERTO
  estado hoje: os cinco erram com "operator does not exist" / "no default operator class"

test_unique_index_rejects_duplicate
  arrange: tabela sem duplicata + UNIQUE sobre a coluna vector
  act:     inserir vetor idêntico a um existente
  assert:  erro de violação de unicidade
```

O quinto caso do primeiro teste é sutil e proposital: sobre dados **com** duplicata, `CREATE UNIQUE INDEX` deve **falhar** — provar que ele constrói exigiria dados sem duplicata, e é o que o segundo teste faz.

#### Acceptance criteria

Os cinco padrões medidos no B-033 passam, e o índice único rejeita duplicata de verdade.

### T1.4 — O caminho ANN não regride

#### Why this step

Adicionar `=` ao tipo dá ao planejador uma alternativa que ele não tinha. O risco é que uma consulta ANN passe a resolver por outro caminho — e o sintoma seria lentidão silenciosa, não erro.

#### TDD

```
test_ann_path_still_uses_the_ann_index
  arrange: tabela com índice theodb_hnsw
  act:     EXPLAIN de  ORDER BY e <-> '[…]'::vector LIMIT k
  assert:  o plano contém Index Scan sobre o índice ANN
  estado hoje: PASSA (é rede de regressão, não alvo)
```

#### Acceptance criteria

O plano de uma consulta ANN é idêntico antes e depois da mudança.

### T1.5 — Declarar no contrato de superfície

#### Why this step

O `surface_contract` é o oráculo que diz o que a extensão entrega. Uma superfície nova que não entra nele é uma superfície que pode sumir sem ninguém perceber — foi exatamente esse o argumento do ciclo anterior.

#### TDD

```
test_extension_surface_contains_public_api   (existente, estendido)
  assert: os 6 operadores e vector_ops entram na lista esperada
```

#### Acceptance criteria

`surface_contains_public_api` cobre a superfície nova e continua verde.

## Failure scenarios

O tipo `vector` não faz I/O externo; os cenários relevantes são de **dado**, não de rede.

| Cenário | Comportamento exigido | Onde é provado |
|---|---|---|
| Dimensões diferentes na comparação | ordena por prefixo e desempata por dimensão; **não** levanta erro | T1.1 |
| NaN ou infinito num vetor | impossível — rejeitados na entrada (`dtype.rs:199,202`), o que é a pré-condição da ordem total | T1.1 (o teste de ordem total só é válido sob esta garantia) |
| Vetor de dimensão zero | comparação devolve `0` contra outro vazio e `-1` contra qualquer não-vazio | T1.1 |
| Duplicata sob índice único | erro de unicidade do PostgreSQL, não corrupção | T1.3 |
| Planejador escolhendo btree para consulta ANN | não acontece; o plano ANN permanece | T1.4 |

## Concurrency tests

**(none — single-threaded.)** A mudança adiciona funções puras e imutáveis sobre valores já materializados. Não introduz estado compartilhado, thread, nem caminho concorrente. A concorrência do produto vive nos AMs e no buffer manager, que não são tocados.

## Dependencies

**Nenhuma dependência nova.** Degrau 4 da parsimony ladder: tudo já está declarado.

| Dependência | Versão | Já instalada | Papel |
|---|---|---|---|
| `pgrx` | 0.19.0 | sim | `#[pg_extern]` e `extension_sql!` |
| `cargo-pgrx` | 0.19.0 | sim | `cargo pgrx test pg18` |
| PostgreSQL | 18.4 | sim | alvo do teste |

Sem manifesto novo, não há superfície de CVE a auditar.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação |
|---|---|---|---|
| R1 | Comparação não-transitiva corromperia o btree **em silêncio** — busca deixaria de achar linhas existentes | máxima | T1.1 prova a transitividade explicitamente; e a alternativa que a quebraria (igualdade com tolerância) foi rejeitada no ADR D1 |
| R2 | Divergir da semântica do upstream cria incompatibilidade nova ao consertar a antiga | alta | ADR D1 fixa paridade byte-a-byte, fundamentada em fonte primária, não em memória |
| R3 | O planejador passar a preferir btree numa consulta ANN — sintoma seria lentidão, não erro | média | T1.4 compara o plano antes e depois |
| R4 | Igualdade exata de float confunde quem espera tolerância | média | herdado do pgvector de propósito (ADR D1); documentado no CHANGELOG em linguagem de usuário |
| R5 | A suíte `cargo pgrx test` é cara e o CI está vermelho (B-029) | baixa | rodar localmente no contêiner com cache incremental, como no ciclo anterior |

## Unresolved Questions

- Q1: O shim `vector` deve declarar algo a mais para o drop-in ficar completo, ou a opclass no tipo já cobre? Levantar em T1.2 conferindo se `vector_ops` resolve sem o shim instalado. **Hipótese:** cobre, porque o tipo é do `theodb_rs`; confirmar em vez de assumir.
- Q2: Existem outros operadores do pgvector ausentes além destes seis? A medição do B-033 olhou só igualdade/ordenação. Levantar comparando o inventário completo de `pg_operator` do upstream com o nosso — fora do escopo deste plano, vira item novo se houver.
