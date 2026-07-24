---
slug: m146-hardening-remediation
milestone_id: M146
created_at: 2026-07-23
goal: Eliminar os 3 defeitos de hardening confirmados do theodb_rs com prova medida RED→GREEN em 100% deles
---

# Plan: M146 — Remediação do review-cycle theodb_rs (hardening + tests + cleanup)

**Version:** 1.0
**Milestone:** M146 (gated M145)
**Baseline SHA:** `b948ea7`
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m146-hardening-remediation-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89)

## Goal

Eliminar os **3 defeitos de hardening confirmados** do `theodb_rs` — panic atravessando a fronteira C por índice de vizinho fora de faixa, injection de segunda ordem em `graph_build`, e export Parquet não-durável — com **métrica observável: 3/3 defeitos com teste RED→GREEN efetivamente executado** (RED falha antes do fix, GREEN passa depois), mais os test-gaps e o cleanup do review fechados.

## Context

O `/review-cycle:loop` full-tree do `theodb_rs` (12 arquivos mais críticos × 10/10 pilares, 32 findings, precision 1.00, 0 blockers) surfou quatro classes de ponto acionável. A fase DISCOVER deste milestone (blueprint acima, `/discover-confidence` 89) fixou, com prior art citado do PostgreSQL upstream, do pgvector e do paradedb, **qual implementação de referência copiar** em cada ponto — evitando invenção local (Regra 9 / `.claude/rules/parsimony-ladder.md` rungs 2-4).

Decisões já travadas pelo blueprint (D1/D2/D3 dele): modelo **read-path** (pgvector) e não amcheck para validação em `from_bytes`; durabilidade com **stdlib pura**, zero dependência nova; **elevel explícito por call-site** (ERROR, não PANIC, para um export refazível).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC hoje | Último commit | Papel |
|---|---|---|---|
| `theodb_rs/src/ann/hnsw.rs` | 602 | `c6025d3` 2026-07-21 | Índice HNSW in-memory; `from_bytes` (`:444`) desserializa o blob persistido |
| `theodb_rs/src/graph.rs` | 1143 | `cce2233` 2026-07-23 | Engine de grafo; `build_csr` (`:263`) monta SQL dinâmico |
| `theodb_rs/src/parquet.rs` | 362 | `259c117` 2026-07-23 | Export Parquet; `atomic_write_parquet` (`:251`) |
| `theodb_rs/src/pg.rs` | — | — | Helpers de erro tipado (`err_input` em `:8`) |
| `theodb_rs/src/am/page/ivf.rs` | 1209 | `c6025d3` 2026-07-21 | Codec de página IVF/AQ — **`mod tests` = 0** |
| `theodb_rs/src/ann/ivf.rs` | 481 | `c6025d3` 2026-07-21 | `with_soar_spill` (`:109`) sem teste; `from_bytes` (`:396`) **é o precedente correto** (`:416`) |
| `theodb_rs/src/am/scan.rs` | 1477 | `c6025d3` 2026-07-21 | Dead code `scan_hnsw_structured` (`:261`); sites de erro de desserialização (`:1259-1262`) |
| `theodb_rs/isolation/corrupt_index.sh` | (NEW) | — | Harness de injeção de corrupção byte-level |

### Current callers / dependents

- `HnswIndex::from_bytes` ← `am/index.rs:33` (`Persisted::from_bytes`) ← `am/scan.rs:1259` e `am/build.rs:757`. **É alimentado do disco**, não só de round-trip.
- `atomic_write_parquet` ← `parquet.rs:332` (caller único — raio de impacto mínimo).
- `with_soar_spill` ← `am/build.rs:192` (está no caminho de produção).
- `graph_build(edge_rel, src_col, dst_col)` é `#[pg_extern]` público (`graph.rs:341-343`), com `REVOKE ALL ... FROM PUBLIC` (`graph.rs:578`) e **sem** `SECURITY DEFINER` → SECURITY INVOKER.

### Domain glossary

- **read-path validation** — checagem local O(1) executada no caminho de leitura (modelo pgvector), oposta à verificação exaustiva offline (modelo amcheck).
- **durable rename** — protocolo `fsync(arquivo) → rename → fsync(diretório-pai)`; sem o fsync do diretório o *rename* pode se perder num crash.
- **injection de segunda ordem** — o `format()` é parametrizado com segurança, mas a string resultante é executada depois; o payload entra pela string montada, não pela chamada de `format`.
- **FFI panic** — panic Rust desenrolando através da fronteira C do PostgreSQL; derruba o backend.

### Architecture boundaries affected

Nenhuma fronteira de camada é alterada. As mudanças ficam dentro de: `ann/` (domínio puro, sem `pg_sys`), `pg.rs` (helper de erro), `am/` (adaptador) e `parquet.rs` (I/O externo). A direção de dependência (`am → ann → vec`) permanece — confirmado pelo pilar architecture do review (`ann/scan_core.rs` é DIP exemplar).

## Prior Art & Related Work

