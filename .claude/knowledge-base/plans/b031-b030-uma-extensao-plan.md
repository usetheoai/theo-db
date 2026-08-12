---
slug: b031-b030-uma-extensao
items: [B-031, B-030]
date: 2026-08-12
branch: workspace
---

# Uma extensão, um caminho de instalação

## Goal

Reduzir o TheoDB de **três extensões PostgreSQL a duas** — `theodb_rs` (o produto) e `vector` (o shim de compatibilidade, que permanece por contrato de nome) — eliminando o umbrella `theodb` e as cadeias de upgrade que existem para um gate que não roda e instalações que não existem.

Ao fazê-lo, recuperar uma garantia hoje perdida: os wrappers públicos voltam a ser validados em tempo de `CREATE`, e a ACL da superfície de egress passa a ser verificada por teste — o ponto cego que o oráculo atual declara não cobrir.

## Baseline Context

**Base:** `3050243` (workspace), 4 commits à frente de `bcf7819`. Working tree limpa.

### Files that will be touched

| Arquivo | Linhas | Papel hoje | Destino |
|---|---|---|---|
| `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql` | 2391 | elo de upgrade (re-emissão convergente) | removido em T1.1 |
| `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` | 2444 | idem | removido em T1.1 |
| `theodb_rs/sql/theodb_rs--1.2.0--1.3.0.sql` | 2478 | idem | removido em T1.1 |
| `theodb_rs/sql/theodb_rs--1.3.0--1.4.0.sql` | 25 | delta à mão (drop SymQG) | removido em T1.1 |
| `theodb_rs/sql/theodb_rs--1.4.0--1.5.0.sql` | 2472 | idem 1.0.0 | removido em T1.1 |
| `theodb_rs/sql/schema_snapshot.sql` | 38 | oráculo de membresia | mantido; vira insumo do T2.1 |
| `sql/theodb--1.0--1.1.sql` … `--1.5--1.6.sql` | 366 | 6 deltas do umbrella | removidos em T1.2 |
| `sql/vector--0.5.1.sql` | 30 | install antigo do shim | removido em T1.3 |
| `sql/vector--0.5.1--0.6.0.sql` | 35 | delta do shim | removido em T1.3 |
| `sql/vector--0.6.0.sql` | 67 | install corrente do shim | mantido |
| `sql/50-theodb-ai.sql` | 113 | `ai.generate/summarize/agg_summarize` | migrado em T3.1 |
| `sql/60-theodb-nl.sql` | 64 | `ai.nl_query` | migrado em T3.2 |
| `sql/61-theodb-nl-config.sql` | 204 | 3 tabelas + 6 funções de config NL | migrado em T3.2 |
| `sql/70-theodb-ml.sql` | 109 | schema + registry `theodb_ml` | migrado em T3.3 |
| `sql/80-theodb-migrate.sql` | 87 | `theodb.import_vectors_chunked` | migrado em T3.4 |
| `sql/85-theodb-htap.sql` | 148 | superfície HTAP/OLAP | migrado em T3.5 |
| `theodb.control` | 5 | control do umbrella | removido em T3.6 |
| `Makefile` | 39 | build PGXS do umbrella | removido em T3.6 |
| `Dockerfile` | 145 | build da imagem | editado em T1.4 e T3.6 |
| `theodb_rs/src/api.rs` | 700+ | wrappers públicos existentes | referência de padrão; não editado |

Total a remover: **10.283 linhas** de cadeia. Total a mover (não apagar): **471 linhas** de corpo.

### Current callers / dependents

| Chamador | Referência | Efeito da mudança |
|---|---|---|
| `Dockerfile:124` | `CREATE EXTENSION IF NOT EXISTS theodb CASCADE` | passa a `theodb_rs` |
| `Dockerfile:93-113` | build do install + `install` do `theodb.control` | simplificado |
| `packaging/Dockerfile.m51-test:21` | `make install` do umbrella | ajustado ou removido (Q1) |
| `README.md`, `PRD.md:167` | instruem `CREATE EXTENSION` | atualizados |
| `ROADMAP.md` | 3 pontos citam `CREATE EXTENSION theodb` | atualizados |
| `wiki/guides/` | possíveis instruções de instalação | levantado em T3.6 (Q2) |
| Repos irmãos | nenhum | `theo-rag`/`theo-memory` usam pgvector |

