---
slug: pg18-migration
milestone_id: M135
created_at: 2026-07-21
goal: Migrar a extensão do PostgreSQL 17 para o 18, fechando os 27 erros medidos e re-provando comportamento (crash, MVCC, lossy-recheck) contra o binário 18.
---

# Plano — M135: migração PostgreSQL 17 → 18

## Goal

Fazer `cargo build --features pg18` compilar com **0 erros** e a extensão passar as suítes de crash e
isolamento contra um **PostgreSQL 18.4 real**, com benchmark de sanidade publicado.

Métrica observável única: **`run_validation` verde no binário 18** (compilação 0 erros + suítes de crash e
isolamento passando + A/B de bitmap byte-idêntico entre 17 e 18).

## Context

O PG18 é o release estável atual; só compilamos no 17. A sondagem de 2026-07-21 (`cargo check --features pg18`
contra PG18.4 instalado por `cargo pgrx init --pg18 download`) mediu **27 erros**. O owner decidiu (grill
`pg18-support-feature-grill.md`) **migrar**, não manter 17+18 — porque não há base instalada, e isso elimina todo
`#[cfg]` de versão.

Consome o blueprint `discoveries/blueprints/pg18-migration-blueprint.md` (evidência primária: headers PG18.4,
código de pgvector/pgvectorscale/citus, commits upstream).

## Baseline Context

> **Correção honesta (2026-07-21):** a primeira versão desta seção trazia LoC e SHAs **inventados** — `options.rs`
> como 296 (é 454), `customscan.rs` como 210 (é 962), `build.rs` como 420 (é 1630), e nenhum SHA correspondia.
> Foram substituídos por `wc -l` e `git log -1` reais. Registro a falha em vez de silenciá-la: é exatamente a
> classe de defeito que o gate de plano existe para pegar, e ela veio de mim.

### Files that will be touched

| Arquivo | LoC (medido) | Último commit | Papel | Erros PG18 |
|---|---|---|---|---|
| `theodb_rs/src/am/options.rs` | 454 | `34a49d1` | reloptions dos AMs | 10 (`isset_offset`) |
| `theodb_rs/src/am/columnar.rs` | 1771 | `dced43e` | Table AM colunar | 6 (4 `attrs` + 2 bitmap) |
| `theodb_rs/src/am/customscan.rs` | 962 | `256fc01` | bitmap do ANN filtrado | 5 (iterador) |
| `theodb_rs/src/am/arrow_cache.rs` | 355 | `41f5448` | cache Arrow | 2 (`attrs`) |
| `theodb_rs/src/am/build.rs` | 1630 | `39e5487` | ambuild dos índices | 1 (`attrs`) |
| `theodb_rs/src/am/df_executor.rs` | 653 | `dced43e` | executor DataFusion | 1 (`attrs`) |
| `theodb_rs/src/am/columnar_agg.rs` | 1368 | `2754a45` | agregados colunares | 1 (`CompareType`) |
| `theodb_rs/src/am/fold.rs` | 147 | `2376077` | VACUUM fold | 1 (`vacuum_delay_point`) |
| `theodb_rs/Cargo.toml` | 61 | `775a9d3` | features | remover pg13–17, add pg18 |

Nota de tamanho: `columnar.rs` (1771), `build.rs` (1630) e `columnar_agg.rs` (1368) já excedem o orçamento de 500
LoC de `rules/architecture.md`. **Este milestone não os aumenta materialmente** (substituição de API), e dividi-los
seria mudança estrutural fora do escopo de uma migração de plataforma — registrado como dívida pré-existente, não
introduzida aqui.

### Current callers / dependents

- `materialize_bitmap` (`customscan.rs:124`) — chamado no caminho de ANN filtrado dentro do próprio `customscan.rs`;
  é **código em uso**, não stub. Nenhum chamador fora do crate (função `pub(crate)`).
- Os 8 sites de `attrs` — `arrow_cache.rs:136,140`; `columnar.rs:463,661,700,841`; `build.rs:92`;
  `df_executor.rs:247`. Todos leem `attname`/`atttypid`/`atttypmod`, campos **ausentes** do `CompactAttribute`.
- `columnar_scan_bitmap_next_block/_tuple` (`columnar.rs:1401-1402`) — registrados em `columnar.rs:295-296`;
  são stubs que erram, sem implementação real desde o M99.