- **Blueprint desta descoberta** — `.claude/knowledge-base/discoveries/blueprints/m146-hardening-remediation-blueprint.md`, com 5 recomendações acionáveis e 3 ADRs.
- **PostgreSQL upstream** `durable_rename` — protocolo de 5 fsyncs (o do diretório-pai é o load-bearing).
- **pgvector** — modelo read-path: checagem local O(1) por vizinho antes do uso.
- **amcheck** — modelo offline + os 3 mecanismos de injeção de corrupção em teste.
- **paradedb** — idioma de duas zonas (erro tipado na fronteira do usuário, panic para dado persistido); `proptest` em codec.
- **M144** — precedente de milestone de remediação sob TDD neste repo.

## Objective

Fechar os pontos do review com o idioma de referência já validado, sem adicionar dependência e sem mudar superfície SQL, provando cada correção com teste executado.

## ADRs

### D1 — Validação de vizinho no `from_bytes` (read-path), não verificação exaustiva

**Decisão:** adicionar em `hnsw.rs::from_bytes`, imediatamente após o bloco de counts/entry, a checagem `neighbors.iter().flatten().flatten().any(|&nb| nb >= n)` retornando `Err` tipado — espelhando verbatim o irmão `ivf.rs:416`.

**Rationale:** o comentário do próprio arquivo (`hnsw.rs:462-464`) declara a invariante — *"a structurally-complete but semantically-corrupt blob must NOT reach `search` … that would panic across the C FFI boundary"* — mas a implementação só cobre `entry` e counts. `search_layer:290-294` faz `visited[nb]` e `self.vectors[nb]` **sem bounds-check**, e `greedy_descend:252` idem: um vizinho fora de faixa é panic atravessando FFI, num path alimentado do disco. **Alternativas rejeitadas:** (i) verificação exaustiva estilo amcheck (invariantes não-locais, custo O(índice), lock mais forte — transformaria uma sonda O(log n) em verificação O(n)); (ii) adiar para uma função `theodb_amcheck()` separada (não resolve o panic no path quente, que é o defeito); (iii) não validar (mantém a garantia documentada não cumprida). Cita `.claude/rules/error-handling.md` (fail-fast, erro tipado na fronteira).

**Consequências:** custo ~zero (O(1) por vizinho, uma vez por scan, sobre bytes já tocados na desserialização). Invariantes de grafo (simetria, alcançabilidade) permanecem não verificadas — aceito e registrado em Unresolved Questions.

### D2 — Durabilidade com stdlib, sem dependência nova

**Decisão:** implementar o protocolo do `durable_rename` com `std::fs::File::sync_all` no arquivo temporário **antes** do rename e no diretório-pai **depois**.

**Rationale:** rungs 2-3 da `.claude/rules/parsimony-ladder.md` resolvem: a stdlib faz, e o idioma é o do host. `grep "sync_all|sync_data|fsync" theodb_rs/src/` retorna **zero hits** hoje — o export é atômico mas não durável. **Alternativas rejeitadas:** (i) `fs2`/`fs-err`/`tempfile` — dependência redundante para zero capacidade nova (rung 4 proíbe, e o paradedb, extensão bem maior, não tem nenhuma); (ii) delegar ao checkpointer do PG como o paradedb — **impossível**, o Parquet é escrito fora do datadir; (iii) manter só o rename atômico — é o defeito.

**Consequências:** dois syscalls a mais por export (caminho de export, não de query — latência aceitável). Nenhuma superfície de dependência nova.

### D3 — Severidade por call-site: `Err`/ERROR, não PANIC

**Decisão:** falha de fsync no export vira `Err` tipado (→ ERROR), nunca PANIC.

**Rationale:** o `durable_rename` upstream **repassa o elevel do caller** e não força PANIC; o PANIC vive em `data_sync_elevel` e existe para o caso em que a única cópia sobrevivente do dado está no WAL. O export é write-temp-then-rename com a fonte ainda disponível → ERROR + refazer é sólido. **Alternativa rejeitada:** herdar PANIC "porque fsync falhou" — derrubaria o cluster por falha de um export re-executável.

**Consequências:** o operador refaz o export; o cluster não cai.

### D4 — SQLSTATE de corrupção próprio (`err_corrupt`)

**Decisão:** adicionar `err_corrupt()` em `pg.rs` usando `ERRCODE_DATA_CORRUPTED` e rotear os sites de erro de desserialização do AM por ele.

**Rationale:** hoje `am/scan.rs:1261` usa `pg_sys::error!` → **XX000 (internal_error)**; página corrompida merece SQLSTATE de corrupção. O precedente é o amcheck (`ERRCODE_INDEX_CORRUPTED`, 53 sites), e `pg.rs` já tem a maquinaria (`ErrorReport::new(PgSqlErrorCode::...)`). **Alternativas rejeitadas:** (i) migrar `Result<_,String>` para enums `thiserror` — nada ramifica na variante; YAGNI (o próprio paradedb usa `#[error(transparent)]` nesse caso); (ii) manter XX000 — indistinguível de bug interno, prejudica diagnóstico do operador.

