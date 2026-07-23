# Blueprint: M146 Hardening Remediation — prior art para durabilidade, validação e teste de corrupção

**Slug:** `m146-hardening-remediation`
**Created:** 2026-07-23
**Plan:** `.claude/knowledge-base/discoveries/plans/m146-hardening-remediation-plan.md` (v1.1)
**Questions:** 8/8 `done`, 0 `blocked`
**Verdict:** `SHIPPABLE_WITH_CAVEATS` (89) — `/discover-confidence` 2026-07-23. Único caveat: `soft_floor_citation_density_low`. 4/4 coverage corners populados, 3 ADRs, **0 citações fabricadas** (todas as `.claude/knowledge-base/references/...` resolvem em disco).

## Reference clone provenance (EC-6)

| Field | Value |
|---|---|
| Path | `.claude/knowledge-base/references/postgres/` |
| Branch | `REL_17_STABLE` |
| Tip SHA | `e99fb3262441b47d88632b2857f03083be67f3e3` (2026-07-04) |
| Version | `PACKAGE_VERSION='17.10'` |

**Caveat honesto:** o clone é **PostgreSQL 17.10, não 18**, enquanto o TheoDB compila contra PG 18. É também clone blob-filtered (arqueologia via `git log -S` indisponível). Para Q1 o gap de versão foi fechado buscando `REL_18_STABLE` diretamente e confirmando que `durable_rename` é idêntico em comportamento. **Q3 e Q5 estão citados em números de linha do PG 17** — o código é estável de longa data, mas as linhas devem ser re-verificadas contra PG 18 antes de serem citadas em outro artefato.

## Objective

Decidir, para cada um dos quatro pontos de hardening do M146, **qual implementação de referência copiar** — em vez de inventar um idioma local. O blueprint entrega, por ponto, uma recomendação acionável de ≤5 linhas ancorada em ≥2 fontes primárias verificáveis, mais os ADRs que travam as decisões de modelo (read-path vs offline; stdlib vs dependência; elevel por call-site).

## Context

O `/review-cycle:loop` full-tree do `theodb_rs` surfou quatro pontos de hardening. Três têm resposta canônica no host (PostgreSQL) ou em extensão permissiva madura; implementá-los "do zero" violaria a Regra 9 (`CLAUDE.md § 9`) e a `.claude/rules/parsimony-ladder.md` (rungs 2-4). Este blueprint fixa **qual implementação de referência o M146 copia**.

---

## Coverage Corner 1 — Integration Tests

### Q5 — como o `amcheck` é testado (padrão de teste de corrupção)

O amcheck usa **três mecanismos estruturalmente diferentes** de injeção de corrupção, mais uma camada SQL pura. Ler só `sql/` teria achado apenas a última (foi o checkpoint EC-4 que evitou a conclusão errada).

**Mecanismo 1 — corrupção byte-level offline (o padrão canônico).**
`contrib/amcheck/t/001_verify_heapam.pl`, `sub corrupt_first_page` (`:178-207`). Receita reproduzível:
1. Resolver o path em disco pelo catálogo — `SELECT pg_relation_filepath('$rel')` + `$node->data_dir` (`:78-89`).
2. **Parar o cluster** (`$node->stop`, `:184`) — inegociável: com o postmaster de pé, shared buffers sobrescreveriam a edição. A tabela é criada com `autovacuum_enabled=false` e o cluster com `autovacuum=off` (`:19`, `:103-124`) para nada reescrever a página.
3. Editar o arquivo in-place com I/O cru — `open($fh,'+<')`, `sysseek($fh, 32, 0)`, `syswrite($fh, pack("L*", 0xAAA15550, ...))` (`:186-203`). O offset **32** cai dentro do array de line-pointers (que começa em `SizeOfPageHeaderData` = 24, `src/include/storage/bufpage.h:214`) — uma única escrita dispara cinco branches diferentes do detector, independente de endianness.
4. Reiniciar (`:206`).
5. **Assertar na mensagem específica, não num booleano** — `detects_heap_corruption` (`:209-224`) casa cinco regexes nomeando a invariante violada; o gêmeo negativo `detects_no_corruption` (`:239-247`) exige string vazia.
6. Provar ausência de falso-positivo varrendo a matriz de opções numa tabela limpa (`:255-283`).

