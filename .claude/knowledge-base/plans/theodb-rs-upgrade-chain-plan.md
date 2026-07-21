---
slug: theodb-rs-upgrade-chain
milestone_id: M137
created_at: 2026-07-21
goal: Tornar o `theodb_rs` atualizável via `ALTER EXTENSION UPDATE`, provado por um oráculo que compara o schema pós-upgrade com o de uma instalação limpa.
---

# Plano — M137: cadeia de upgrade do `theodb_rs`

## Goal

Fazer `ALTER EXTENSION theodb_rs UPDATE` funcionar a partir de qualquer instalação rotulada `1.0.0`, com prova
mecânica de que o resultado é **idêntico** a uma instalação limpa.

Métrica observável única: **`diff` vazio** entre o snapshot de `pg_depend` do banco atualizado e o de um banco
com `CREATE EXTENSION` fresco.

## Context

Medido em 2026-07-21: `theodb_rs` expõe 94 `pg_extern` e tem **zero** scripts de upgrade, travado em
`default_version = '1.0.0'` através de 120 releases. Consome o blueprint
`discoveries/blueprints/pgrx-upgrade-chain-blueprint.md`, cujo achado T4 inverte a recomendação do campo: a
superfície foi **0 → 25 → 57 → 71 → 94** `pg_extern` com a versão congelada, então **`1.0.0` rotula pelo menos
cinco catálogos diferentes** e o primeiro salto precisa ser convergente, não delta.

## Baseline Context

### Files that will be touched

| Arquivo | LoC (medido) | Papel |
|---|---|---|
| `theodb_rs/theodb_rs.control` | 6 | `default_version` (hoje `'1.0.0'`) |
| `theodb_rs/sql/` | — | **NEW** — diretório que o pgrx copia no `install`/`package` |
| `theodb_rs/sql/schema_snapshot.sql` | — | **NEW** — o oráculo |
| `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql` | — | **NEW** — o salto convergente |
| `scripts/test-upgrade.sh` | — | **NEW** — harness (roda na droplet, como as suítes de crash) |
| `CHANGELOG.md` | 2400+ | Regra 6 |

### Current callers / dependents

- `theodb_rs.control` é lido pelo Postgres no `CREATE EXTENSION` / `ALTER EXTENSION`; nenhum código Rust o lê.
- Os 94 `#[pg_extern]` espalhados por `theodb_rs/src/` produzem a superfície; `api.rs` (957 LoC) concentra a
  maior parte das declarações SQL explícitas.
- Objetos **sem** `CREATE OR REPLACE` que possuímos e que o script convergente precisa guardar (medido):
  4 access methods (`theodb_ivfflat`, `theodb_hnsw`, `theodb_symqg`, `theodb_columnar`) e ≥ 8 operator classes
  (`theodb_hnsw_{cosine,ip,l2}_ops`, `theodb_ivfflat_{cosine,ip,l2,label}_ops`, `theodb_symqg_l2_ops`).
- A extensão umbrella `theodb` já tem cadeia 1.0→1.4 e **não** é tocada por este milestone.

### Domain glossary

- **Salto convergente** — script de upgrade total e idempotente que leva *qualquer* catálogo de origem ao estado
  alvo, em vez de assumir um ponto de partida conhecido.
- **Oráculo de schema** — `pg_depend` + `pg_describe_object()` ordenado: identificadores qualificados e sem OID,
  comparáveis entre bancos.
- **Cenário B1** — `.so` novo carregado contra catálogo antigo **sem** rodar `ALTER EXTENSION` (o usuário que
  faz `apt upgrade` e esquece).
- **Imposto do `.so` versionado** — quando `module_pathname` é omitido, cada função aponta para um `.so` com a
  versão no nome, o que obriga a re-emitir tudo em todo upgrade. **Nós não pagamos** — nosso control fixa
  `module_pathname = '$libdir/theodb_rs'`.

### Architecture boundaries affected

**Nenhuma.** É superfície de empacotamento/catálogo. Nenhum código Rust muda de camada; nenhum tipo do `pg_sys`
passa a vazar. Por `rules/architecture.md`, o `.control` e o `sql/` são artefatos de distribuição.

## Prior Art & Related Work

- **Blueprint** `discoveries/blueprints/pgrx-upgrade-chain-blueprint.md` (T1–T6, ADR-1..2).
- **pgvectorscale** — modelo de script total idempotente com guardas `DO $$ IF NOT EXISTS ... pg_am/pg_opclass`.
- **ParadeDB `pg_search`** — cadeia de deltas + `check_migration_diff.py` (o gate de autoria).
- **pg_durable** `scripts/test-upgrade.sh` — os três cenários, incluindo o B1.
- **In-repo:** `sql/theodb--1.0--1.1.sql` … `--1.3--1.4.sql` já estabelecem a convenção de delta aqui.