**Consequências:** ~10 linhas; melhora diagnosticabilidade. Provar o SQLSTATE exige assertion out-of-process (`psql`), não coberta por `#[pg_test(error=)]` — registrado em Unresolved Questions.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Owner |
|---|---|---|---|---|
| R1 | A validação de vizinho pode **rejeitar blobs legítimos** de layout M26 deprecado | MEDIA | O gate é `nb >= n` com `n = vectors.len()` — um blob válido nunca referencia fora da faixa; o teste RED usa blob corrompido sinteticamente e um teste de round-trip garante que blob válido continua passando | implementador |
| R2 | O fsync do diretório-pai **falha em alguns filesystems** (o próprio upstream tolera `EBADF`/`EINVAL` em diretório) | MEDIA | Espelhar a tolerância do upstream: erro de fsync **de diretório** com `EBADF`/`EINVAL` não é fatal; demais erros propagam como `Err` | implementador |
| R3 | `::regclass` **rejeita nomes que hoje "passam"** — mudança de comportamento observável | BAIXA | É a mudança pretendida (fail-closed). O erro fica mais claro (42P01 nomeando a relação) em vez de erro de sintaxe tardio. Documentar no CHANGELOG | implementador |
| R4 | O harness de corrupção byte-level pode ficar **flaky** se algo reescrever a página | MEDIA | Copiar a disciplina do amcheck: `autovacuum=off` no cluster + `autovacuum_enabled=false` na tabela + **parar o cluster** antes de editar | implementador |
| R5 | `cargo pgrx test` **não roda localmente** (símbolos PG) | BAIXA | Tier 1 dos testes é `#[test]` Rust puro (roda local); tiers 2-3 rodam no droplet (`cargo-pgrx 0.19.0`, PG 18.4, usuário `pgtest`) — substrato já verificado |

## Unresolved Questions

- Q1: Devemos verificar invariantes de grafo (simetria de arestas, monotonicidade de nível, alcançabilidade a partir do entry)? Permanecem não verificadas — são amcheck-shaped (O(n·m), não-locais). Ficam para um eventual `theodb_amcheck()` opt-in; **fora do escopo do M146** por D1.
- Q2: Como provar o SQLSTATE, já que `#[pg_test(error=)]` casa a mensagem e não o código? Provar `ERRCODE_DATA_CORRUPTED` exige assertion out-of-process via `psql`. Decisão tomada: incluir a assertion de SQLSTATE no harness `corrupt_index.sh` (que já roda `psql`), não em `#[pg_test]`.
- Q3: As linhas citadas do PG 17.10 valem para o PG 18? O comportamento é estável e nenhuma linha do upstream é copiada para o código — só o *protocolo*. Sem impacto na implementação; registrado por honestidade.

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.19` | rust | Já é a fundação da extensão; fornece `#[pg_test]`, `PgSqlErrorCode` (para D4) e a fronteira FFI. Nenhuma versão nova. |
| `std` (Rust stdlib) | — | rust | `std::fs::File::sync_all` resolve a durabilidade (D2, rungs 2-3 da parsimony-ladder) |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale (libs evaluated) | Why this one |
|---|---|---|---|---|
| (none) | — | — | Avaliadas e **rejeitadas**: `fs2` (lock/alloc de arquivo — não cobre fsync de diretório, dependência redundante), `fs-err` (só melhora mensagem de erro; `Err` tipado nosso já nomeia o path), `tempfile` (o fluxo já cria o temp determinístico ao lado do alvo, que é requisito do rename atômico no mesmo FS). O paradedb, extensão pgrx bem maior, não declara **nenhuma** dependência de durabilidade — delega ao checkpointer; nós não podemos (escrevemos fora do datadir), mas a stdlib basta. | — |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

**Nota:** `proptest` foi considerado (o paradedb usa em codec, `pg_search/Cargo.toml:56`) e **deferido** — os testes deste milestone são casos construídos determinísticos; adicionar geração de propriedade agora é YAGNI (rung 1). Reavaliar se a suíte de corrupção crescer.

## Dependency Graph

```
Fase 1 (defeitos reais, independentes entre si — podem paralelizar)
  T1.1 hnsw bounds ──┐
  T1.2 graph regclass├──▶ Fase 2 (taxonomia de erro + testes)
  T1.3 parquet fsync ┘        T2.1 err_corrupt (depende de T1.1 para ter sites a rotear)
                              T2.2 page/ivf mod tests
                              T2.3 with_soar_spill test
                              T2.4 dead code + doc-drift
                                        │
                                        ▼
                              Fase 3 (prova end-to-end)
                                T3.1 corrupt_index.sh (depende de T1.1 + T2.1)
```

## Phase 1: Defeitos de hardening confirmados

### T1.1 — Validação de índice de vizinho em `HnswIndex::from_bytes`

#### Objective
Fechar o panic atravessando FFI: vizinho fora de faixa vira `Err` tipado antes de chegar ao `search`.