**Mecanismo 2 — corrupção lógica por mutação de catálogo (nenhum byte tocado).**
`t/004_verify_nbtree_unique.pl:149-155`: `UPDATE pg_catalog.pg_amproc SET amproc = 'bad_cmp1'::regproc WHERE amproc = 'ok_cmp1'::regproc` — constrói o índice com o comparador correto, depois troca a função de suporte do opclass, tornando a ordem em disco inconsistente com a semântica declarada. Alcança invariantes que um byte-flip não alcança limpo.

**Mecanismo 3 — "corrupção" que é estado intermediário legítimo (teste de falso-positivo).**
`t/005_pitr.pl` — PITR até a LSN exata de um `UNLINK_PAGE` com `recovery_target_inclusive = off`, deixando o índice meio-deletado-mas-válido; assere que `bt_index_parent_check` **passa** emitindo `interrupted page deletion detected` em debug1.

**Camada SQL pura** (`contrib/amcheck/sql/check_heap.sql`) cobre só o que não precisa de corrupção: validação de argumento, faixas de bloco, permissões, `pg_stat_io`.

**Duas lições portáveis, independentes de mecanismo:** (a) assertar na **string diagnóstica específica**, nunca num booleano — um checker que diz "corrupto: sim" sem dizer qual invariante quebrou não é testável assim; (b) **fixar o ambiente** para nada reescrever o estado injetado.

### Q6 — como o paradedb testa paths de erro/corrupção

Distribuição contada (não estimada) em `.claude/knowledge-base/references/paradedb/`:

| Tier | Local | Contagem | Runner |
|---|---|---|---|
| Rust puro (sem PG) | `pg_search/src/**` `#[test]` | 60 | `cargo test -p pg_search` |
| PG in-process | `pg_search/src/**` `#[pg_test]` | 153 | `cargo pgrx test` |
| SQL golden/regression | `pg_search/tests/pg_regress/sql/` | 290 `.sql` | `cargo pgrx regress` |
| Integração out-of-process | `tests/tests/*.rs` | 543 fns | `cargo test -p tests` |

**Achado negativo forte: paradedb usa `#[pg_test(error = "...")]` ZERO vezes.** Usa `#[pg_test]` + `#[should_panic(expected=)]` (14 sites, ex. `pg_search/src/postgres/options.rs:1185-1186`), `std::panic::catch_unwind(...).is_err()` inline quando precisa de várias asserções de erro na mesma fn (`pg_search/src/gucs.rs:906-907`), e out-of-process `result.is_err()` + `assert_eq!` na mensagem inteira (`tests/tests/bm25_search.rs:1398-1401`). Para codecs usa **`proptest`** (`pg_search/src/postgres/types_arrow.rs:639-780`) — gera os blobs corrompidos em vez de escrevê-los à mão.

**Veredito para o TheoDB:** o nosso `#[pg_test(error = "substring")]` (32 usos) é **mais forte** que o `#[should_panic]` do paradedb — assere que a mensagem chegou ao cliente pelo caminho completo pgrx→`ereport`, enquanto `should_panic` só assere que houve panic Rust (não prova conversão para erro SQL limpo). Dois caveats honestos: (1) **nenhum dos dois assere o SQLSTATE** — provar `ERRCODE_DATA_CORRUPTED` exige assertion out-of-process (`psql`), lacuna que `#[pg_test(error=)]` não fecha; (2) um erro esperado por fn — para uma suíte de N mutações, usar `#[test]` Rust puro com `assert!(matches!(..., Err(m) if m.contains(...)))`, que **roda localmente sem símbolos PG**.

---

## Coverage Corner 2 — Dependencies

### Q7 — dependências para durabilidade de arquivo e teste de corrupção

**Para durabilidade: NENHUMA.** A extensão do paradedb não faz I/O de arquivo:
- `grep "fsync|sync_all|sync_data|tempfile|fs2" paradedb/pg_search/src/` → **um único hit, e é comentário**, não chamada (`pg_search/src/postgres/rel.rs:435`, citando o header do PG sobre `XLogIsNeeded()`).
- `grep "std::fs::|File::create|File::open" pg_search/src/` → **zero**.
- `paradedb/pg_search/Cargo.toml` não declara `tempfile`, `fs2`, `fs-err` nem `nix`.
- Toda persistência passa pelo PG: buffer manager (`pg_search/src/postgres/storage/buffer.rs`), rmgr WAL customizado (`storage/custom_rmgr.rs`), `relation_needs_wal()` (`postgres/rel.rs:429`). fsync é **delegado ao checkpointer** — o ponto inteiro de viver dentro de `pg_sys`.