### Domain glossary

| Termo | Significado neste plano |
|---|---|
| umbrella | a extensão `theodb`, SQL-only, sem `.so` |
| shim | a extensão `vector`, sem implementação, que só empresta o nome |
| cadeia | os arquivos `X--Y.sql` que migram uma instalação existente |
| greenfield | instalação limpa, `CREATE EXTENSION` numa base sem versão anterior |
| re-emissão convergente | elo que reemite o SQL de instalação inteiro em vez de um delta |
| egress | chamada HTTP de saída feita pelo servidor, na superfície `ai.*` e `theodb.embed` |

### Architecture boundaries affected

```
theodb_rs (.so + SQL)          ← ÚNICA extensão do produto após esta mudança
  ├── schema theodb_rs         objetos internos (#[pg_extern] default)
  ├── schema theodb            superfície pública  ─┐ criados por
  ├── schema ai                superfície pública  ─┤ src/dtype.rs:392
  └── schema theodb_ml         registry (absorvido)─┘ (bootstrap)

vector (control + SQL, sem código)   ← permanece separada: o NOME é o contrato
```

A fronteira que desaparece é a que hoje divide `ai`/`theodb` entre duas extensões. `extension_sql!(sql, name = "...", requires = [...])` é o mecanismo de ordenação do pgrx: `requires` aceita identificador de `#[pg_extern]` ou nome de outro bloco. Medido: **39 blocos em 11 módulos**, 33 nomes registrados, raiz em `"theodb_schema_bootstrap"`.

## Prior Art

- **Interna, e é a que manda:** `theodb_rs/src/api.rs` já é, por declaração própria, "the `extension_sql!` DDL that creates the public `theodb.*` / `ai.*` wrappers". O padrão de destino existe e está em uso. Esta mudança estende um padrão, não inventa um.
- `wiki/decisions/0029-m70-drop-pgvector.md` — fixou a direção da dependência entre umbrella e extensão Rust; nunca considerou o colapso (ver ADR D1).
- `wiki/decisions/0058-pgvector-compat-shim.md` — por que o shim `vector` é separado. Vinculante: o colapso não o toca.
- `.claude/rules/parsimony-ladder.md` — o degrau 1 (precisa existir?) é o que mata a cadeia; o degrau 4 (dependência já instalada?) é o que manda usar `extension_sql!` em vez de inventar mecanismo de ordenação.
- `.claude/rules/architecture.md` — o § 6 nomeia god-module como anti-pattern; é o que decide a granularidade em ADR D4.
- `theodb_rs/sql/schema_snapshot.sql` — o oráculo de membresia e a declaração explícita do seu ponto cego de ACL, que T2.2 fecha.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa | Verificação |
|---|---|---|---|
| G1 | Cadeia `theodb_rs` removida | T1.1 | nenhum `theodb_rs--*--*.sql`; `CREATE EXTENSION theodb_rs` verde |
| G2 | Cadeia do umbrella removida | T1.2 | nenhum `theodb--*--*.sql`; versão do install == `default_version` |
| G3 | Shim reduzido a uma versão | T1.3 | só `vector--0.6.0.sql`; install sem CASCADE verde |
| G4 | Build coerente sem cadeia | T1.4 | `docker build` termina em 0 |
| G5 | Superfície verificada por teste | T2.1 | `#[pg_test]` compara o conjunto de objetos |
| G6 | ACL de egress verificada | T2.2 | `#[pg_test]` assevera ausência de EXECUTE para PUBLIC |
| G7 | Wrappers validados no `CREATE` | T2.3 | `#[pg_test]` assevera `lanname = 'sql'` |
| G8 | `ai.generate/summarize/agg` absorvidos | T3.1 | presentes após `CREATE EXTENSION theodb_rs` sozinho |
| G9 | Superfície NL absorvida | T3.2 | 3 tabelas + 7 funções presentes |
| G10 | Registry `theodb_ml` absorvido | T3.3 | schema + 4 funções presentes |
| G11 | `theodb.import_vectors_chunked` absorvido | T3.4 | `prokind = 'p'` |
| G12 | Superfície HTAP absorvida | T3.5 | 5 funções + tabela presentes |
| G13 | Umbrella deixa de existir | T3.6 | sem `theodb.control`, sem `Makefile`, sem `sql/[0-9]*` |
| G14 | Shim permanece separado | T1.3, T3.6 | `vector.control` intacto, `requires = theodb_rs` |