#### Why this step (action + reasoning — ReAct discipline)
**Ação:** adicionar uma checagem `any(|&nb| nb >= n)` sobre `neighbors` em `from_bytes`, espelhando `ivf.rs:416`.
**Raciocínio:** o comentário em `hnsw.rs:462-464` declara exatamente esta invariante mas a implementação só a garante para `entry`; `search_layer:290-294` e `greedy_descend:252` indexam sem bounds-check, e o blob vem do disco (`am/index.rs:33` ← `scan.rs:1259`). É o defeito de maior severidade do milestone (derruba o backend), e o blueprint D1 já travou o modelo (read-path, O(1)).

#### Evidence
`theodb_rs/src/ann/hnsw.rs:462-475` (comentário + validação incompleta); `:288-294` e `:251-253` (indexação sem checagem); `theodb_rs/src/ann/ivf.rs:416` (o precedente correto); `theodb_rs/src/am/index.rs:33` (alimentação do disco).

#### Files to edit
- `theodb_rs/src/ann/hnsw.rs` (validação + teste)

#### Deep file dependency analysis
`from_bytes` é chamado só por `am/index.rs:33`; a mudança é aditiva (mais um caminho de `Err`), sem alteração de assinatura. Nenhum caller precisa mudar.

#### TDD
- **RED** — `#[test] fn from_bytes_rejects_out_of_bounds_neighbor()` (Rust puro, **roda localmente**): construir um índice válido, serializar com `to_bytes`, mutar um índice de vizinho para `n` (fora de faixa), e assertar `assert!(matches!(HnswIndex::from_bytes(&bad), Err(m) if m.contains("out-of-bounds")))`. Antes do fix o teste FALHA (hoje retorna `Ok`).
- **GREEN** — adicionar a checagem; o teste passa.
- **REFACTOR** — verificar que a mensagem segue o padrão `"theodb hnsw: ..."` do arquivo.
- **Regressão de não-quebra:** `#[test] fn from_bytes_accepts_valid_roundtrip()` — blob válido continua `Ok` (protege contra R1).
- **EC-3 (edge):** `#[test] fn from_bytes_accepts_empty_index()` — índice vazio (`n=0`) continua `Ok`; `any()` sobre iterador vazio é `false`, então o comportamento correto cai por construção, mas o teste trava a regressão.

#### Concurrency tests
(none — single-threaded) A desserialização é local ao backend, sem estado compartilhado mutável; nenhuma primitiva de concorrência é tocada.

#### Acceptance Criteria
- `cargo test --lib from_bytes_rejects_out_of_bounds_neighbor` passa (executado localmente, saída anexada ao log).
- Executar `cargo test --lib from_bytes_accepts_valid_roundtrip` retorna exit 0 (blob válido continua `Ok`).
- Executar `cargo test --lib from_bytes_rejects_out_of_bounds_neighbor` e confirmar que a mensagem casa a substring `out-of-bounds` (não booleano) — lição do amcheck.

#### DoD
- [ ] RED executado e registrado como falho antes do fix
- [ ] GREEN executado e verde depois
- [ ] `cargo check --features pg18` verde no droplet
- [ ] CHANGELOG `[Unreleased] § Fixed`

### T1.2 — `graph.rs`: resolver `edge_rel` via `::regclass::text` + corrigir o comentário falso

#### Objective
Fechar a injection de segunda ordem em `build_csr` e remover a afirmação de segurança que o código não entrega.

#### Why this step
**Ação:** trocar `$3` por `($3)::regclass::text` no `format` de `build_csr` e reescrever o comentário `:262`.
**Raciocínio:** `edge_rel` é entrada de usuário (`graph_build` é `#[pg_extern]`) spliced com `%s` cru e depois executada (`graph.rs:274`). `::regclass` valida existência e falha **42P01 antes** de montar o SQL, e `regclassout` re-renderiza o nome a partir do catálogo (injection-proof por construção). `%I` seria errado: trata a entrada inteira como um identificador, mutilando `schema.tabela`. **O próprio arquivo já usa `::regclass::oid` corretamente em `:362`, `:380`, `:397`** — o scan é o único outlier, e alinhar elimina também uma divergência latente de `search_path`.

#### Evidence
`theodb_rs/src/graph.rs:262` (comentário falso), `:265` (`%s` em `$3`), `:274` (execução), `:341-343` (`#[pg_extern]`), `:362`/`:380`/`:397` (idioma correto já presente); blueprint Q3 (tabela regclass vs `%I` vs `%s`).

#### Files to edit
- `theodb_rs/src/graph.rs`

#### Deep file dependency analysis
`build_csr` é chamado por `graph_build` (`:345`) e indiretamente por `graph_refold` (`:370`). Ambos passam `edge_rel` do usuário — os dois ganham o gate.

#### TDD
- **RED** — `#[pg_test(error = "does not exist")] fn graph_build_rejects_nonexistent_relation()`: `SELECT theodb.graph_build('no_such_table_xyz','src','dst')` deve falhar com 42P01 vindo do `regclass`. Antes do fix o erro vem tarde e diferente (erro de sintaxe/relação na execução do scan montado).
- **RED-2 (o que prova a injection)** — teste no harness `corrupt_index.sh`/smoke: criar tabela-vítima, chamar `graph_build` com payload `'g; DROP TABLE victim; --'`, assertar que **a vítima sobrevive** e a chamada errou.
- **GREEN** — aplicar `($3)::regclass::text`; ambos passam.
- **REFACTOR** — reescrever o comentário `:262` descrevendo o que o código realmente garante.