**Para teste de corrupção:** `proptest` 1.11 (`paradedb/pg_search/Cargo.toml:56`, dev-dep `:83`), `rstest` 0.25 (`:88`), `tempfile` 3.27 **apenas no crate de integração out-of-process** (`paradedb/tests/Cargo.toml:44`, usado em `tests/tests/pg_dump.rs:27` como scaffolding de dump — **não** para durabilidade).

**Veredito reuse-before-you-add (parsimony rung 4) para o `parquet.rs`: adicionar NADA.** `grep "sync_all|sync_data|fsync" theodb_rs/src/` retorna **zero hits** hoje. O fix precisa de exatamente duas chamadas `std` (rungs 2-3 resolvem). Caveat honesto que diferencia do paradedb: `parquet.rs` escreve num path arbitrário **fora do datadir do PG**, então **não dá para delegar ao checkpointer** — os `sync_all()` são genuinamente necessários, não opcionais.

---

## Coverage Corner 3 — Tools

### Q8 — ferramentas que injetam falha/corrupção

| Ferramenta | Invocação | O que injeta/prova | Runnable no TheoDB? |
|---|---|---|---|
| `pg_regress` | `REGRESS = check check_btree check_heap` (`contrib/amcheck/Makefile`); `cargo pgrx regress` no paradedb | Sem injeção — diff de golden output | **Sim no droplet** |
| `pg_isolation_regress` | `theodb_rs/isolation/run.sh` | Permutações de concorrência (MVCC) | **Sim — já em uso** |
| TAP / `prove` (`PostgreSQL::Test::Cluster`) | `TAP_TESTS = 1` (`contrib/amcheck/Makefile`) | **O injetor real de corrupção** (mecanismo 1 do Q5) | **Em espírito sim, as-is não** — o módulo Perl vem com a árvore do PG, não com um install pgrx. **A técnica porta direto para bash**, que é o que nossos harnesses já fazem |
| `cargo pgrx test` | `cargo pgrx test` | Asserções in-process | **Não localmente** (símbolos PG); **sim no droplet** |
| `cargo test` (Rust puro) | `cargo test` | Round-trip de codec, `proptest` | **Sim, LOCALMENTE** — único tier executável na box de dev |
| `proptest` | dentro de `#[test]` | Inputs gerados → falhas de round-trip | **Sim, localmente** |
| Harnesses próprios | `theodb_rs/isolation/crash.sh`, `crash_fold.sh`, `crash_unlogged.sh` | Crash SIGABRT real + recovery | **Sim no droplet — já provados (M48/#46/#47)** |

**Conclusão prática:** a única ferramenta que falta é o **injetor byte-level de corrupção**, e é ~10 linhas de bash sobre o scaffolding que já existe (`isolation/crash.sh` já faz initdb → start → `CREATE EXTENSION` → workload).

---

## Coverage Corner 4 — Techniques

### Q1 — o protocolo exato de rename durável

**Lição (1) — a ORDENAÇÃO (é isto que torna o rename durável).** `durable_rename(oldfile, newfile, elevel)` (`postgres/src/backend/storage/file/fd.c:781-854`) emite **cinco alvos de fsync** em ordem estrita:

| # | Syscall | Alvo | Por quê |
|---|---|---|---|
| 1 | `open(oldfile, O_RDWR)` → `fsync` → `close` | **dados do arquivo de origem** | O conteúdo precisa estar durável **antes** de o nome novo ficar visível. `rename()` é atômico só para a entrada de diretório — não diz nada sobre os blocos de dados |
| 2 | `open(newfile)` → `fsync` → `close`, pulado se `ENOENT` | alvo pré-existente, se houver | **Explicitamente NÃO estritamente necessário** — o comentário in-tree (`fd.c:786-792`) diz: *"não é estritamente necessário, mas facilita raciocinar sobre crashes"* |
| 3 | `rename(oldfile, newfile)` | — | A troca atômica da entrada de diretório |
| 4 | `open(newfile, O_RDWR)` → `fsync` → `close` | o arquivo sob o **novo nome** | Re-sincroniza o inode agora alcançável pelo novo nome |
| 5 | `open(parent_dir, O_RDONLY)` → `fsync` → `close` | **o DIRETÓRIO-PAI** | **O load-bearing.** `fsync(2)`: *"Calling fsync() does not necessarily ensure that the entry in the directory containing the file has also reached disk. For that an explicit fsync() on a file descriptor for the directory is also needed."* Sem o passo 5 o **próprio rename** pode se perder |

Dois detalhes que um re-implementador erra: (a) **flags de open diferem por tipo** — `fsync_fname_ext` (`fd.c:3796-3864`) abre `O_RDWR` para arquivo e `O_RDONLY` para diretório, e `pg_fsync` (`fd.c:389-433`) até faz `Assert` disso; (b) **falha de fsync em diretório é seletivamente tolerada** (`fd.c:3822-3825`) — FS que recusa fsync de diretório não é tratado como erro.

**Lição (2) — a SEMÂNTICA DE FALHA (lição SEPARADA — não conflatar).** `durable_rename` **não faz PANIC**: repassa o `elevel` cru do caller aos três helpers (`fd.c:793`, `:847`, `:850` — o argumento é `elevel`, não `data_sync_elevel(elevel)`), faz `ereport(elevel, ...)` e retorna `-1`. Callers usam de `DEBUG1` a `PANIC`. A política de PANIC vive em `data_sync_elevel()` (`fd.c:3918-3939`, com `data_sync_retry = false` por default em `fd.c:162`), aplicada por `fsync_fname` (`fd.c:755-759`) e pelo checkpoint — **não** por `durable_rename`.

**Por que a assimetria é correta, e a lição para o TheoDB:** o risco do fsyncgate é sobre *dados cuja única cópia sobrevivente é o WAL*. Um write-temp-then-rename tem a fonte da verdade ainda disponível → `ERROR` + o caller refaz a sequência é sólido. **O elevel deve ser escolha explícita por call-site**; herdar "ERROR porque o upstream usa ERROR" sem perguntar "o caller consegue refazer isto?" é o bug.

Corroboração externa (R0), mantida distinta da ordenação: fsyncgate 2018 (https://danluu.com/fsyncgate/) — Linux marca páginas sujas como limpas após erro de writeback e reporta o erro uma única vez; retry então "sucede" falsamente → PG adotou PANIC. `fsync(2)` (https://man7.org/linux/man-pages/man2/fsync.2.html) confirma a exigência do fsync de diretório e o fan-out de erro só desde Linux 4.13. USENIX ATC'20, Rebello et al., "Can Applications Recover from fsync Failures?" (https://www.usenix.org/system/files/atc20-rebello.pdf) conclui que **nenhuma** das 5 aplicações estudadas tem estratégia suficiente — o teto honesto: PANIC é a opção menos ruim, não prova de segurança.

### Q2 — validação de índice ANN persistido: read-path vs offline

**(i) READ PATH (pgvector).** Fato estrutural: as referências de vizinho do pgvector são `ItemPointerData` (blkno, offno), não índices densos — o análogo de "índice < n" é "este TID é estruturalmente válido?", e o pgvector **checa, por vizinho, antes de usar**. Nunca desserializa o grafo inteiro: lê uma tupla por hop, então toda checagem é O(1) deliberadamente.

| Site | Valida | Severidade |
|---|---|---|
| `pgvector/src/hnswutils.c:310-311` | magic number da metapage | **`elog(ERROR, "hnsw index is not valid")`** |
| `hnswutils.c:318` | entry point válido → índice vazio, não erro | graceful |
| `hnswutils.c:547` | tag de tipo da tupla | `Assert` (só debug) |
| `hnswutils.c:549-550` | elemento deletado | **`elog(ERROR)`** |
| `hnswutils.c:777-782` | coerência da tupla de vizinho (version/count) | soft — `return false` |
| **`hnswutils.c:809`** | **por vizinho: `if (!ItemPointerIsValid(indextid)) break;`** | **soft — sentinela, O(1) por vizinho** |

Escada de severidade numa linha: **`elog(ERROR)` para "isto não é meu índice / não pode acontecer"; `Assert` para invariante interna já garantida pelo código; skip suave para coerência por-vizinho, porque um scan que perde uma aresta degrada recall — não corrompe resposta.**

**(ii) OFFLINE (amcheck).** `postgres/contrib/amcheck/verify_nbtree.c` é função SQL sob demanda (`bt_index_check`) que varre a estrutura inteira (`bt_check_every_level`), com `ereport(ERROR, errcode(ERRCODE_INDEX_CORRUPTED))` em **53 sites** e `CHECK_FOR_INTERRUPTS()` espalhado (espera-se que rode tempo suficiente para precisar de cancelamento). `verify_heapam.c` não aborta: empurra linhas `(blkno, offnum, attnum, msg)` num tuplestore.

**Por que são modelos diferentes, não versões mais/menos estritas do mesmo:** amcheck valida **invariantes relacionais entre páginas** ("o downlink no pai é lower bound válido do filho", "o left link concorda com o right link") — predicados **não-locais**, cujo custo é O(índice) e cuja soundness exige lock mais forte que um scan. Não cabem no read path. Inversamente, as checagens do read path são **locais e auto-contidas**. **read path = invariantes locais baratas, fail-fast, por acesso; amcheck = invariantes globais caras, sob demanda, nunca no hot path.**

### Q3 — idioma canônico para nome de relação vindo do usuário

| Idioma | Valida | Falha / SQLSTATE | Quando usar |
|---|---|---|---|
| **`$1::regclass`** (`postgres/src/backend/utils/adt/regproc.c:881-917`) | parse completo do identificador, schema-qualificação e **existência** via `search_path` (`RangeVarGetRelid(..., NoLock, true)`). **Não checa privilégio e não pega lock** (`regproc.c:907`) | Não encontrado → `ERROR: relation "x" does not exist`, **42P01**. **Falha fechado, ANTES de montar o SQL** | **O validador correto para nome de relação do usuário** |
| `to_regclass($1)` (`regproc.c:924-937`) | idêntico | Não encontrado → **NULL, sem erro** | Quando se quer *ramificar* na ausência (probe) |
| `quote_ident` / `format('%I')` (`postgres/src/backend/utils/adt/ruleutils.c:12698-12774`; wrapper em `quote.c:24-34`) | **puramente lexical — nunca toca o catálogo.** **Trata a entrada inteira como UM identificador**, então `public.edges` → `"public.edges"` | NULL → 22004. Inexistente → sem erro aqui, adiado para execução → 42P01. Schema-qualificado → **silenciosamente mutilado** | **Nomes de coluna** e identificadores de parte única |
| `format('%s')` (`varlena.c:5921-5923`) | **nada.** Splice de texto cru | Nenhuma — "sucede" no SQL que o atacante escreveu | Só para fragmentos que **você** gerou (ex.: saída de `::regclass::text`) |

Assimetria decisiva: **`%I` é injection-safe mas cego a existência e hostil a schema; `regclass` dá as duas coisas, e sua direção de saída (`regclassout`) re-renderiza o nome a partir do catálogo** — a string que entra no SQL veio do `pg_class`, não do usuário. Round-trip por `regclass` é injection-proof **por construção**.

### Q4 — idioma de erro tipado para dado persistido (pgrx)

paradedb roda um **idioma de duas zonas**, com a fronteira exatamente em *"este dado veio do usuário ou dos nossos bytes nas nossas páginas?"*: **Zona 1 (fronteira usuário/query)** → enums `thiserror` (`paradedb/pg_search/src/query/mod.rs:1666-1704` `QueryError` com 13 variantes; `postgres/types.rs:1259-1310`); **Zona 2 (dado persistido/página)** → `assert!`/`expect`/`panic!` (`postgres/storage/merge.rs:198` `.expect("expected to deserialize valid MergeEntry")`; `storage/metadata.rs:224-244` asserts; `storage/utils.rs:217` `debug_assert!`). É coerente: sob pgrx um panic Rust **é** um ERROR do Postgres (ambos os perfis fixam `panic = "unwind"`, `paradedb/Cargo.toml:18,26`).

**Veredito sobre o TheoDB: alinhado e, num aspecto, mais estrito que o campo.** `theodb_rs/src/pg.rs:8` (`err_input`) passa por `ErrorReport::new(PgSqlErrorCode::...)` fixando SQLSTATE explícito com domínio `"theodb"` — os enums do paradedb **não fixam SQLSTATE algum**. Nosso `from_bytes -> Result<Self, String>` com conversão na fronteira do AM é a mesma arquitetura de duas zonas, e estritamente mais recuperável que o `.expect()` do paradedb na operação idêntica. **A lacuna única:** o site de conversão usa `pg_sys::error!`, que rende **XX000 (internal_error)**; página de índice corrompida merece SQLSTATE de corrupção (precedente amcheck: `ERRCODE_INDEX_CORRUPTED`, 53 sites).

---

## Cross-cutting Comparison

| Dimensão | PostgreSQL upstream | pgvector | paradedb | TheoDB hoje |
|---|---|---|---|---|
| Durabilidade de arquivo | `durable_rename`: 5 fsyncs, diretório-pai incluído | n/a (tudo em página PG) | n/a (delega ao checkpointer) | **`parquet.rs`: 0 fsyncs** ❌ |
| Validação de índice no read path | amcheck é offline; o read path do PG confia em `ReadBuffer` | checagem local O(1) por vizinho | `expect()` em codec | `ivf.rs:416` ✓ / **`hnsw.rs` sem checar vizinho** ❌ |
| SQL dinâmico com relação do usuário | `regclass` (existência, 42P01, fail-closed) | n/a | n/a | `graph.rs` **3× correto** (`:362,:380,:397`) / **`:265` outlier `%s`** ❌ |
| SQLSTATE de corrupção | `ERRCODE_INDEX_CORRUPTED` (53 sites) | `elog(ERROR)` genérico | sem SQLSTATE fixado | **XX000** (via `pg_sys::error!`) ⚠ |
| Teste de corrupção | 3 mecanismos (byte-level TAP, mutação de catálogo, PITR) | — | `proptest` em codec | crash harnesses ✓, **sem injeção de corrupção** ❌ |

---

## Recommendations (≤5 linhas por ponto)

**(a) Durabilidade do export Parquet — `theodb_rs/src/parquet.rs:251`.** Adotar o protocolo do `durable_rename` com **stdlib pura, zero dependência nova** (rungs 2-3): recuperar o `File` do `ArrowWriter` (`into_inner`), `file.sync_all()` **antes** do rename, `std::fs::rename`, e então `File::open(parent_dir)?.sync_all()` — o fsync do diretório é o load-bearing. Elevel: `Err` tipado (a fonte é refazível, `ERROR`+retry é sólido — não PANIC).

**(b) Validação de vizinho — `theodb_rs/src/ann/hnsw.rs:466`.** Adotar o **modelo read-path** (pgvector), não o exaustivo do amcheck: espelhar verbatim o irmão `ivf.rs:416`, logo após o bloco de counts/entry — `if neighbors.iter().flatten().flatten().any(|&nb| nb >= n) { return Err(...) }`. É O(1) por vizinho, uma vez por scan (não por hop, já que desserializamos o blob inteiro), e fecha um **panic atravessando FFI** que o próprio comentário do arquivo (`:462-464`) diz estar guardando. **Não** validar invariantes de grafo (simetria, alcançabilidade) — isso é amcheck-shaped.

**(c) SQL dinâmico — `theodb_rs/src/graph.rs:265`.** Trocar `$3` por `($3)::regclass::text` no `format` — valida existência e falha 42P01 **antes** de montar o SQL, e re-renderiza o nome a partir do catálogo (injection-proof por construção). **Não** usar `%I`: mutilaria `schema.tabela` num único identificador. Isso alinha o scan aos três sites do mesmo arquivo que já usam `::regclass::oid`. **Corrigir também o comentário `:262`**, que hoje afirma segurança que o código não entrega (Regra 3).

**(d) Teste de corrupção — `theodb_rs/src/am/page/ivf.rs` + suíte.** Três tiers: (1) `#[test]` **Rust puro** para o bounds-check de (b) — roda **localmente**, sem símbolos PG; (2) `#[pg_test(error = "...")]` no droplet para o erro tipado de (c) chegar ao cliente; (3) `isolation/corrupt_index.sh` no molde de `crash.sh` + `t/001_verify_heapam.pl:178-207` (parar cluster → `dd` no offset → reiniciar → assertar ERROR limpo, backend vivo) — é o **único** teste que prova que corrupção semântica vira erro SQL em vez de derrubar o backend. Assertar sempre na **string diagnóstica específica**, nunca num booleano.

**(e) Bônus de baixo custo (do Q4).** Adicionar `err_corrupt()` em `pg.rs` usando `ERRCODE_DATA_CORRUPTED` e rotear os sites de erro de desserialização do AM por ele — ~10 linhas, usa maquinaria que `pg.rs` já tem, e segue o precedente do amcheck. Caveat honesto: provar o SQLSTATE exige assertion out-of-process (`psql`), que `#[pg_test(error=)]` não cobre.

---

## ADRs

### D1 — Adotar o modelo read-path (pgvector) e recusar o modelo amcheck para `from_bytes`

**Decisão:** a validação de integridade referencial do índice ANN entra no `from_bytes` como checagem **local, O(1) por vizinho**; invariantes globais de grafo ficam de fora.

**Rationale:** amcheck valida predicados **não-locais** (custo O(índice), lock mais forte, precisa de `CHECK_FOR_INTERRUPTS`); colocá-los no caminho de leitura transformaria uma sonda O(log n) numa verificação O(n). **Alternativas rejeitadas:** (i) validação exaustiva no read path — custo proibitivo, modelo errado; (ii) adiar tudo para uma função `theodb_amcheck()` separada — não resolve o panic no path quente, que é o defeito real; (iii) não validar — mantém o panic atravessando FFI que o comentário do próprio arquivo diz estar guardando. Cita `.claude/rules/error-handling.md` (fail-fast com erro tipado na fronteira) e a Regra 9.

**Consequências:** ganho de robustez com custo ~zero; invariantes de grafo permanecem não-verificadas (aceito, documentado como incerteza residual).

### D2 — Durabilidade com stdlib, sem dependência nova

**Decisão:** implementar o protocolo de rename durável com `std::fs::File::sync_all` (arquivo + diretório-pai), sem adicionar `fs2`/`fs-err`/`tempfile`.

**Rationale:** rungs 2-3 da `.claude/rules/parsimony-ladder.md` resolvem — a stdlib faz, e o idioma do fsync-do-diretório é o mesmo do host (`fd.c:3872-3892`). **Alternativas rejeitadas:** (i) `fs2`/`fs-err` — dependência redundante para zero capacidade nova (rung 4 proíbe); (ii) delegar ao checkpointer como o paradedb — **impossível aqui**, o Parquet é escrito fora do datadir do PG; (iii) manter só o rename atômico — não é durável, é o defeito.

**Consequências:** duas chamadas de syscall a mais por export (latência aceitável num caminho de export, não de query); nenhuma superfície de dependência nova.

### D3 — Elevel/severidade é escolha explícita por call-site

**Decisão:** o erro de fsync no export Parquet é `Err` tipado (→ `ERROR`), **não** PANIC.

**Rationale:** `durable_rename` do upstream repassa o `elevel` do caller e **não** força PANIC (`fd.c:793,:847,:850`); o PANIC vive em `data_sync_elevel` e existe para o caso em que *a única cópia sobrevivente do dado está no WAL*. O export Parquet é write-temp-then-rename com a fonte ainda disponível → `ERROR` + refazer é sólido. **Alternativa rejeitada:** herdar PANIC "porque fsync falhou" — derrubaria o cluster por uma falha de export re-executável.

**Consequências:** falha de export não derruba o cluster; o operador refaz o `htab_refresh`.

---

## Blocked questions

Nenhuma. 8/8 respondidas.

## Uncertainties (honestidade — Regra 3)

- **Severidade de (c):** confirmou-se que `edge_rel` é alcançável pelo usuário e spliced cru, mas **não** se construiu payload executado contra instância viva. A *presença* do defeito é HIGH-confidence; a *severidade* exploratória é MEDIUM. Mitigante já verificado: `graph_build` não é `SECURITY DEFINER` e tem `REVOKE ALL ... FROM PUBLIC`.
- **Linhas do PG 17 vs PG 18:** Q1 foi confirmado idêntico em `REL_18_STABLE`; Q3/Q5 não — re-verificar linhas antes de citar noutro artefato.
- **Runnability do amcheck TAP sob pgrx:** raciocinada a partir dos requisitos do harness e do precedente `theodb_rs/isolation/`, **não verificada empiricamente**. Tratar como recomendação de design a validar.