Cobertura: **14 de 14 afirmações mapeadas (100%)**. Nenhuma tarefa órfã: T1.1–T1.4, T2.1–T2.3, T3.1–T3.6 aparecem todas acima.

## ADRs

### D1 — Colapsar o umbrella em vez de manter duas extensões

**Decisão:** absorver os 22 objetos do `theodb` em blocos `extension_sql!` do `theodb_rs`, e apagar a extensão `theodb`.

**Alternativas consideradas:**

1. *Manter as duas, corrigindo só o que dói* (late-binding, arquivos vazios). **Rejeitada:** não remove a co-propriedade de schema; `DROP EXTENSION theodb` continuaria deixando metade da API `ai` de pé, e cada objeto novo continuaria com dois endereços possíveis sem critério.
2. *Mover o `theodb_rs` para dentro do umbrella.* **Rejeitada** pelo mesmo motivo já registrado no ADR-0029: o I/O do tipo `vector` vive no `.so`, e uma extensão SQL-only não pode provê-lo.
3. *Colapsar também o shim `vector`.* **Rejeitada:** o nome é o contrato — drizzle/alembic/prisma emitem `CREATE EXTENSION vector` literalmente, e o issue #181 mediu app que não subia sem ele (ADR-0058).
4. *Colapsar o umbrella (escolhida).* O `requires` do `extension_sql!` resolve a ordem que hoje força o late-binding; o padrão já está em uso; e a migração que justificava a divisão terminou (zero `plpython3u` ativo).

**Consequência aceita:** quem tiver a extensão `theodb` instalada não terá caminho de upgrade. Medido: não existe instalação — 10 de 10 execuções do workflow de publicação falharam, e a imagem nunca foi publicada.

### D2 — Remover as cadeias de upgrade em vez de reconstruir o gerador

**Decisão:** apagar os 13 arquivos de cadeia; manter apenas o install greenfield de cada extensão.

**Alternativas consideradas:**

1. *Restaurar o gerador e o comparador de superfície a partir de `bcf7819` e seguir mantendo a cadeia.* **Rejeitada:** mantém 10.283 linhas com 86% de duplicação para proteger instalações que não existem, e recria um gate cuja própria disciplina o projeto declarou aplicável a um estado "pré-release sem instalação em campo".
2. *Manter a cadeia colapsada num elo único.* **Rejeitada:** metade do custo, todo o risco — continuaria exigindo gerador, gate e oráculo para um caminho que ninguém percorre.
3. *Apagar (escolhida).* Degrau 1 da parsimony ladder: o artefato não precisa existir.

**Consequência aceita:** a cadeia é append-only; um elo não criado hoje não pode ser criado depois. Se surgir instalação em campo, a primeira versão pós-mudança será a base nova.

### D3 — Verificação como `#[pg_test]`, não como script shell

**Decisão:** o oráculo de superfície e o de ACL nascem como `#[pg_test]` dentro do `theodb_rs`.

**Alternativas consideradas:**