#### Concurrency tests
(none — single-threaded) Sem estado compartilhado novo; o gate é puramente de validação de entrada.

#### Acceptance Criteria
- Relação inexistente → erro 42P01 nomeando a relação, **antes** de qualquer SQL montado ser executado.
- Executar `psql -tAc "SELECT count(*) FROM victim"` após o payload e obter o mesmo count de antes (tabela-vítima intacta).
- Executar `SELECT theodb.graph_build('myschema.edges','src','dst')` e obter contagem de arestas > 0 (prova que `%I` seria errado).
- Confirmar por `grep -n 'regclass' theodb_rs/src/graph.rs` que o comentário `:262` cita `regclass` e não mais `%I` para a relação.

#### DoD
- [ ] RED executado (falha antes) e GREEN (passa depois) no droplet
- [ ] Teste de sobrevivência da vítima executado
- [ ] Teste de schema-qualificado verde
- [ ] CHANGELOG `[Unreleased] § Security`

### T1.3 — `parquet.rs`: rename durável (fsync arquivo + diretório-pai)

#### Objective
Tornar o export Parquet crash-durável, seguindo o protocolo do `durable_rename`.

#### Why this step
**Ação (sequência obrigatória — absorve EC-1 e EC-2 do `/edge-case-plan`):**
1. `w.finish()?` — **escreve o footer/metadata do Parquet** e NÃO consome `self` (`parquet-54.3.1/src/arrow/arrow_writer/mod.rs:333-338`).
   **NUNCA trocar `close()` por `into_inner()` diretamente:** `into_inner` (`:325`) faz só `flush()` + devolve o writer, **sem** `finish()` → produziria arquivo **sem footer, ilegível** (EC-1). `close()` (`:341`) é literalmente `self.finish()`.
2. `let file = w.into_inner()?;` — recupera o `File` já com o footer escrito.
3. `file.sync_all()?` — dados duráveis **antes** do rename.
4. `std::fs::rename(&tmp, path)?`.
5. fsync do diretório-pai, com o fallback do upstream para parent vazio (EC-2): `let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));` — espelha `fd.c:3885-3886`. Sem isso, um path relativo simples (`"out.parquet"`) daria `File::open("")` → ENOENT e o fsync load-bearing não aconteceria.
**Raciocínio:** hoje há temp+rename (atômico) mas **zero fsync** (`grep` confirma 0 hits no crate) → um crash após o rename pode deixar arquivo presente-porém-curto. O upstream resolve com 5 fsyncs, sendo o do **diretório-pai** o load-bearing (sem ele o próprio rename se perde). D2 trava stdlib pura; D3 trava `Err`/ERROR (não PANIC).

#### Evidence
`theodb_rs/src/parquet.rs:251-270` (fluxo atual), `:332` (caller único); blueprint Q1 (protocolo de 5 fsyncs, `fd.c:781-854`) e Q7 (`grep` zero-hit; nenhuma dependência necessária).

#### Files to edit
- `theodb_rs/src/parquet.rs`

#### Deep file dependency analysis
Caller único (`:332`), assinatura preservada (segue retornando `Result`). Nenhum consumidor externo muda.

#### TDD
- **RED** — `#[test] fn atomic_write_parquet_fsyncs_before_rename()`: teste Rust puro que escreve num diretório temporário e assere o **efeito observável** possível sem crash real — o arquivo final existe, tem tamanho > 0 e é lido de volta corretamente; e, para o protocolo, um teste que assere que `into_inner`+`sync_all` são exercitados (erro de fsync propaga como `Err`, via caminho de path inválido).
- **RED-2 (prova real de durabilidade)** — no harness de crash do droplet: export → `pg_ctl stop -m immediate` durante/após → restart → o arquivo exportado está íntegro e legível.
- **GREEN** — aplicar o protocolo; testes passam.
- **REFACTOR** — tolerar `EBADF`/`EINVAL` no fsync **de diretório** (espelha `fd.c:3822-3825`), propagando os demais erros (R2).

#### Concurrency tests
(none — single-threaded) O export roda no backend chamador; não há writers concorrentes ao mesmo path por contrato da função.

#### Acceptance Criteria
- `sync_all()` chamado no arquivo temporário **antes** do rename e no diretório-pai **depois** (verificável por leitura do código + teste de erro).
- Executar o teste de path inválido e confirmar retorno `Err` (exit 0 do teste), sem panic — D3.
- Confirmar por leitura do código que `EBADF`/`EINVAL` no fsync de diretório retorna `Ok` (espelha `fd.c:3822-3825`), com teste unitário do ramo.
- Executar o harness de crash no droplet e confirmar `parquet-tools`/leitura Arrow do arquivo pós-restart retorna as N linhas exportadas.