## ADRs

### ADR-1 — Primeiro salto convergente; deltas do 1.1.0 em diante

**Decisão:** `1.0.0--1.1.0` é total e idempotente; a partir de `1.1.0`, deltas.
**Alternativas rejeitadas:** (a) deltas desde o início (recomendação inicial da pesquisa) — **incorreta**, porque
a medição mostrou origem ambígua; (b) fan-out N×N permanente estilo pgvectorscale — rejeitada: 8,5k linhas
duplicadas, crescimento quadrático, e não pagamos o imposto do `.so` versionado que os obriga a isso.
**Razão:** convergência só onde a incerteza existe (parsimony rung 1).

### ADR-2 — Baseline honesto, sem caminho retroativo fabricado

**Decisão:** o script converge qualquer catálogo rotulado `1.0.0`; instalações cujo catálogo divergiu de formas
que o script não cobre são declaradas sem caminho, na doc de migração.
**Alternativa rejeitada:** afirmar cobertura total — seria alegação não verificável, já que não temos registro
de qual superfície cada release entregou.

## Dependencies

| Dep | Versão | Já instalada? | Regra 9 |
|---|---|---|---|
| pgrx | `=0.19.0` | sim | copia `sql/*--*--*.sql` automaticamente; nada a adicionar |
| PostgreSQL | 18.4 | sim | `pg_depend`/`pg_describe_object` são nativos — o oráculo não precisa de ferramenta |

**Nenhuma dependência nova.** (parsimony rung 3 — o oráculo é feature nativa do Postgres)

## Phase 1 — O oráculo e o escopo real

### T1.1 — `schema_snapshot.sql`

#### Why this step

Sem oráculo não há como provar nada do resto, e ele é a coisa mais barata do plano (~8 linhas de SQL, sem
ferramenta). Vem primeiro porque todos os outros critérios de aceite o consomem. `pg_describe_object` devolve
identificador qualificado e **sem OID**, então a saída é comparável entre bancos diferentes; `ORDER BY 1` mata a
instabilidade de ordem que torna `diff(1)` inútil sobre schema gerado pelo pgrx.

#### TDD

```
RED: test_m137_snapshot_is_stable_and_oid_free
     Given dois bancos com CREATE EXTENSION theodb_rs fresco
     When  o snapshot roda nos dois
     Then  a saída é byte-idêntica (prova que não vaza OID nem ordem instável)
```

#### Files to edit
- `theodb_rs/sql/schema_snapshot.sql` (NEW)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `diff <(snapshot db_a) <(snapshot db_b)` sobre dois bancos frescos → **saída vazia, exit 0**.
- `psql -f schema_snapshot.sql | wc -l` → **≥ 94**, e `psql -f schema_snapshot.sql | grep -cE '[0-9]{4,}'` → **0** (nenhum OID cru).

#### DoD
- Duas execuções consecutivas do snapshot no MESMO banco produzem `md5sum` idêntico.

### T1.2 — Enumerar o que um catálogo `1.0.0` pode conter

#### Why this step

O script convergente precisa saber o que pode existir a mais (objeto removido depois) além do que falta. Tentei
extrair isso por regex sobre tags e o resultado foi **inconclusivo** (0 nomes históricos — o regex só casa o
estilo atual do `api.rs`), então registro como medição a fazer, não como "não há removidos". A forma correta é
gerar o schema nas tags históricas e comparar conjuntos.

#### TDD

```
RED: test_m137_removed_objects_enumerated
     Given os schemas gerados em v0.30.0, v0.60.0, v0.90.0, v0.110.0 e HEAD
     When  os conjuntos de objetos são comparados
     Then  a lista de "existiu e não existe mais" é produzida e versionada
     (hoje: desconhecida — a extração por regex falhou)
```

#### Files to edit
- `docs/benchmarks/m137-upgrade-chain.md` (NEW — a tabela medida)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `docs/benchmarks/m137-upgrade-chain.md` contém uma tabela com ≥ 4 tags amostradas, cada linha com contagem de objetos, e uma seção `## Removidos` listando os nomes (ou a frase literal `nenhum objeto removido — medido`).
- Para cada nome na seção `## Removidos`, `grep -c "DROP.*IF EXISTS.*<nome>" theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql` → **≥ 1**.