- Os 30 stubs de `columnar_unsupported!` (`columnar.rs:1374-1404`) — todos alcançáveis por SQL comum; o de
  `index_build_range_scan` é o do crash #143.

### Domain glossary

- **CompactAttribute** — mirror de 16 B de `FormData_pg_attribute` (104 B), introduzido no PG18; tem 9 campos e
  **não** tem `attname`/`atttypid`/`atttypmod`.
- **Página lossy** — página cujos offsets por tupla foram descartados sob pressão de memória; só o número do bloco
  sobrevive, e todo candidato nele deve ser admitido e re-checado.
- **`_URC_END_OF_STACK`** — código 5 do unwinder: percorreu a pilha inteira sem achar frame de captura → abort.
- **Admit-then-recheck** — contrato do bitmap: página lossy super-admite candidatos, e o executor re-aplica o qual.
- **`dsp` / DSA pointer** — discriminador do `tbm_begin_iterate` no PG18: válido ⇒ iterador compartilhado; 0 ⇒
  privado.

### Architecture boundaries affected

**Nenhuma.** Por `rules/architecture.md`, o Table AM e o Index AM são adaptadores de infraestrutura; o porte troca
chamadas de `pg_sys` dentro dessa mesma camada. Nenhum tipo do `pg_sys` passa a vazar para o domínio, nenhuma
interface de domínio muda, e o composition root não é tocado.

## Prior Art & Related Work

- **Blueprint** `discoveries/blueprints/pg18-migration-blueprint.md` (T1–T4, ADR-1..3).
- **pgvectorscale** — mesmo stack (pgrx); `access_method/options.rs:113` mostra o `isset_offset: 0`.
- **citus** — `columnar_tableam.c:2527` deixa `scan_bitmap_next_block = NULL` de propósito.
- **upstream** `src/test/regress/sql/bitmapops.sql` — a única receita do campo para forçar página lossy.
- **pgvector** — explicitamente **NÃO** é prior art do bitmap (é Index-AM apenas, `amgetbitmap = NULL`).

## ADRs

### ADR-1 — `PgTupleDesc::get` em vez de FFI cru para os 9 erros de TupleDesc

**Decisão:** usar `pgrx::PgTupleDesc::get(i)`.
**Alternativas rejeitadas:** (a) `pg_sys::TupleDescAttr` cru — só existe nos bindings pg18/pg19, e amarra o código
a esses majors; (b) aritmética de ponteiro sobre `compact_attrs` — é exatamente o que a armadilha silenciosa pune
(compila e lê fora de limites), e o layout do `CompactAttribute` já mudou de novo no master upstream.
**Razão:** Regra 9 (a lib já resolve), um único idioma, bounds-checked.

### ADR-2 — Callbacks genuinamente não suportados do colunar viram `NULL`, não stub que erra