1. *Restaurar os scripts de smoke e de superfície a partir de `bcf7819`.* **Rejeitada:** reintroduz harness externo para testar o que a extensão produz; o teste fica longe do código e exige orquestração de contêiner para rodar.
2. *Não verificar, confiando no build.* **Rejeitada:** é o atalho explicitamente proibido; mudar o contrato de `CREATE EXTENSION` sem oráculo é o risco que o B-029 registra.
3. *`#[pg_test]` (escolhida).* Roda com `cargo pgrx test pg18` (toolchain medido: cargo-pgrx 0.19.0, PostgreSQL 18.4 em `~/.pgrx`), vive junto do código que produz a superfície, e cobre ACL — que o oráculo atual declara não cobrir.

**Ganho não-óbvio:** o teste de ACL fecha um ponto cego que existia mesmo com a cadeia viva.

### D4 — Um módulo Rust por área funcional, não um arquivo único

**Decisão:** as 471 linhas do umbrella vão para `src/ai_text.rs`, `src/nl_config.rs`, `src/ml_registry.rs`, `src/import_vectors.rs` e `src/htap.rs` — um por área, espelhando os corpos-fonte que substituem.

**Alternativas consideradas:**

1. *Tudo em `api.rs`.* **Rejeitada:** `api.rs` já tem 17 blocos; somar 22 objetos faria dele o god-module que `.claude/rules/architecture.md` § 6 nomeia como anti-pattern.
2. *Um módulo `umbrella.rs`.* **Rejeitada:** preserva a fronteira que a mudança existe para apagar; o nome só faria sentido em referência à coisa removida.
3. *Um por área (escolhida).* SRP no nível de módulo — cada um tem uma razão para mudar. É também OCP na prática: superfície nova entra como módulo novo, sem editar lista central, porque o pgrx coleta `extension_sql!` de qualquer módulo.

## Tasks

### T1.1 — Remover a cadeia de upgrade do `theodb_rs`

#### Why this step

9.810 linhas com 86% de duplicação entre si, geradas por um script que não existe mais, verificadas por um gate que não roda. É a maior massa morta do repositório, e removê-la primeiro encolhe todas as tarefas seguintes.

#### TDD

Remover arquivo de upgrade não muda comportamento observável do greenfield, então esta tarefa não tem teste próprio de comportamento — ela tem uma **rede**, que é o T2.1 e precisa estar verde antes e depois.

```
test_extension_surface_contains_public_api   (escrito em T2.1)
  arrange: extensão instalada pelo harness do pg_test
  act:     consultar pg_depend deptype='e'
  assert:  conjunto contém a superfície pública esperada
```

**Ordem imposta: T2.1 antes de T1.1.**

#### Acceptance criteria

- `ls theodb_rs/sql/theodb_rs--*--*.sql` retorna vazio
- `theodb_rs/sql/schema_snapshot.sql` permanece no disco
- `theodb_rs.control` mantém `default_version = '1.5.0'`
- `test_extension_surface_contains_public_api` continua verde

### T1.2 — Remover a cadeia do umbrella e alinhar a versão

#### Why this step

Os 6 deltas duplicam definições do greenfield — o próprio arquivo `theodb--1.5--1.6.sql` declara que "re-aplica em intenção byte-idêntica", sem verificação. E o descompasso entre a versão gerada (1.0) e a declarada (1.6) faz uma instalação limpa executar 7 scripts.

#### TDD

```
test_umbrella_install_version_matches_control
  arrange: ler a versão do install gerado e o default_version do theodb.control
  act:     comparar
  assert:  iguais
  estado hoje: FALHA (1.0 != 1.6)
```

Teste temporário: morre em T3.6 junto com o umbrella.

#### Acceptance criteria

- nenhum `sql/theodb--*--*.sql` no disco
- versão do install gerado igual a `default_version`
- `CREATE EXTENSION theodb` numa base limpa executa exatamente 1 script

### T1.3 — Reduzir o shim `vector` a uma versão

#### Why this step

`vector--0.5.1.sql` e `vector--0.5.1--0.6.0.sql` somam 65 linhas que só servem a quem instalou a 0.5.1. Ninguém instalou. O shim em si permanece — é o único dos três cuja separação tem justificativa externa.