#### DoD
- [ ] RED/GREEN local executados
- [ ] Prova de crash no droplet executada com output anexado
- [ ] Zero dependência nova em `Cargo.toml` (D2)
- [ ] CHANGELOG `[Unreleased] § Fixed`

## Phase 2: Taxonomia de erro e fechamento dos test-gaps

### T2.1 — `err_corrupt()` com `ERRCODE_DATA_CORRUPTED`

#### Objective
Dar SQLSTATE de corrupção aos erros de desserialização do AM, hoje XX000.

#### Why this step
**Ação:** adicionar `err_corrupt(msg) -> !` em `pg.rs` (mesmo molde de `err_input`) usando **`ERRCODE_INDEX_CORRUPTED` (XX002)** — mais preciso que `ERRCODE_DATA_CORRUPTED` (XX001) para página de índice, seguindo o precedente do amcheck (EC-6); ambos existem em `pgrx-pg-sys-0.19.0/src/submodules/errcodes.rs:384-385`. Rotear os sites de erro de desserialização do AM por ele.
**Raciocínio:** XX000 é indistinguível de bug interno; o precedente do amcheck é dar código próprio à corrupção (53 sites com `ERRCODE_INDEX_CORRUPTED`). `pg.rs` já tem a maquinaria — é reuso, não construção (D4).

#### Evidence
`theodb_rs/src/pg.rs:8` (molde `err_input`); `theodb_rs/src/am/scan.rs:1259-1262` (site XX000); blueprint Q4.

#### Files to edit
- `theodb_rs/src/pg.rs`, `theodb_rs/src/am/scan.rs`, `theodb_rs/src/am/build.rs`

#### Deep file dependency analysis
Aditivo: nova função em `pg.rs`; sites de erro trocam `pg_sys::error!` por `crate::pg::err_corrupt`. Sem mudança de assinatura pública.

#### TDD
- **RED** — teste no harness que assere o SQLSTATE (`psql -tAc` com `\set VERBOSITY verbose` ou `SQLSTATE` capturado) ser `XX001`/`ERRCODE_DATA_CORRUPTED` ao ler índice corrompido. Antes do fix vem XX000.
- **GREEN** — rotear os sites; SQLSTATE correto.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Erro de desserialização de índice retorna SQLSTATE de corrupção, não XX000.
- Confirmar que a mensagem retornada por `psql` começa com `theodb ` (prefixo de domínio preservado).

#### DoD
- [ ] SQLSTATE verificado via `psql` no droplet (evidência anexada)
- [ ] CHANGELOG `[Unreleased] § Changed`

### T2.2 — `mod tests` in-file em `am/page/ivf.rs`

#### Objective
Cobrir os paths de borda/corrupção do codec de página que os testes SQL não alcançam.

#### Why this step
**Ação:** adicionar `#[cfg(test)] mod tests` cobrindo LABEL_K truncation, `read_record_at` em straddle de chunk, e os erros tipados de corrupção.
**Raciocínio:** o arquivo tem 1209 LoC e **zero** testes in-file, enquanto os irmãos (`page/symqg.rs`, `am/columnar_codec.rs`) têm 8 e 12; os paths de erro tipado são inalcançáveis pelos testes de integração SQL (finding HIGH do pilar tests).

#### Evidence
`theodb_rs/src/am/page/ivf.rs` (1209 LoC, `mod tests` = 0 — verificado por grep); irmãos com testes.

#### Files to edit
- `theodb_rs/src/am/page/ivf.rs`

#### Deep file dependency analysis
Somente adição de módulo de teste; nenhuma mudança em código de produção.

#### TDD
- **RED/GREEN** — cada teste é escrito para falhar contra um input construído-inválido e passar com a asserção do erro tipado esperado (`assert!(matches!(..., Err(m) if m.contains(...)))`), no tier Rust puro sempre que possível.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- ≥ 3 testes cobrindo: truncation de LABEL_K, straddle de chunk em `read_record_at`, erro tipado de corrupção.
- Executar `cargo test --lib page::ivf` e confirmar que cada teste assere uma substring diagnóstica específica (grep `contains(` nos asserts), não booleano.

#### DoD
- [ ] Testes executados verdes
- [ ] CHANGELOG `[Unreleased] § Added`

### T2.3 — Cobrir `with_soar_spill`

#### Objective
Testar a matemática SOAR que está wired em produção mas sem teste.

#### Why this step
**Ação:** teste unitário do efeito de `with_soar_spill(lambda)` sobre a atribuição de listas.
**Raciocínio:** está no caminho de produção (`am/build.rs:192`) e é matemática não-trivial — finding MEDIUM do pilar tests.

#### Evidence
`theodb_rs/src/ann/ivf.rs:109`; caller `theodb_rs/src/am/build.rs:192`.

#### Files to edit
- `theodb_rs/src/ann/ivf.rs`

#### TDD
- **RED/GREEN** — construir um caso pequeno determinístico onde o spill muda a atribuição, assertar o comportamento esperado; com `lambda` neutro, atribuição idêntica ao caminho sem spill.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- Executar `cargo test --lib with_soar_spill` 3× e obter resultado idêntico (determinístico, sem RNG não-injetado), cobrindo lambda neutro e ativo.