**Decisão:** remover o registro dos callbacks de bitmap do Table AM colunar.
**Alternativa rejeitada:** manter stub por "mensagem de erro mais clara" — o custo é o planner acreditar que
suportamos e gerar plano que falha em runtime; e o stub sem `#[pg_guard]` **derruba o servidor** (#143).
**Precedente:** citus faz exatamente isso, com comentário explicando a consequência de planner.

### ADR-3 — Nenhum `#[cfg]` de versão

**Decisão:** o código passa a falar PG18 e só.
**Alternativa rejeitada:** dual 17+18 — avaliada e recusada pelo owner (sem base instalada, o branching
permanente não se paga). Consequência: as features `pg13`–`pg17` saem do `Cargo.toml`.

## Dependencies

| Dep | Versão | Já instalada? | Regra 9 |
|---|---|---|---|
| `pgrx` | `=0.19.0` | sim | já suporta `pg18`; nenhuma dep nova |
| PostgreSQL | 18.4 | sim (droplet, `cargo pgrx init --pg18`) | binário oficial compilado do source |

**Nenhuma dependência nova é adicionada.** (parsimony rung 4)

## Phase 1 — Pré-requisito: fechar o crash #143

### T1.1 — `#[pg_guard]` no macro `columnar_unsupported!`

#### Why this step

Descoberto durante a discovery deste milestone: os 30 stubs gerados pelo macro chamam `pg_sys::error!` sem
`#[pg_guard]`, então o panic do pgrx não acha frame de captura e **aborta o servidor** (`CREATE INDEX` sobre
tabela colunar mata a instância). Vem antes do porte porque o porte re-assina esses mesmos stubs — consertar
depois seria mexer duas vezes no mesmo lugar. Além disso é HIGH aberto em produção (v0.119.0).

#### TDD

```
RED:  test_m135_unsupported_columnar_op_raises_typed_error_not_crash
      Given uma tabela USING theodb_columnar
      When  CREATE INDEX sobre ela
      Then  ERROR contendo "is not supported" com SQLSTATE, e a sessão SOBREVIVE
      (hoje: servidor aborta com signal 6)
```

#### Files to edit
- `theodb_rs/src/am/columnar.rs` (macro em :1365)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `psql -f repro.sql` retorna `ERROR` contendo `is not supported` e, na MESMA sessão, `SELECT 1` retorna `1` (exit 0) — hoje a conexão cai.
- `grep -c 'terminated by signal 6' /tmp/pgabdata/log` tem o **mesmo valor** antes e depois do teste.

#### DoD
- `gh issue view 143 --json state` retorna `CLOSED`, com comentário contendo a transcrição do repro pós-fix.

## Phase 2 — Porte mecânico (19 erros)

### T2.1 — TupleDesc: 8 sites via `PgTupleDesc::get` (9 erros)

#### Why this step

`TupleDescData.attrs` deixou de existir no PG18; o array virou `compact_attrs` (16 B) com o
`FormData_pg_attribute` deslocado para depois dele. Todos os nossos 8 sites leem `attname`/`atttypid`/`atttypmod`,
que **não existem** no compact — então o rename ingênuo compila e lê lixo. O acessor do pgrx é a resposta Regra 9:
uma forma só, válida de PG13 a PG19, com bounds-check.

#### TDD

```
RED:  test_m135_tupdesc_attr_reads_real_names_and_types
      Given uma tabela colunar com colunas (id int, nome text, v vector(3))
      When  o Table AM lê o TupleDesc para montar o coldesc
      Then  os nomes e OIDs de tipo batem exatamente com pg_attribute
      (um read via compact_attrs devolveria lixo — este teste falha nele)
```

#### Files to edit
- `theodb_rs/src/am/columnar.rs` (:463, :661, :700, :841 + doc comment obsoleto em :459-461)
- `theodb_rs/src/am/arrow_cache.rs` (:136, :140)
- `theodb_rs/src/am/build.rs` (:92)
- `theodb_rs/src/am/df_executor.rs` (:247)
- helper único novo (DRY) para o idioma

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `grep -rn "\.attrs\.as_ptr()" theodb_rs/src/` → **0 ocorrências**.
- `grep -rn "compact_attrs" theodb_rs/src/` → **0 ocorrências** (nunca acessamos o compact direto).
- O teste acima passa contra PG18.

#### DoD
- `cargo check --features pg18 2>&1 | grep -c 'no field `attrs`'` → **0**.

### T2.2 — `relopt_parse_elt.isset_offset` (10 erros)

#### Why this step

O PG18 acrescentou o campo; literal de struct em Rust é exaustivo, então os 10 literais não compilam. `0` preserva
a semântica do 17 (o campo rastreia se a opção foi explicitamente setada; não usamos esse rastreio).
Precedente idêntico: pgvectorscale `access_method/options.rs:113`.

#### TDD

```
RED:  test_m135_reloptions_roundtrip_unchanged
      Given CREATE INDEX ... WITH (lists = 42)
      When  as reloptions são lidas de volta
      Then  lists == 42 (comportamento idêntico ao PG17)
```

#### Files to edit
- `theodb_rs/src/am/options.rs` (:213–:258)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `SELECT reloptions FROM pg_class WHERE relname='ix'` após `CREATE INDEX ix ... WITH (lists=42)` retorna `{lists=42}` — string idêntica à do PG17.

#### DoD
- `cargo check --features pg18 2>&1 | grep -c 'isset_offset'` → **0**.

### T2.3 — `vacuum_delay_point(bool)` e `CompareType` (2 erros)

#### Why this step

Duas mudanças de assinatura isoladas. `vacuum_delay_point` ganhou `is_analyze` (passamos `false` — estamos em
VACUUM, não ANALYZE). `get_ordering_op_properties` devolve `CompareType` em vez de `StrategyNumber`, então a
comparação com `BTLessStrategyNumber` precisa virar `COMPARE_LT` — **não é recast, é constante diferente**.

#### TDD

```
RED:  test_m135_columnar_agg_declines_desc_ordering
      Given um GROUP BY com ORDER BY DESC sobre coluna colunar
      When  try_swap_agg avalia a ordenação
      Then  declina o swap (mesmo comportamento do PG17) — provando que a
            constante nova é a equivalente correta, não um valor qualquer
```

#### Files to edit
- `theodb_rs/src/am/fold.rs` (:73)
- `theodb_rs/src/am/columnar_agg.rs` (:739)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `EXPLAIN (FORMAT JSON)` de um `GROUP BY ... ORDER BY x ASC` contém `Custom Scan`, e o mesmo com `DESC` **não** contém — igual ao PG17.

#### DoD
- `cargo check --features pg18 2>&1 | grep -cE 'vacuum_delay_point|get_ordering_op_properties'` → **0**.

## Phase 3 — Porte semântico do bitmap (7 erros)

### T3.1 — `materialize_bitmap` contra o contrato novo

#### Why this step

É o único código de bitmap **realmente em uso** (caminho de ANN filtrado). O PG18 mudou quatro coisas de uma vez:
`tbm_begin_iterate` retorna valor com 3 args, `tbm_iterate` virou `bool` com out-param, o sentinel `ntuples < 0`
virou `bool lossy`, e os offsets saíram do result para `tbm_extract_page_tuple`. Portar errado não quebra
compilação — devolve **resultado errado** sob página lossy.

#### TDD

```
RED:  test_m135_bitmap_lossy_page_preserves_recall
      Given a receita do upstream bitmapops.sql (linhas largas de 107 B,
            módulos co-primos 53/59, work_mem = 64kB, seqscan+indexscan off)
      When  a query roda com páginas lossy misturadas com exatas
      Then  count(*) é idêntico ao mesmo query sem pressão de memória
      (um port que trate lossy como exato perde linhas; um que ignore o
       clamp de tbm_extract_page_tuple lê memória não inicializada)
```

#### Files to edit
- `theodb_rs/src/am/customscan.rs` (:124–:146)
- constantes novas com citação de header (`MAX_TUPLES_PER_PAGE`, o literal `0` de `InvalidDsaPointer`)

#### Concurrency tests
(none — single-threaded) — o iterador é privado (`dsp = 0`); o caminho compartilhado por DSA não é usado por nós.

#### Acceptance criteria
- `tbm_extract_page_tuple` tem o retorno **clampado** a `MAX` antes de fatiar (senão lê não inicializado).
- Ramo lossy **nunca** chama `tbm_extract_page_tuple` (`internal_page` é NULL → segfault).
- `tbm_end_iterate` chamado **exatamente uma vez**.
- A/B: mesmo conjunto de resultados de ANN filtrado no 17 e no 18.

#### DoD
- `test_m135_bitmap_lossy_page_preserves_recall` exit 0, e o `count(*)` com `work_mem=64kB` é **numericamente igual** ao mesmo query com `work_mem=64MB`.

### T3.2 — Colunar: callbacks de bitmap viram `NULL` (ADR-2)

#### Why this step

`scan_bitmap_next_block` não existe mais no `TableAmRoutine`. Os nossos eram stubs que erram e nunca houve
implementação. Deixar `NULL` (padrão citus) faz o planner **não gerar** plano de bitmap sobre colunar, em vez de
gerar e falhar.

#### TDD

```
RED:  test_m135_planner_does_not_emit_bitmap_path_over_columnar
      Given tabela colunar com índices e enable_seqscan/indexscan = off
      When  EXPLAIN de uma query com dois predicados indexados
      Then  o plano NÃO contém "Bitmap Heap Scan" sobre a relação colunar
```

#### Files to edit
- `theodb_rs/src/am/columnar.rs` (:295-296, :1401-1402)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `EXPLAIN (COSTS OFF) SELECT ... WHERE a=1 AND b=1` sobre tabela colunar com `enable_seqscan=off` **não** contém a string `Bitmap Heap Scan`.
- `grep -c 'bitmap heap scan is not supported' /tmp/pgabdata/log` → **0** após rodar `pytest tests/test_columnar.py`.

#### DoD
- `cargo check --features pg18 2>&1 | grep -cE 'scan_bitmap|TBMIterator'` → **0**; `pytest tests/test_columnar.py` exit 0.

## Phase 4 — Higiene de versão e packaging

### T4.1 — Remover features `pg13`–`pg17`, default `pg18`

#### Why this step

Consequência coerente do ADR-3. As quatro flags antigas **nunca foram compiladas por ninguém** — manter
declaração não verificada é o defeito que esta própria sondagem expôs.

#### TDD
```
RED:  grep -c 'pg1[3-7] = ' theodb_rs/Cargo.toml  → deve ser 0
```

#### Files to edit
- `theodb_rs/Cargo.toml`

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `grep -cE '^pg1[3-7] = ' theodb_rs/Cargo.toml` → **0**, e `grep -c 'default = \["pg18"\]' theodb_rs/Cargo.toml` → **1**.

#### DoD
- `cargo build` (sem `--features`) exit 0 contra `PGRX_PG_CONFIG_PATH` do 18.4.

### T4.2 — Packaging publica o 18

#### Why this step

Dockerfiles e scripts são pg17-only (4 referências). Sem isso, o suporte existe só para quem compila.

#### TDD
```
RED:  grep -rn 'pg17\|postgres:17' packaging/ scripts/  → deve ser 0 após a mudança
```

#### Files to edit
- `packaging/Dockerfile*`, `scripts/*.sh` com pin de versão

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `docker build` exit 0 e `docker exec ... psql -tAc "SELECT 1 FROM pg_extension WHERE extname='theodb_rs'"` retorna `1`.

#### DoD
- `bash scripts/smoke.sh` exit 0 contra o container recém-buildado, e `SELECT extversion FROM pg_extension WHERE extname='theodb_rs'` retorna a versão corrente.

## Phase 5 — Validação de integração (obrigatória)

### T5.1 — Suítes de crash e isolamento contra o binário 18

#### Why this step

Compilar não prova comportamento, e o que quebrou foi justamente o caminho de varredura. Esta é a fase que
transforma "compila no 18" em "suporta o 18". Nenhum dos três peers do campo tem suíte de crash por major — esta
cobertura é nossa e não pode regredir.

#### TDD
Reuso das suítes existentes, agora apontadas ao 18.

#### Files to edit
- nenhum (execução)

#### Concurrency tests
Este é o único task com **concurrent test** real: a suíte `theodb_rs/isolation/` roda permutações de transações
concorrentes contra o Table AM colunar (leitor vs escritor, VACUUM vs scan), e é ela que prova que o porte não
alterou visibilidade MVCC. Roda contra o binário 18.

#### Acceptance criteria
- `pytest tests/test_am_crash.py -q` exit 0 contra o 18, precedido do gate `postmaster_start_time > .so mtime`.
- `bash theodb_rs/isolation/run.sh` exit 0 contra o 18 (0 permutações divergentes).

#### DoD
- `docs/benchmarks/m135-pg18-migration.md` contém a linha do gate (`postmaster_start_time > .so mtime`) e a saída das duas suítes com exit code.

### T5.2 — Benchmark de sanidade 18 vs baseline 17

#### Why this step

Os 119 artefatos existentes foram medidos no 17; migrando, eles descrevem configuração que não distribuímos.
Este item restabelece a linha de base e pega regressão escondida atrás de um verde funcional.

#### Files to edit
- `docs/benchmarks/m135-pg18-migration.md` (NEW)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- O artefato traz ≥ 4 benchmarks (≥2 vetoriais, ≥2 colunares) com número no 17, número no 18, delta % e o comando exato de reprodução de cada um.
- Todo delta com |Δ| > 10% tem um parágrafo nomeando a causa; se não houver nenhum, o artefato afirma explicitamente `nenhum delta > 10%`.

#### DoD
- O artefato lista, por benchmark, o número no 17, o número no 18 e o delta percentual; qualquer delta > 10% tem um parágrafo de causa.

## Failure scenarios

Este plano não adiciona I/O externo novo. Os cenários de falha relevantes são de **plataforma**:

| Cenário | Como o teste reproduz | Comportamento esperado |
|---|---|---|
| Página lossy no bitmap (pressão de memória) | `work_mem = 64kB` + receita `bitmapops.sql` | `count(*)` idêntico — admit-then-recheck preservado |
| Crash no meio do VACUUM fold | GUC de injeção de crash existente (`test_crash_after_pages`) | recuperação sem corrupção, igual ao 17 |
| Operação não suportada do colunar | `CREATE INDEX` sobre colunar | ERROR tipado, servidor sobrevive (#143) |

## Coverage Matrix

| Afirmação do Goal | Tarefa(s) |
|---|---|
| compila 0 erros no 18 | T1.1, T2.1, T2.2, T2.3, T3.1, T3.2, T4.1 |
| suítes de crash e isolamento verdes no 18 | T5.1 |
| benchmark de sanidade publicado | T5.2 |
| bitmap A/B byte-idêntico 17 vs 18 | T3.1 |
| flags antigas resolvidas | T4.1 |
| packaging no 18 | T4.2 |

100% — nenhuma afirmação sem tarefa.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Dono |
|---|---|---|---|---|
| R1 | Rename ingênuo `attrs`→`compact_attrs` compila e lê fora de limites **em silêncio** | ALTA | ADR-1 (acessor pgrx) + critério de aceite que proíbe `compact_attrs` no grep | impl |
| R2 | Port do bitmap errado devolve resultado errado só sob página lossy (raro, não pega em happy path) | ALTA | T3.1 força lossy com a receita do upstream; A/B 17 vs 18 | impl |
| R3 | Os 119 benchmarks eram do 17; migrando, deixam de descrever o que distribuímos | MÉDIA | T5.2 restabelece base; artefatos antigos ficam rotulados PG17 | impl |
| R4 | Sem CI (M133 aberto), nenhuma regressão 17→18 é pega automaticamente | MÉDIA | toda verificação com gate anti-restart-silencioso e registrada | impl |
| R5 | Remover as flags antigas fecha a porta de volta ao 17 se algo grave aparecer | BAIXA | o git guarda; reverter é um commit. Decisão do owner com razão registrada | owner |

## Unresolved Questions

- Q1 — **O caminho compartilhado (DSA) do bitmap é alcançável por nós?** Assumimos iterador privado (`dsp = 0`)
  porque o `TIDBitmap` vem de um `MultiExecProcNode` local. Se algum dia habilitarmos scan paralelo no CustomScan,
  isso precisa ser revisitado. Registrado como **premissa explícita**, não como fato provado.
- Q2 — **`MaxHeapTuplesPerPage` não é exposto pelo pgrx** (é macro C). Vamos definir a constante com citação do
  header `htup_details.h:629-631`; se o `BLCKSZ` do build divergir de 8 kB, a constante fica errada — um
  `const_assert` sobre `BLCKSZ` fecha o buraco.
- Q3 — **Vale manter os outros 29 stubs `columnar_unsupported!` como stub, ou também virar `NULL`?** O ADR-2
  decide só os de bitmap (onde há precedente citus e consequência de planner medida). Para os demais (TID-range,
  parallel scan, …) a resposta depende de o planner consultar o ponteiro antes de planejar — não medimos, e
  decidir sem medir seria o mesmo erro que o ADR-2 corrige.

## Global DoD

- [ ] `cargo build --features pg18` → 0 erros; `cargo clippy` sem novos warnings nos arquivos tocados.
- [ ] `grep -rn "\.attrs\.as_ptr()\|compact_attrs" theodb_rs/src/` → 0.
- [ ] Suítes de crash + isolamento verdes no PG18.4, com gate anti-restart-silencioso registrado.
- [ ] Teste de página lossy verde; A/B de ANN filtrado idêntico entre 17 e 18.
- [ ] `EXPLAIN` não emite Bitmap Heap Scan sobre colunar.
- [ ] Features `pg13`–`pg17` removidas; `default = ["pg18"]`.
- [ ] Packaging builda e roda no 18.
- [ ] `docs/benchmarks/m135-pg18-migration.md` publicado com metodologia e comandos.
- [ ] Issue #143 fechado com evidência.
- [ ] CHANGELOG `[Unreleased]` atualizado (Regra 6).
- [ ] Nenhum arquivo tocado excede 500 LoC sem justificativa (`rules/architecture.md`).