#### DoD
- O artefato contém a linha `Reproduzir:` seguida do comando exato que gera a tabela.

## Phase 2 — O salto convergente

### T2.1 — `theodb_rs--1.0.0--1.1.0.sql`

#### Why this step

É o coração do milestone. Precisa levar **qualquer** catálogo rotulado `1.0.0` ao estado `1.1.0`, o que exige
idempotência real: `CREATE OR REPLACE` onde existe, guarda de existência onde não existe (`TYPE`,
`ACCESS METHOD`, `OPERATOR`, `OPERATOR CLASS`, `CAST` dão `42710`), e `DROP ... IF EXISTS` para o que foi
removido. E `CREATE OR REPLACE FUNCTION` **preserva** owner e ACL, enquanto `DROP`+`CREATE` **perde** — então
qualquer troca de assinatura precisa re-emitir o `REVOKE ... FROM PUBLIC` no mesmo arquivo.

#### TDD

```
RED: test_m137_convergent_upgrade_matches_fresh_install
     Given um banco com o catálogo de uma release antiga rotulado 1.0.0
     When  ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'
     Then  o snapshot bate byte-a-byte com o de um CREATE EXTENSION fresco
     (hoje: não existe caminho de update — o comando erra)
```

#### Files to edit
- `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql` (NEW)
- `theodb_rs/theodb_rs.control` (`default_version` → `'1.1.0'`)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- O arquivo começa com `\echo Use "ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'" to load this file. \quit`.
- Os 4 access methods e as ≥ 8 operator classes aparecem sob guarda de existência (`pg_am` / `pg_opclass`),
  não como `CREATE` cru — rodar o script duas vezes seguidas **não** produz `42710`.
- `psql -tAc "SELECT default_version FROM pg_available_extensions WHERE name='theodb_rs'"` → `1.1.0`.
- Rodar o script **duas vezes** no mesmo banco termina com exit 0 e snapshot inalterado (idempotência real).

#### DoD
- `psql -tAc "SELECT extversion FROM pg_extension WHERE extname='theodb_rs'"` após o UPDATE → `1.1.0`.

## Phase 3 — As provas

### T3.1 — Cenário A: pós-upgrade == instalação limpa

#### Why this step

É a garantia que o usuário compra. O perigo desta classe não é o script faltando — caminho ausente é **erro
alto** (`extension.c:1415`) — é o script **presente e incompleto**, que sobe sem erro e deixa o banco
estruturalmente diferente, silenciosamente.

#### TDD

```
RED: test_m137_scenario_a_upgraded_equals_fresh
     Given banco A upgradado de 1.0.0 e banco B com CREATE EXTENSION fresco
     When  os dois snapshots são comparados
     Then  diff vazio
```

#### Files to edit
- `scripts/test-upgrade.sh` (NEW)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `bash scripts/test-upgrade.sh; echo $?` → **0**, e a saída contém a linha literal `SCENARIO_A_OK` seguida de um bloco `diff` vazio.
- Com uma linha removida do script de upgrade, `bash scripts/test-upgrade.sh; echo $?` → **não-zero**, e a saída nomeia o objeto ausente.

#### DoD
- O log do harness contém a linha do gate (`postmaster_start_time > .so mtime`) antes de qualquer asserção.

### T3.2 — Cenário B1: `.so` novo contra catálogo antigo

#### Why this step

O usuário que faz `apt upgrade` e esquece o `ALTER EXTENSION` carrega o `.so` novo contra o catálogo velho. Para
a maioria das extensões isso é um erro de função ausente; **para nós é potencialmente um crash**, porque nossos
index AMs leem páginas em disco e o `theodb_columnar` é um TableAM. Divergência de assinatura ali não é
mensagem de erro — é o que produziu o #143.

#### TDD

```
RED: test_m137_scenario_b1_old_catalog_new_so_degrades_safely
     Given catálogo 1.0.0 e o .so de 1.1.0 carregado, SEM ALTER EXTENSION
     When  uma query toca um índice theodb_hnsw e uma tabela theodb_columnar
     Then  ou funciona, ou dá erro tipado — NUNCA derruba o servidor
```

#### Files to edit
- `scripts/test-upgrade.sh` (cenário B1)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `grep -c "terminated by signal" no log do PG` tem o **mesmo valor** antes e depois do cenário B1.
- Toda falha do cenário B1 aparece no artefato com o SQLSTATE de 5 caracteres; se não houver falha, o artefato diz `B1 sem falha observada`.

