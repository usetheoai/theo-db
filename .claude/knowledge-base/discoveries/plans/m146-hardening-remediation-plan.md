# Discovery Plan: M146 Hardening Remediation — prior art para durabilidade, validação e teste de corrupção

> **Version 1.1** — Esta descoberta investiga o prior art canônico para os quatro pontos de hardening que o M146 vai corrigir no `theodb_rs`: (a) o protocolo de rename durável do PostgreSQL upstream, (b) validação de integridade referencial no read path de índices ANN persistidos, (c) o idioma canônico para resolver nome de relação vindo do usuário em SQL dinâmico, e (d) a estratégia de teste de paths de corrupção em codecs de página. Projetos em escopo: `postgres` (upstream), `pgvector`, `paradedb`. O blueprint resultante deve entregar, por ponto, uma recomendação acionável de ≤5 linhas ancorada em ≥2 fontes primárias. (v1.0 → v1.1: absorvidos os MUST-FIX EC-1 e EC-2 e os checkpoints EC-3/4/5/6 do `/discover-edge-cases`.)

**Slug:** `m146-hardening-remediation`
**Owner:** TheoDB
**Created:** 2026-07-23
**Time budget:** 3h (postgres 1.5h, pgvector 0.75h, paradedb 0.75h — ver D1)

## Context

O `/review-cycle:loop` full-tree do `theodb_rs` (núcleo: 12 arquivos mais críticos × 10/10 pilares, 32 findings, precision 1.00, 0 blockers — relatório em `knowledge-base/review-archive/theodb-full-core-complete-2026-07-23/review-2026-07-23.md`) surfou quatro classes de ponto acionável que o M146 corrige. Três deles têm resposta canônica no PostgreSQL upstream ou em extensões permissivas maduras; implementá-los "do zero" violaria a Regra 9 (`CLAUDE.md § 9`, Não Reinvente) e a `.claude/rules/parsimony-ladder.md` (rungs 2-4: stdlib → nativo → dep já instalada, antes de código próprio).

Evidência que dispara a descoberta:

- **(a)** `theodb_rs/src/parquet.rs:263` `atomic_write_parquet` faz temp+rename (atômico) mas **sem `fsync`** → o export não é crash-durável. O upstream resolve com `durable_rename()`.
- **(b)** `theodb_rs/src/ann/hnsw.rs:466` `from_bytes` valida counts e entry mas **não os índices de vizinho**, enquanto o irmão `ann/ivf.rs:416` valida (`node >= n`). Issue interna do review.
- **(c)** `theodb_rs/src/graph.rs:265` usa `format('%s', $3)` cru para `edge_rel` (issue #168).
- **(d)** `theodb_rs/src/am/page/ivf.rs` (1209 LoC) não tem `mod tests` in-file; os paths de erro tipado por corrupção são inalcançáveis pelos testes de integração SQL.

## Objective

Decidir, para cada um dos quatro pontos, **qual implementação de referência o M146 deve copiar** — em vez de inventar um idioma local.

Critérios de sucesso mensuráveis:

- [ ] All research questions in this plan answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project
- [ ] Recommendations section provides at least one concrete decision proposal per in-scope research question (≤5 linhas, acionável pelo M146)
- [ ] Cada questão `techniques` com ≥2 fontes primárias independentes (R2 do `.claude/rules/discover-phd-rigor.md`), sendo ≥1 web em Q1 e Q3 (R0)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/postgres/` | `src/backend/storage/file/fd.c`, `src/backend/utils/adt/regproc.c`, `src/backend/utils/adt/quote.c`, `src/backend/utils/adt/ruleutils.c`, `contrib/amcheck/` | É o HOST do TheoDB (não-fork). Copiar o idioma do host é a máxima aderência possível (compatibilidade + zero surpresa para DBA) |
| `.claude/knowledge-base/references/pgvector/` | `src/hnswutils.c`, `src/hnswscan.c` | Extensão PG permissiva com índice ANN persistido — o análogo mais direto do nosso read path |
| `.claude/knowledge-base/references/paradedb/` | `pg_search/src/`, `pg_search/tests/`, `tests/`, `Cargo.toml`, `Makefile` | Extensão **pgrx** madura (mesma stack Rust) — prior art de erro tipado e de teste em `#[pg_test]` |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `postgres/src/backend/{optimizer,executor,replication}/` | Irrelevantes aos 4 pontos; leitura custaria o budget inteiro |
| `pgvector/` fora do read path de elemento/vizinho HNSW (halfvec, sparsevec, kernels SIMD, build/CI) | Stop criterion de D3 (EC-5) — evita scope creep |
| `paradedb/` motor BM25, UI, docs | Já coberto por M139/M140; aqui só interessa o idioma de erro e de teste |
| `.claude/knowledge-base/references/{duckdb,hydra,citus,lance,...}/` | Motores columnar/formatos externos não acrescentam prior art para os 4 pontos (ver D4) |
| Qualquer projeto NÃO clonado em `.claude/knowledge-base/references/` | Cross-Project Rule: nunca afirmar feature de um projeto sem ler sua fonte |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** postgres 1.5h, pgvector 0.75h, paradedb 0.75h (total 3h).

**Rationale:** O postgres responde sozinho 3 dos 4 pontos (a, c, d-parcial) e é o host — merece a maior fatia. pgvector e paradedb são confirmações laterais (o análogo ANN e o análogo pgrx). **Alternativas consideradas:** divisão igual (desperdiça budget em refs secundárias); deep-dive só no postgres (perde o idioma pgrx de teste, que é exatamente o que o ponto (d) precisa); sem budget (halt-loop não converge).

**Stop condition — per question (mandatory):** quando a busca de uma questão retorna vazio após 3 tentativas com variantes de query diferentes, marcar a questão BLOCKED com razão "busca exaurida" e seguir. NUNCA preencher com achado de outro escopo.

**Stop condition — per project (mandatory):** com o budget do projeto exaurido e questões pendentes, marcar as restantes daquele projeto BLOCKED com razão "budget exhausted" e seguir para o próximo. Se todo o restante estiver `done` ou honestamente `blocked`, emitir `<promise>BLUEPRINT_BLOCKED</promise>` (NUNCA `BLUEPRINT_COMPLETE`) com o relatório de bloqueio.

**Anti-pattern:** NUNCA fabricar resposta para fechar questão cuja busca foi exaurida. BLOCKED honesto com razão é obrigatório (Regra 3).

**Consequences:** o blueprint pode sair com questões BLOCKED explícitas, que viram semente da próxima descoberta.

### D2 — Priorizar o upstream PostgreSQL como fonte primária

**Decision:** Para os pontos (a), (c) e (d), a fonte primária é o código do PostgreSQL upstream; pgvector/paradedb entram como confirmação secundária.

**Rationale:** O TheoDB é PostgreSQL 18 upstream **sem fork** + extensão. O idioma do host é o que o DBA já conhece, o que o `psql`/tooling espera, e o que tem 25 anos de casos de borda resolvidos (fsyncgate 2018). Isso é aplicação direta da Regra 9 (`CLAUDE.md § 9`) e do rung 3 da `.claude/rules/parsimony-ladder.md` (feature nativa da plataforma antes de código próprio). **Alternativas consideradas:** derivar o protocolo de primeiros princípios (reinventa roda com casos de borda conhecidos); copiar de um motor não-PG (idioma estranho ao host, quebra a expectativa do operador).

**Consequences:** o blueprint recomenda idiomas que já existem no host; o custo é ficar preso à semântica do PG (aceitável — é o host por decisão de arquitetura).

### D3 — Desdobrar a questão de validação em read-path vs offline (absorve EC-2)

**Decision:** A questão sobre validação de índice (Q2) é investigada em duas metades explícitas: (i) o que se valida **no read path quente** (pgvector) e (ii) o que se valida **offline sob demanda** (`amcheck`). A resposta DEVE declarar qual modelo o M146 adota.

**Rationale:** `amcheck` é uma função SQL invocada sob demanda que varre a estrutura inteira; o `from_bytes` do M146 valida no caminho de leitura quente. Recomendar "faça como o amcheck" levaria a validação exaustiva no hot path (custo proibitivo) ou a adiar a validação para função separada (não resolve o OOB). **Alternativas consideradas:** tratar os dois como o mesmo problema (produz recomendação inaplicável — foi o MUST-FIX EC-2); investigar só pgvector (perde o vocabulário de corrupção do amcheck, útil para o ponto (d)).

**Consequences:** duas listas separadas no blueprint; o custo é uma questão mais longa, pago pelo budget de techniques (D1).

### D4 — Não investigar motores columnar/formatos externos neste ciclo

**Decision:** `duckdb`, `hydra`, `citus`, `lance` ficam fora.

**Rationale:** Os quatro pontos são sobre o host PG e sobre extensões pgrx. Motores externos têm modelo de durabilidade e de erro diferentes (sem MVCC/WAL do PG), então seu prior art não transfere. Aplicação de YAGNI (`.claude/rules/parsimony-ladder.md` rung 1) ao escopo da própria investigação. **Alternativas consideradas:** varredura ampla (custo alto, retorno marginal, estouraria o budget de questões do `discover-phd-rigor.md § 2`).

**Consequences:** o blueprint não terá comparação com formatos columnar; aceito — não é o problema do M146.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Qual é o protocolo EXATO de rename durável do PostgreSQL (`durable_rename`) — quais `fsync` são emitidos, em que ordem (arquivo antes ou depois do rename? **diretório-pai?**), e qual o comportamento em falha de fsync? | techniques | `.claude/knowledge-base/references/postgres/` | `Grep -n "durable_rename\|fsync_fname\|fsync_parent_path\|pg_fsync" postgres/src/backend/storage/file/fd.c` para mapear os hotspots | **EC-1 (MUST-FIX):** Read o corpo de `durable_rename` (`fd.c:782`) **MAIS os helpers que ele delega: `fsync_fname` (`:756`), `fsync_fname_ext` (`:3797`) e `fsync_parent_path` (`:3873`)** — ler só o corpo perde o fsync do diretório-pai, que é o que torna o rename durável. **R0:** WebSearch/WebFetch de fsyncgate 2018 + "Can Applications Recover from fsync Failures?" (USENIX ATC'20) | Sequência ordenada de syscalls **enumerando cada fsync com seu alvo (arquivo vs diretório)** + razão de cada um + `elevel` de falha. **EC-7:** separar EXPLICITAMENTE "ordenação dos fsyncs" de "semântica de falha do fsync" |
| Q2 | **(i)** O que o pgvector valida — e com que severidade — no **read path** de elemento/vizinho HNSW? **(ii)** O que o `amcheck` valida **offline** e por que o modelo é diferente? | techniques | `.claude/knowledge-base/references/pgvector/`, `.claude/knowledge-base/references/postgres/` | `Grep -n "elog(ERROR\|Assert\|corrupt\|invalid" pgvector/src/hnswutils.c pgvector/src/hnswscan.c` e `postgres/contrib/amcheck/verify_nbtree.c` | (i) Read cada hotspot do read path — **stop criterion EC-5: APENAS o caminho de leitura de elemento/vizinho; o resto do pgvector é out-of-scope**. (ii) Read as checagens estruturais de `verify_nbtree.c`/`verify_heapam.c`. WebSearch de práticas de validação de índice ANN persistido | Duas listas separadas (read-path vs offline) + severidade de cada + **declaração explícita de qual modelo o M146 adota para `from_bytes` (EC-2: read-path, O(1) por vizinho, barato — NÃO o exaustivo do amcheck)** |
| Q3 | Qual o idioma canônico do upstream para resolver nome de relação vindo do usuário em SQL dinâmico — `regclass`/`to_regclass`, `quote_ident` ou `format('%I')` — e como cada um falha para entrada maliciosa/inexistente? | techniques | `.claude/knowledge-base/references/postgres/` | `Grep -n "regclassin\|to_regclass" postgres/src/backend/utils/adt/regproc.c`; `Grep -n "quote_identifier\|quote_ident" postgres/src/backend/utils/adt/quote.c postgres/src/backend/utils/adt/ruleutils.c` | Read cada hotspot. **EC-3 (checkpoint):** confirmar leitura de `quote_identifier` em `ruleutils.c` — é a implementação real por trás de `format('%I')`; `quote.c` só expõe a função SQL. **R0:** WebFetch da doc oficial PG ("Object Identifier Types" + "String Functions") | Tabela: idioma → o que valida → SQLSTATE em falha → quando usar. Mais a recomendação para `graph.rs:265` |
| Q4 | Como uma extensão pgrx madura (paradedb) estrutura o erro tipado de dados persistidos que lê de páginas PG? | techniques | `.claude/knowledge-base/references/paradedb/` | `Grep -rn "invalid\|corrupt\|ErrorKind\|panic!\|ereport" paradedb/pg_search/src/` para mapear os sites | Read os 2-3 sites mais representativos, comparando com o idioma próprio (`theodb_rs/src/pg.rs` `err_input`) | Padrão de erro (typed vs panic) + onde fica o boundary + veredito: nosso idioma já está alinhado ou não |
| Q5 | Como o `amcheck` do upstream é testado — que padrão de teste de corrupção (injeção? fixture corrompida?) ele usa? | tests | `.claude/knowledge-base/references/postgres/` | `ls postgres/contrib/amcheck/sql/ postgres/contrib/amcheck/t/ postgres/contrib/amcheck/expected/` | **EC-4 (checkpoint):** listar e ler `t/` (TAP) além de `sql/` — corromper uma página não é possível de SQL puro, então a injeção real de corrupção vive no TAP. Read 1-2 arquivos representativos + o `Makefile` | Padrão de teste reproduzível de corrupção, com o mecanismo de injeção nomeado |
| Q6 | Como o paradedb testa paths de erro/corrupção do seu codec — `#[pg_test]` com `error =`, teste Rust puro, ou SQL regression? | tests | `.claude/knowledge-base/references/paradedb/` | `find paradedb/pg_search/tests paradedb/tests -type f \| head`; `Grep -rn "pg_test(error\|should_panic\|assert.*Err" paradedb/` | Read os arquivos de teste representativos de cada nível | Distribuição dos 3 níveis da pirâmide + o idioma de "espera erro" aplicável ao nosso `#[pg_test]` |
| Q7 | Que dependências o paradedb usa para durabilidade de arquivo (fsync) e para teste de corrupção — ou é tudo `std`/`pg_sys`? | deps | `.claude/knowledge-base/references/paradedb/` | `Grep -n "fsync\|sync_all\|tempfile\|fs2" paradedb/Cargo.toml paradedb/pg_search/Cargo.toml` | Read `Cargo.toml` na íntegra + os sites de uso encontrados | Lista de deps (ou "nenhuma — std/pg_sys") com justificativa, cruzada com o rung 4 da parsimony-ladder |
| Q8 | Que ferramentas o upstream e o paradedb usam para injetar falha/corrupção em teste (pg_regress, TAP, cargo-pgrx harness)? | tools | `.claude/knowledge-base/references/postgres/`, `.claude/knowledge-base/references/paradedb/` | `Read postgres/contrib/amcheck/Makefile`; `Grep -n "pg_regress\|prove\|TAP\|cargo pgrx" postgres/contrib/amcheck/Makefile paradedb/Makefile` | Read ambos os Makefiles + a config de teste do paradedb | Nome das ferramentas + como se invoca cada uma + qual é executável no nosso droplet |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q5, Q6 | Covered |
| Dependencies | Q7 | Covered |
| Tools | Q8 | Covered |
| Techniques | Q1, Q2, Q3, Q4 | Covered |

**Coverage: 4/4 corners covered (100%)**

Budget check (perfil frontier do `.claude/rules/discover-phd-rigor.md § 2`): 8 questões (janela 6-14 ✓); por corner — techniques 4 (≤5 ✓, ≥2 ✓), tests 2 (≥1 ✓), deps 1 (≥1 ✓), tools 1 (≥1 ✓). Cada questão mapeia a exatamente um corner.

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | O path `.claude/knowledge-base/references/{project}/{path}` declarado na Fase A existe | Marcar Qx BLOCKED com razão "path not found", seguir |
| Per-question budget | A busca da Fase A retornou ao menos um hotspot OU 3 variantes de query foram tentadas | Após 3 tentativas vazias, marcar Qx BLOCKED com razão "busca exaurida"; seguir |
| After answering Qx | A seção do blueprint sob Qx tem ao menos uma citação `arquivo:linha` que resolve em disco | Re-iterar Qx (1 retry) |
| Q1 — completude do protocolo (EC-1) | A resposta enumera o fsync do **diretório-pai**, não só o do arquivo | Re-ler `fsync_parent_path` (`fd.c:3873`) antes de marcar `done` |
| Q2 — separação de modelos (EC-2) | A resposta tem DUAS listas (read-path vs offline) e declara qual o M146 adota | Re-iterar Q2; não aceitar lista única |
| Q3 — implementação do `%I` (EC-3) | `quote_identifier` foi lido em `ruleutils.c` | Ler antes de marcar `done` |
| Q5 — TAP incluído (EC-4) | `contrib/amcheck/t/` foi listado e lido | Ler antes de concluir o padrão de teste |
| Q2 — escopo (EC-5) | Só o read path de elemento/vizinho do pgvector foi lido | Interromper leitura fora do escopo |
| Transversal (EC-6) | A versão/SHA do clone do `postgres` está registrada no blueprint | Registrar antes de fechar |
| Techniques — ≥2 fontes (R2) | Cada questão `techniques` cita ≥2 fontes primárias independentes; Q1 e Q3 com ≥1 web (R0) | Re-iterar até satisfazer ou marcar BLOCKED honesto |
| Before promising complete | Os 4 coverage corners têm seção populada | Recusar a promise, continuar iterando |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly marked BLOCKED with reason
- [ ] All four coverage corners have populated sections in the blueprint
- [ ] Every citation in the blueprint points to a real `.claude/knowledge-base/references/{...}` path
- [ ] At least one ADR section in the blueprint synthesizes decisions taken
- [ ] Time budget respected per project
- [ ] R0 cumprido: ≥2 buscas web citadas (Q1, Q3 no mínimo) com fonte autoritativa
- [ ] Para cada ponto (a)-(d) do M146: uma recomendação acionável de ≤5 linhas
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint saved at `.claude/knowledge-base/discoveries/blueprints/m146-hardening-remediation-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed → confidence re-score)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100% covered
- [ ] ADRs reference at least one principle from project rules — D2 e D4 citam `.claude/rules/parsimony-ladder.md` (rungs 1/3) e `CLAUDE.md § 9` (Regra 9, Não Reinvente); D3 cita `.claude/rules/error-handling.md` (erro tipado no boundary)