#### TDD

```
test_vector_shim_installs_without_cascade
  arrange: base com theodb_rs instalado, sem a extensão vector
  act:     CREATE EXTENSION vector      (SEM CASCADE — o cenário do issue #181)
  assert:  sucesso; o tipo vector resolve e o alias de AM hnsw existe
```

#### Acceptance criteria

- apenas `sql/vector--0.6.0.sql` permanece
- `vector.control` intacto: `default_version = '0.6.0'`, `requires = 'theodb_rs'`
- `cargo pgrx test pg18 test_vector_shim_installs_without_cascade` termina em exit code 0

### T1.4 — Ajustar o build para o mundo sem cadeia

#### Why this step

O `install` do Dockerfile usa o glob `sql/theodb--*--*.sql`, que passa a não casar nada — `install` falha com "no such file". O build precisa refletir a nova realidade antes de qualquer migração de superfície.

#### TDD

```
test_docker_build_succeeds   (integração, executada de verdade)
  act:    docker build -t theodb:b031 .
  assert: código de saída 0
```

#### Acceptance criteria

- `docker build -t theodb:b031 .` termina em exit code 0
- `docker run --rm -e POSTGRES_PASSWORD=x theodb:b031` produz log de initdb que **não** contains `ERROR`

### T2.1 — Teste do conjunto de objetos da extensão

#### Why this step

É a rede que torna as fases 1 e 3 seguras. Sem ela, remover ou mover objeto é mudança não observada. Substitui, dentro do código, o que o oráculo de superfície fazia por fora.

#### TDD

```
test_extension_surface_contains_public_api
  arrange: extensão instalada
  act:     SELECT pg_describe_object(classid, objid, objsubid) FROM pg_depend
           WHERE deptype = 'e' AND refobjid = 'theodb_rs'::regclass ORDER BY 1
  assert:  o conjunto CONTÉM cada nome da lista esperada
  estado hoje: FALHA para os 22 objetos do umbrella, ainda não migrados
```

#### Acceptance criteria

- `cargo pgrx test pg18 test_extension_surface_contains_public_api` retorna exit code 1 antes da Fase 3
- o mesmo comando retorna exit code 0 ao fim do T3.6, com a lista esperada sem exceções

### T2.2 — Teste da ACL da superfície de egress

#### Why this step

O oráculo de membresia declara: registra membresia, não ACL — um upgrade que perca um `REVOKE ... FROM PUBLIC` passa por ele. Toda a superfície `ai.*` faz HTTP de saída server-side. Perder um `REVOKE` abriria egress para todo papel do banco, e nada hoje detectaria.

#### TDD

```
test_egress_surface_is_revoked_from_public
  arrange: extensão instalada
  act:     SELECT proname, proacl FROM pg_proc WHERE proname IN (<lista de egress>)
  assert:  para cada uma, PUBLIC não tem EXECUTE
  estado hoje: verde; vira RED se qualquer REVOKE se perder na migração
```

#### Acceptance criteria

`cargo pgrx test pg18 test_egress_surface_is_revoked_from_public` termina em exit code 0, cobrindo `ai._chat`, `ai.generate`, `ai.summarize`, `ai.agg_summarize`, `ai.if`, `ai.rank`, `ai.analyze_sentiment`, `ai.generate_batch`, `theodb.embed`, `theodb.embed_batch`.

### T2.3 — Teste de que os wrappers são validados no `CREATE`

#### Why this step

É o ganho central do colapso. Enquanto forem `plpgsql` late-bound, um erro de assinatura em `ai._chat` só aparece em tempo de execução, para o usuário.

#### TDD

```
test_ai_wrappers_are_sql_language
  act:    SELECT l.lanname FROM pg_proc p JOIN pg_language l ON l.oid = p.prolang
          WHERE p.proname IN ('generate', 'summarize')
  assert: 'sql'
  estado hoje: FALHA (retorna 'plpgsql')
```

#### Acceptance criteria