#### DoD
- [ ] Teste executado verde
- [ ] CHANGELOG `[Unreleased] § Added`

### T2.4 — Remover dead code e corrigir doc-drift

#### Objective
Higiene: remover `scan_hnsw_structured` (0 callers) e corrigir a doc de `write_ivf_aq_split_streaming`.

#### Why this step
**Ação:** remover a função morta; corrigir os dois parágrafos de doc que descrevem um writer v6/SQ8 sobre uma função v5/f32.
**Raciocínio:** dead code verificado (1 ocorrência no repo inteiro = a própria definição, issue #169) e doc que mente sobre a versão do formato induz erro em quem for mexer no codec.

#### Evidence
`theodb_rs/src/am/scan.rs:261` (dead code, grep 1 ocorrência); `theodb_rs/src/am/page/ivf.rs:780` (doc-drift).

#### Files to edit
- `theodb_rs/src/am/scan.rs`, `theodb_rs/src/am/page/ivf.rs`

#### Deep file dependency analysis
Remoção segura: nenhum caller estático, sem `#[allow(dead_code)]`, função privada. Verificar se helpers ficam órfãos após a remoção.

#### TDD
Sem teste novo (remoção). Aceite mecânico: `cargo check --features pg18` verde + grep zero-ocorrência + suíte existente verde.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- `grep -rn "scan_hnsw_structured" theodb_rs/src/` retorna 0.
- Executar `cargo check --features pg18` no droplet sem warning de `dead_code` novo (nenhum helper órfão).
- Confirmar por `grep -n -A3 'fn write_ivf_aq_split_streaming' theodb_rs/src/am/page/ivf.rs` que a doc cita v5/f32/magic 5.

#### DoD
- [ ] `cargo check` verde no droplet
- [ ] CHANGELOG `[Unreleased] § Removed`

## Phase 3: Prova end-to-end de corrupção

### T3.1 — `isolation/corrupt_index.sh`

#### Objective
Provar que corrupção semântica de índice vira **erro SQL limpo** com o backend vivo — o único teste que prova o efeito de T1.1 + T2.1.

#### Why this step
**Ação:** harness no molde de `isolation/crash.sh` + `t/001_verify_heapam.pl:178-207`: initdb → `CREATE EXTENSION` → construir índice → parar cluster → `dd` numa **faixa** de offsets do arquivo do índice → reiniciar → consultar → assertar ERROR com SQLSTATE de corrupção e backend vivo. **EC-4:** corromper uma faixa e assertar a PROPRIEDADE (qualquer corrupção → erro SQL limpo, backend vivo, nunca `server closed the connection`), em vez de depender de um offset mágico do array de vizinhos — robusto a evolução de layout.
**Raciocínio:** `#[pg_test(error=)]` nunca toca uma página real; um teste Rust puro prova que a checagem dispara, mas não que o backend sobrevive. Só o harness prova a propriedade que interessa ao operador.

#### Evidence
Blueprint Q5 (mecanismo 1 + disciplina de ambiente) e Q8 (a técnica porta para bash sobre o scaffolding existente); `theodb_rs/isolation/crash.sh` (precedente).

#### Files to edit
- `theodb_rs/isolation/corrupt_index.sh` (NEW), `theodb_rs/isolation/Makefile` (alvo)

#### TDD
- **RED** — rodar o harness contra o binário **sem** o fix de T1.1: o backend deve cair (panic FFI) — este é o RED que prova o defeito.
- **GREEN** — com o fix: `ERROR` limpo, SQLSTATE de corrupção, backend vivo (conexão seguinte funciona).

#### Concurrency tests
(none — single-threaded) O harness é sequencial por construção.

#### Failure-scenario coverage
Cobre o cenário "índice corrompido em disco" da seção `## Failure scenarios`.

#### Acceptance Criteria
- Ambiente fixado (`autovacuum=off` no cluster + `autovacuum_enabled=false` na tabela + cluster parado antes da edição) — disciplina do amcheck (R4).
- O harness assere a substring diagnóstica E o SQLSTATE retornado por `psql` (ambos verificados no output).
- Executar uma segunda query após o erro e obter resultado (backend sobrevive; sem `server closed the connection`).

#### DoD
- [ ] RED (sem fix) executado no droplet com output anexado provando o crash
- [ ] GREEN (com fix) executado com output anexado
- [ ] Alvo adicionado ao `isolation/Makefile`
- [ ] CHANGELOG `[Unreleased] § Added`

## Coverage Matrix

| Claim do Goal / finding do review | Task(s) | Status |
|---|---|---|
| Panic FFI por vizinho fora de faixa (defeito 1) | T1.1, T3.1 | mapeado |
| Injection de segunda ordem em `graph_build` (defeito 2) | T1.2 | mapeado |
| Export Parquet não-durável (defeito 3) | T1.3 | mapeado |
| SQLSTATE XX000 para corrupção | T2.1 | mapeado |
| `page/ivf.rs` sem `mod tests` (test-gap HIGH) | T2.2 | mapeado |
| `with_soar_spill` sem teste (test-gap MEDIUM) | T2.3 | mapeado |
| Dead code `scan_hnsw_structured` (#169) | T2.4 | mapeado |
| Doc-drift `ivf.rs:780` | T2.4 | mapeado |
| Prova de que corrupção não derruba o backend | T3.1 | mapeado |
| Zero dependência nova (D2) | T1.3 (DoD) | mapeado |

**Cobertura: 10/10 = 100%.** Nenhum item diferido.

## Correção medida do substrato de teste (2026-07-23, durante o IMPLEMENT)

**Achado empírico que corrige uma suposição do blueprint (Q6/Q8, marcada MEDIUM):** neste ambiente a tier de
teste unitário **não é executável** — nem `cargo test`, nem `cargo pgrx test`. Ambos falham a linkar o target
`lib test` com símbolos do backend indefinidos (`PG_exception_stack`, `errstart`, `errcode`, `errmsg`,
`errhint`, `errfinish`, `do_ereport`), porque uma extensão é carregada *dentro* do postgres e esses símbolos
vivem no binário do servidor.

**Não é causado por este milestone:** o crate já contém **69 `#[test]` puros e 326 `#[pg_test]`** pré-existentes
— eles estão escritos mas não são executados neste substrato. Isso bate com o registro do projeto de que a
validação de M144/M145 foi feita por **A/B in-PG**, não por unit test.

**Consequência para o TDD deste plano (honestidade — Regra 3):** o RED→GREEN das tasks é medido no nível
**SQL/harness sobre instância PG real**, não em unit test:

| Task | RED (antes do fix) | GREEN (depois) |
|---|---|---|
| T1.1 | binário SEM o fix + índice corrompido em disco → backend morre (panic FFI) | binário COM o fix → `ERROR` limpo, backend vivo |
| T1.2 | binário SEM o fix + payload de injection → efeito colateral observável | binário COM o fix → 42P01 do `regclass`, sem efeito colateral |
| T1.3 | export + crash → arquivo truncado/ilegível | export + crash → arquivo íntegro |

Os testes unitários **continuam sendo escritos** (documentação executável para quando o substrato permitir), mas
**não são contados como evidência** neste milestone. Nenhum DoD será marcado com base neles.

## Global Definition of Done

- [ ] 3/3 defeitos com RED executado (falhando) e GREEN executado (passando) — a métrica do Goal
- [ ] Suíte completa verde (`cargo test` local + `cargo pgrx test` no droplet)
- [ ] `cargo check --features pg18` verde no droplet
- [ ] Zero dependência nova em `Cargo.toml`
- [ ] Zero mudança de superfície SQL (mesmas assinaturas `pg_extern`)
- [ ] CHANGELOG `[Unreleased]` com entrada por task
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}
- [ ] Issues #168, #169 referenciadas nos commits correspondentes

## Failure scenarios (I/O externo: filesystem no export Parquet)

| Dependência | Modo de falha | Como o teste reproduz | Comportamento esperado |
|---|---|---|---|
| Filesystem (arquivo temp) | `sync_all()` falha (EIO) | path em FS somente-leitura / fd inválido | `Err` tipado propagado (D3), nunca panic; arquivo final não é publicado |
| Filesystem (diretório-pai) | fsync de diretório não suportado (`EBADF`/`EINVAL`) | ambiente onde o FS recusa | **Não fatal** — espelha `fd.c:3822-3825`; export conclui |
| Filesystem | ENOSPC durante a escrita | disco cheio simulado (arquivo em tmpfs pequeno) | `Err` com mensagem clara; temp removido ou deixado sem publicar |
| Filesystem | crash entre `sync_all` e `rename` | `pg_ctl stop -m immediate` no droplet | Após restart: ou o arquivo antigo íntegro, ou o novo íntegro — nunca um truncado |
| Índice em disco | página corrompida | `corrupt_index.sh` (dd num offset) | ERROR limpo com SQLSTATE de corrupção; backend vivo |

## Final Phase: Integration Validation (MANDATORY)

### Execution

1. `cargo test --lib` (local) — tier Rust puro verde.
2. Sync para o droplet + `cargo check --features pg18` verde.
3. `cargo pgrx test` no droplet (usuário `pgtest`) — tier in-process verde.
4. `bash theodb_rs/isolation/corrupt_index.sh` — GREEN com output anexado.
5. Harness de crash do export Parquet — arquivo íntegro pós-crash.
6. `/code-quality` — verdict ∉ {FAIL_HARD, INVALID}.

### Acceptance Criteria

- Todos os 6 passos executados com output real anexado ao implementation log (nunca "deveria funcionar").
- Nenhum DoD marcado sem evidência correspondente.

### If Validation Fails

Loop de validação por `cycle-implement`: corrigir a causa-raiz de um check FAIL por iteração, re-rodar. Nunca enfraquecer teste nem baixar threshold. Se um FAIL_HARD não for remediável sem scope-creep, emitir BLOCKED honesto e voltar ao `/to-plan`.