#### DoD
- A saída contém a linha literal `SCENARIO_B1_DONE` e o artefato tem uma seção `## Cenário B1` não vazia.

## Phase 4 — Validação de integração

### T4.1 — Artefato de evidência

#### Why this step

Fecha o ciclo com o que o projeto exige: nenhuma afirmação sem artefato reproduzível.

#### Files to edit
- `docs/benchmarks/m137-upgrade-chain.md`

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `grep -cE 'SCENARIO_A_OK|SCENARIO_B1_DONE|GATE ok|Reproduzir:' docs/benchmarks/m137-upgrade-chain.md` → **≥ 4**.
- O artefato tem uma seção `## Limites honestos` com ≥ 2 itens, incluindo a ausência de cobertura de ACL.

#### DoD
- `test -f docs/benchmarks/m137-upgrade-chain.md` exit 0 e `git diff --name-only HEAD~1 | grep -c CHANGELOG.md` → **1**.

## Failure scenarios

| Cenário | Como o teste reproduz | Comportamento esperado |
|---|---|---|
| Script de upgrade ausente | `ALTER EXTENSION` para versão sem caminho | `ERRCODE_INVALID_PARAMETER_VALUE`, erro alto (`extension.c:1415`) |
| Script incompleto | remover uma linha e rodar o cenário A | harness **falha** com o objeto faltante nomeado |
| Script rodado duas vezes | executar o mesmo update em sequência | exit 0, sem `42710`, snapshot inalterado |
| `.so` novo, catálogo velho | cenário B1 | erro tipado ou sucesso — **nunca** signal |

## Coverage Matrix

| Afirmação do Goal | Tarefa(s) |
|---|---|
| `ALTER EXTENSION UPDATE` funciona a partir de 1.0.0 | T2.1 |
| resultado idêntico a instalação limpa | T1.1, T3.1 |
| origem ambígua tratada (convergência) | T1.2, T2.1 |
| não derruba quem esquece o UPDATE | T3.2 |
| evidência reproduzível | T4.1 |

100% — nenhuma afirmação sem tarefa.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Dono |
|---|---|---|---|---|
| R1 | O script convergente não cobre um catálogo `1.0.0` que divergiu de forma não prevista — upgrade "sucede" e deixa o banco errado | ALTA | oráculo de snapshot é critério de aceite, e o teste de falha injetada prova poder de detecção | impl |
| R2 | `DROP`+`CREATE` numa troca de assinatura perde ACL (`REVOKE ... FROM PUBLIC`) silenciosamente | ALTA | regra de autoria no plano + o snapshot **não** cobre ACL — declarado em Unresolved Questions | impl |
| R3 | Guarda de existência escrita errada num ACCESS METHOD deixa o upgrade quebrado só para quem já tinha o AM | MÉDIA | critério de idempotência (rodar duas vezes) pega exatamente isso | impl |
| R4 | Sem CI (M133/M136 abertos), o harness roda só na droplet — nenhuma regressão futura é pega automaticamente | MÉDIA | o harness é script, não workflow: quando o CI voltar, é uma linha de wiring | owner |

## Unresolved Questions

- Q1 — **ACL não entra no oráculo.** `pg_depend` registra membros da extensão, não `proacl`. Um upgrade que
  perca um `REVOKE ... FROM PUBLIC` passa no cenário A. Fechar isso exige snapshot separado de `proacl`;
  registrado como limite conhecido deste milestone, não resolvido.
- Q2 — **Quais objetos foram removidos entre releases** segue desconhecido: a extração por regex falhou
  (T1.2). Até a medição rodar, o `DROP ... IF EXISTS` do script cobre um conjunto possivelmente incompleto.
- Q3 — **O cenário B1 pode revelar um crash** em vez de erro tipado. Se revelar, o resultado honesto é filar
  issue e declarar a limitação, não silenciar — o escopo deste milestone é a cadeia, não consertar o AM.

## Global DoD

- [ ] `ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'` funciona contra PG18.4 real.
- [ ] `diff` vazio entre snapshot pós-upgrade e snapshot de instalação limpa.
- [ ] Script idempotente: rodar duas vezes não erra e não muda o snapshot.
- [ ] Falha injetada faz o harness falhar (o teste tem poder de detecção).
- [ ] Cenário B1 executado, com resultado documentado seja qual for.
- [ ] `docs/benchmarks/m137-upgrade-chain.md` publicado com comandos de reprodução.
- [ ] CHANGELOG `[Unreleased]` atualizado (Regra 6).
- [ ] Nenhum arquivo tocado excede 500 LoC sem justificativa (`rules/architecture.md`).