- `cargo pgrx test pg18 test_ai_wrappers_are_sql_language` retorna exit code 1 antes do T3.1
- o mesmo comando retorna exit code 0 depois do T3.1

### T3.1 — Migrar `ai.generate`, `ai.summarize` e `ai.agg_summarize`

#### Why this step

É o caso que carrega o ganho de validação (T2.3) e o maior risco de ACL (T2.2) — cinco `REVOKE` a preservar.

#### TDD

```
test_ai_generate_is_sql_and_revoked
  act:    consultar prolang e proacl das cinco funções
  assert: lanname = 'sql' E PUBLIC sem EXECUTE
  estado hoje: FALHA em lanname
```

#### Acceptance criteria

- as 5 funções e o agregado existem após `CREATE EXTENSION theodb_rs` sozinho
- `requires` referencia o bloco que cria `ai._chat`
- `cargo pgrx test pg18 test_egress_surface_is_revoked_from_public test_ai_wrappers_are_sql_language` termina em exit code 0

### T3.2 — Migrar a superfície de linguagem natural

#### Why this step

186 linhas de código somando os dois arquivos — a maior fatia. Inclui 3 tabelas de catálogo cujo `CREATE` precisa ordenar antes das funções que as leem, e é onde o `requires` mais importa.

#### TDD

```
test_nl_config_tables_and_functions_exist
  assert: as 3 tabelas e as 7 funções presentes; ai.nl_query resolve
  estado hoje: FALHA
```

#### Acceptance criteria

Tabelas e funções presentes após `CREATE EXTENSION theodb_rs` sozinho.

### T3.3 — Migrar o registry `theodb_ml`

#### Why this step

Cria um schema próprio (`theodb_ml`), então a ordem de bootstrap precisa ser explícita via `requires` — é o único do conjunto que traz schema novo.

#### TDD

```
test_theodb_ml_schema_and_registry_exist
  assert: schema theodb_ml, a tabela de registry e as 4 funções presentes
  estado hoje: FALHA
```

#### Acceptance criteria

Schema, tabela e 4 funções presentes.

### T3.4 — Migrar `theodb.import_vectors_chunked`

#### Why this step

É `PROCEDURE`, não `FUNCTION` — a única do conjunto com essa forma, e a que mais facilmente se perde numa migração feita no olho.

#### TDD

```
test_import_vectors_chunked_is_procedure
  act:    SELECT prokind FROM pg_proc WHERE proname = 'import_vectors_chunked'
  assert: 'p'
  estado hoje: FALHA
```

#### Acceptance criteria

Procedimento presente com `prokind = 'p'`.

### T3.5 — Migrar a superfície HTAP

#### Why this step

As duas funções duplicadas entre delta e greenfield vivem aqui; migrá-las encerra a duplicação que o T1.2 removeu de um lado só.

#### TDD

```
test_htap_surface_exists
  assert: as 5 funções e a tabela de registro presentes
  estado hoje: FALHA
```

#### Acceptance criteria

Superfície presente após `CREATE EXTENSION theodb_rs` sozinho.

### T3.6 — Apagar o umbrella

#### Why this step

Fecha o item. Enquanto `theodb.control` existir, um objeto novo tem dois endereços possíveis e nada decide qual.

#### TDD

```
test_single_create_extension_delivers_full_surface     (o T2.1, agora sem exceções)
  act:    CREATE EXTENSION theodb_rs      (sozinho)
  assert: os 22 objetos migrados presentes
  estado: verde apenas aqui
```

#### Acceptance criteria

- `theodb.control`, `Makefile` e `sql/[0-9]*-theodb-*.sql` removidos
- `Dockerfile` passa a criar apenas `theodb_rs` e `vector` no initdb
- `packaging/Dockerfile.m51-test` ajustado ou removido conforme Q1
- README, PRD, ROADMAP e wiki atualizados onde instruem `CREATE EXTENSION theodb`
- CHANGELOG atualizado sob `[Unreleased]`

## Failure scenarios

A superfície migrada faz HTTP de saída server-side, então os cenários de I/O externo importam mesmo sem mudarmos o código de rede.

| Cenário | Comportamento exigido | Onde é provado |
|---|---|---|
| Endpoint de modelo indisponível durante o teste | erro tipado, sem pânico, sem backend derrubado | testes existentes de `chat.rs` e `embed.rs` permanecem verdes |
| `REVOKE` perdido na migração | detecção antes do merge | T2.2 |
| Bloco `extension_sql!` emitido fora de ordem | `CREATE EXTENSION` falha alto no build | T1.4 e T2.1 |
| Objeto do umbrella esquecido na migração | detecção | T2.1 |
| `CREATE EXTENSION vector` sem CASCADE após a mudança | sucesso, como no cenário do issue #181 | T1.3 |
| Tabela de catálogo NL criada depois da função que a lê | `CREATE EXTENSION` falha no build | T3.2 |

## Concurrency tests

**(none — single-threaded.)** A mudança é DDL de instalação e movimentação de definições SQL entre extensões. Não introduz estado compartilhado, thread, nem caminho concorrente novo. Os access methods e o buffer manager, onde a concorrência do produto vive, não são tocados.

## Dependencies

**Nenhuma dependência nova.** Degrau 4 da parsimony ladder: tudo que a mudança precisa já está declarado.

| Dependência | Versão | Já instalada | Papel nesta mudança |
|---|---|---|---|
| `pgrx` | 0.19.0 | sim, em `theodb_rs/Cargo.toml` | `extension_sql!` e `#[pg_test]` |
| `cargo-pgrx` | 0.19.0 | sim, medido | executar `cargo pgrx test pg18` |
| PostgreSQL | 18.4 | sim, em `~/.pgrx/18.4` | alvo do teste |

Sem manifesto novo, não há superfície de CVE nova a auditar.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação |
|---|---|---|---|
| R1 | A cadeia é append-only: o elo não criado hoje não pode ser criado depois; instalação futura em campo ficaria sem caminho de upgrade | alta se a premissa cair | medido que não há imagem publicada — 10 de 10 execuções do publish falharam; a primeira versão pós-mudança vira a base nova |
| R2 | Mover 471 linhas de SQL para string literals Rust troca erro de sintaxe em tempo de arquivo por erro em tempo de build | média | o `cargo build` do pgrx valida a emissão, e o T1.4 exercita `CREATE EXTENSION` de verdade |
| R3 | `requires` mal fiado produz falha de instalação, não de compilação — é a classe de defeito mais provável desta mudança | alta | T2.1 roda o `CREATE EXTENSION` completo; qualquer ordem errada aparece ali |
| R4 | Perder um `REVOKE` abriria egress HTTP para todo papel do banco | máxima | T2.2 existe exclusivamente para isso e é escrito antes de qualquer migração |
| R5 | A suíte `cargo pgrx test` é cara — a imagem de teste medida tem 30,9 GB | baixa | rodar por módulo durante o desenvolvimento; suíte inteira uma vez ao fim |
| R6 | O CI já está vermelho por causa das remoções de `benchmarks/` e `scripts/`; esta mudança não conserta isso e não deve ser confundida com o conserto | média | declarado aqui e no CHANGELOG; o B-029 permanece aberto |

## Unresolved Questions

- Q1: O `packaging/Dockerfile.m51-test` ainda é usado? Ele faz `make install` do umbrella, que deixa de existir. Se estiver morto, o T3.6 deve removê-lo em vez de ajustá-lo — decidir com medição durante o T3.6, não agora.
- Q2: Os guias em `wiki/guides/` instruem `CREATE EXTENSION theodb`? Levantar no T3.6; se sim, atualizar, porque a wiki é a documentação viva do projeto e um guia que instrui um comando que falha é pior que nenhum.
- Q3: O `default_version` do `theodb_rs` deve subir para sinalizar que a superfície absorveu o umbrella? Fica para a fase de release decidir pela regra de derivação de semver; não bloqueia a implementação.
