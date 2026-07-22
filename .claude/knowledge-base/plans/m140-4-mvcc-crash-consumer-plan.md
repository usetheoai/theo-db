---
slug: m140-4-mvcc-crash-consumer
milestone_id: M140.4
created_at: 2026-07-22
goal: Provar MVCC/VACUUM/crash da engine BM25 de produção contra o binário shipado + probe de threads #153 + primeiro consumidor theo-lens com evidência.
---

# Plan: M140.4 — MVCC/VACUUM/crash provados + primeiro consumidor (theo-lens)

> **Version 1.0** — Fecha o M140: prova a robustez de produção da engine BM25 (M140.3) pelas **suítes de
> isolamento+crash contra o binário shipado** (o mesmo padrão do M99/M135, não uma versão mais fraca), instala a
> **disciplina de thread-safety #153** como regressão de CI (o probe de threads), e liga o **primeiro consumidor
> real** — a busca de traces do theo-lens sobre `bm25_search`. Alimenta o anchor do M141 (dogfood `running`).

## Goal

> Enable a engine BM25 de produção a ser reivindicada **robusta** (MVCC/VACUUM/crash) e **consumida** por um
> cliente real, provando: (a) `scripts/m140-4-lexical-robustness.sh` verde (crash+replay + VACUUM + MVCC contra o
> binário shipado), (b) o probe de threads `probe_directory_threads` verde (nenhum toque em PG/SPI fora da main
> thread), (c) o consumidor theo-lens indexa+busca traces via `bm25_search` retornando os traces corretos,
> measured by os três artefatos de evidência verdes em `docs/benchmarks/m140-4-data/`.

## Context

M140.3 (v0.128.0, `docs/adr/0054`) entregou a superfície BM25 de produção (`bm25_build`/`bm25_search`) com cache
MVCC-correto — provado por um smoke de 2 sessões, mas a robustez de PRODUÇÃO (crash real + VACUUM + as suítes de
isolamento contra o binário shipado, o padrão M99/M135) é o escopo deste milestone. O review do M140.3 deixou dois
follow-ups rastreados aqui: (1) o LOW de co-localização SPI (`read_generation` + `load` num snapshot, sob RC), e
(2) a disciplina #153 (o probe de threads como regressão). O consumidor real (theo-lens, hoje `ts_rank` em
`trace-read-repository.ts:365`) fecha o loop: prova que a engine serve um cliente de verdade — o anchor do M141.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/lexical/engine.rs` | ~200 | `30c91fc`~ (M140.3) | `bm25_build`/`bm25_search` + cache | Comportamento M140.3 preservado; o hardening SPI é aditivo/interno |
| `theodb_rs/lexical_core/src/lib.rs` | ~305 | M140.3 | `PgDirectory` (o `Directory` do Tantivy) | pgrx-free mantido; o probe instrumenta sem tocar PG |
| `theodb_rs/lexical_core/src/cache.rs` | ~130 | M140.3 | `IndexCache` | inalterado |
| `scripts/m140-4-lexical-robustness.sh` (NEW) | 0 | — | (novo) crash+replay + VACUUM + MVCC contra o binário shipado | — |
| `theodb_rs/lexical_core/src/probe.rs` (NEW) | 0 | — | (novo) o probe de threads #153 (qual thread chama o Directory) | — |
| `scripts/m140-4-consumer-theolens.sh` (NEW) | 0 | — | (novo) proof do consumidor: span-data do theo-lens → bm25 → traces certos | — |
| `../theo-lens/packages/core/src/infrastructure/db/observability/trace-read-repository.ts` | 540 | (theo-lens) | a busca de traces (`ts_rank`) — o consumidor | um NOVO caminho bm25 aditivo (feature-flag); o `ts_rank` continua o default até o M141 |
| `docs/adr/0055-m140-4-lexical-robustness-consumer.md` (NEW) | 0 | — | (novo) ADR do modelo de robustez + o boundary do consumidor (proof vs M141) | — |
| `docs/benchmarks/m140-4-robustness-consumer.md` (NEW) | 0 | — | (novo) o report com as 3 evidências | — |

### Current callers / dependents

- **Symbol:** `bm25_build`/`bm25_search` (`engine.rs`) — chamados via SQL. O consumidor theo-lens é o novo caller (via SQL, cross-repo).
- **Symbol:** `PgDirectory` (`lexical_core/src/lib.rs`) — o `probe.rs` instrumenta os métodos do trait `Directory` para registrar o `ThreadId`; nenhum caller de produção muda.
- **Symbol (theo-lens):** `listTraces` (`trace-read-repository.ts:269`) usa `ts_rank` (`:365`) — o novo caminho bm25 é aditivo, não substitui até o M141.

### Domain glossary

- **suíte de isolamento (padrão M99/M135)** — scripts que sobem um cluster real, exercem a superfície, e provam MVCC/crash/VACUUM contra o **binário shipado** (`cargo pgrx install`), não um mock.
- **probe de threads #153** — instrumentação que registra qual `ThreadId` chama cada método do `Directory` do Tantivy, provando que os métodos que tocam PG (flush/load via SPI) só rodam na main thread; os workers do Tantivy só tocam o `MemStore` em RAM.
- **flush-sob-merge** — o risco residual #153: se o Tantivy fizer merge de segmentos durante um flush concorrente, a consistência precisa valer. O probe + o crash test cobrem a disciplina.
- **consumidor (theo-lens)** — o cliente real que passa a buscar traces via `bm25_search` em vez de `ts_rank`.

### Architecture boundaries affected

Mantém as fronteiras M140.2/M140.3: o probe é lógica pura no núcleo pgrx-free (`lexical_core`); as suítes são
scripts de teste (fora do crate); o consumidor cruza o boundary de repo (theo-lens → TheoDB via SQL). Nenhum
boundary de produção novo em `theodb_rs`.

## Prior Art & Related Work

- **Internal:** `theodb_rs/isolation/crash.sh` (o padrão crash-replay do M99), `scripts/m139-lexical-crash-smoke.sh` (crash do spike — a base a estender p/ a engine de produção), `scripts/m140-3-bm25-smoke.sh` (o MVCC de 2 sessões a formalizar), ADR-0051/0052/0053/0054 (a cadeia M139→M140.3), issue **#153** (os follow-ups).
- **Reference:** theo-lens `trace-read-repository.ts:329-365` (o `ts_rank`/`websearch_to_tsquery` a espelhar/migrar); M99/M135 (o padrão isolamento+crash contra o binário shipado).
- **Skill de patterns:** nenhuma `skills/*-patterns/` casa — verificado.
- **External:** `dogfood-golden-rule.md` (o anchor do M141 que este alimenta); Tantivy 0.26 `merge`/`IndexWriter` docs (a semântica de merge de segmentos).

## Objective

- [ ] Sub-goal 1 — crash+replay da engine BM25 de produção: um índice `bm25_build` COMMITADO sobrevive a SIGABRT + restart (WAL replay).
- [ ] Sub-goal 2 — VACUUM: após rebuild (segmentos antigos deletados), `VACUUM theodb.lexical_files` recupera espaço sem corromper; a busca segue correta.
- [ ] Sub-goal 3 — MVCC formalizado na suíte de isolamento (leitor com snapshot antigo não vê o build novo) + o hardening SPI-co-location do M140.3 LOW.
- [ ] Sub-goal 4 — probe de threads #153: `probe_directory_threads` prova que os métodos que tocam PG só rodam na main thread; regressão de CI.
- [ ] Sub-goal 5 — consumidor theo-lens: span-data real do theo-lens indexado via `bm25_build` + busca via `bm25_search` retorna os traces corretos (evidência), + o caminho bm25 aditivo no `trace-read-repository.ts`.
- [ ] Sub-goal 6 — ADR-0055 (modelo de robustez + o boundary consumidor-proof vs M141) + report com as 3 evidências.

## ADRs

### D1 — Robustez provada contra o BINÁRIO SHIPADO (padrão M99/M135), não um mock

- **Decision:** as provas de MVCC/VACUUM/crash rodam contra a extensão instalada (`cargo pgrx install`) num
  cluster real que sobe/cai — `scripts/m140-4-lexical-robustness.sh`, espelhando `isolation/crash.sh`.
- **Rationale:** o padrão da casa (M99/M135): "green unit suite is not enough" — a robustez só vale provada no
  binário que ships. O M139 já provou crash do spike; M140.3 provou MVCC do smoke; M140.4 consolida os três
  (crash+VACUUM+MVCC) da engine de PRODUÇÃO no binário shipado.
- **Alternatives considered:** pg-tests via `cargo pgrx test` — rejeitado: não linka neste ambiente (M139/M140.3);
  a validação da camada pgrx é via extensão instalada + SQL (o mesmo do CI cassert). Um mock de MVCC — rejeitado
  (não prova o binário real).
- **Consequences:** a robustez é reivindicável com evidência real; o script vira regressão (roda no e2e-runner / CI).

### D2 — Probe de threads #153 como lógica pura no núcleo pgrx-free + assert de main-thread

- **Decision:** um `SegmentStore` de probe (ou um wrapper do `PgDirectory`) que registra o `ThreadId` de cada
  chamada; um teste (`cargo test` stock) prova que, num index+search real do Tantivy, as chamadas do `Directory`
  vêm de múltiplas threads MAS o `MemStore` (o único store no caminho das threads) nunca toca PG — o flush/load
  (SPI) só é chamado da main thread (registrado como a thread do teste).
- **Rationale:** materializa a disciplina #153 (hoje convenção) em teste. Vive no núcleo pgrx-free → testável
  stock + zero pgrx (o probe não precisa de PG; prova a SEPARAÇÃO estrutural).
- **Alternatives considered:** um probe in-PG (pg-test) — rejeitado: não linka + o ponto é a separação estrutural,
  provável melhor no núcleo puro. Só review manual — rejeitado (não é regressão).
- **Consequences:** o CI (`lint-rust.yml`, que já roda o teste do núcleo) pega uma regressão que ponha SPI numa
  worker thread; o `panic="unwind"` (o outro item #153) já é gateado pelo M140.2 review.

### D3 — Consumidor theo-lens: PROOF de integração + caminho aditivo; o cutover de 30 dias é o M141

- **Decision:** M140.4 entrega (a) um proof executável (`scripts/m140-4-consumer-theolens.sh`) — span-data no
  formato do theo-lens (input_value||output_value por span, keyed por trace) indexado via `bm25_build` + busca via
  `bm25_search` retornando os traces corretos; e (b) um caminho bm25 **aditivo** (feature-flag) no
  `trace-read-repository.ts` do theo-lens, sem remover o `ts_rank` default. O **cutover de produção + os 30 dias**
  são o M141 (dogfood `running`).
- **Rationale:** o DoD pede "primeiro consumidor real, com evidência" que "alimenta o anchor do M141" — não os 30
  dias (que são explicitamente o M141). O proof + o caminho aditivo provam que a engine serve o consumidor sem
  quebrar o default; o M141 faz o cutover e mede o uso sustentado.
- **Alternatives considered:** cutover total do theo-lens agora — rejeitado: é o escopo do M141 (dogfood), exige o
  theo-lens rodando contra TheoDB por 30 dias; fazê-lo aqui colapsaria M140.4 e M141. Não tocar o theo-lens —
  rejeitado: o DoD pede o consumidor real, não só o proof isolado.
- **Consequences:** o anchor do M141 fica preparado (o caminho existe + o proof); o `ts_rank` segue default até o
  cutover medido (honestidade — não reivindicar produção antes do M141).

### D4 — Hardening do M140.3 LOW: co-localizar `read_generation` + `load` num snapshot

- **Decision:** ler a geração e carregar o heap sob o MESMO snapshot (um `Spi::connect` compartilhado OU
  confirmar/forçar `read_only` nas leituras), fechando o straddle sob READ COMMITTED que o review do M140.3 apontou.
- **Rationale:** torna a invariante tag==conteúdo airtight independente do nível de isolamento e do flag do pgrx —
  o review pediu; o M140.4 (robustez) é o lugar.
- **Alternatives considered:** deixar como está (auto-cura) — rejeitado: o milestone de robustez deve fechar o gap.
- **Consequences:** o cache é MVCC-airtight sob RC e RR; provado pela suíte de isolamento.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| flush-sob-merge em escala pode ter inconsistência residual (#153) | Medium | o crash test + o probe cobrem a disciplina; merge concorrente é testado no crash script; escala bilhão é fora de escopo | dev |
| VACUUM do heap `lexical_files` pode não recuperar espaço se as tuplas antigas ainda visíveis | Low | o rebuild DELETA as linhas antigas (M140.3); VACUUM as recupera após o snapshot mais antigo passar; testar | dev |
| O consumidor theo-lens exige TheoDB (theodb_rs) no PG dele — não é o PG plain de hoje | Medium | o proof roda contra TheoDB (extensão instalada); o caminho no theo-lens é feature-flag (não quebra o default); cutover=M141 | dev |
| O hardening SPI (D4) pode alterar sutilmente o comportamento do cache | Medium | re-rodar o smoke MVCC de 2 sessões + a suíte de isolamento após o hardening | dev |
| O probe de threads pode ser frágil (depende do scheduling do Tantivy) | Low | o probe assere a SEPARAÇÃO (nenhum SPI em worker), não um nº fixo de threads; robusto a scheduling | dev |

## Unresolved Questions

- Q1 — o pgrx 0.16.1/0.19 usa `read_only=true` em `Spi::get_one`? → resolver no D4/T1 verificando empiricamente + forçando o snapshot compartilhado se preciso.
- Q2 — o VACUUM do heap `lexical_files` sob um índice com rebuilds recupera espaço mensuravelmente? → medir no T1 (pg_total_relation_size antes/depois de VACUUM).
- Q3 — o caminho bm25 no theo-lens deve ser um método novo (`listTracesBm25`) ou um branch dentro de `listTraces`? → método/branch aditivo feature-flagged; resolver no T3 (o menor que não toca o default).

## Dependency Graph

```
Phase 1 (robustez: crash+VACUUM+MVCC + hardening SPI) ──▶ Phase 3 (consumidor theo-lens: proof + caminho)
Phase 2 (probe de threads #153) ─────────────────────────▶ Phase 4 (ADR-0055 + report)
                                                                 ▲
Phase 1, Phase 2 ───────────────────────────────────────────────┘
```

Phase 1 e Phase 2 são independentes (paralelizáveis). Phase 3 depende do binário robusto (Phase 1). Phase 4 consolida.

---

## Phase 1: Robustez — crash + VACUUM + MVCC contra o binário shipado

**Objective:** provar MVCC/VACUUM/crash da engine de produção no binário shipado + fechar o LOW SPI-co-location.

### T1.1 — Hardening SPI-co-location (M140.3 LOW) + suíte de robustez

#### Objective
Fechar o straddle SPI (D4) e escrever `scripts/m140-4-lexical-robustness.sh` provando crash+replay, VACUUM, e MVCC.

#### Why this step (action + reasoning)
1. **What this step does** — co-localiza `read_generation`+`load` num snapshot (D4) e cria a suíte de robustez.
2. **Why it is necessary now** — é o coração do milestone ("provados"); o LOW do M140.3 review é rastreado aqui.

#### Evidence
`engine.rs:151,158` (as 2 leituras SPI a co-localizar), `isolation/crash.sh` (o padrão), `m140-3-bm25-smoke.sh` (o MVCC a estender), review M140.3 (o LOW).

#### Files to edit
```
theodb_rs/src/lexical/engine.rs — co-localiza read_generation + open_from_heap num snapshot (D4)
scripts/m140-4-lexical-robustness.sh — (NEW) crash+replay + VACUUM + MVCC contra o binário shipado
```

#### Deep file dependency analysis
- `engine.rs`: o hardening é interno ao `bm25_search` (ler geração + build sob o mesmo `Spi::connect`/read_only); a assinatura pública não muda.
- `m140-4-lexical-robustness.sh`: espelha `crash.sh`/`m139-crash-smoke` — install, cluster real, build+commit, SIGABRT, restart, verify; + VACUUM; + MVCC 2-sessões.

#### Deep Dives
- Hardening (D4): garantir que `read_generation` e o `load` do build_fn vejam o MESMO snapshot. Opção A: um `Spi::connect` que lê a geração E (se rebuild) carrega. Opção B: confirmar `read_only=true` (usa o snapshot da txn). Invariante: tag do cache == conteúdo carregado, independente de RC/RR.
- Crash: `bm25_build` COMMITADO → SIGABRT → restart → replay → `bm25_search` retorna os mesmos ids. (o heap é WAL-logged, M139 gate 3.)
- VACUUM: build → rebuild (deleta segmentos antigos) → `VACUUM theodb.lexical_files` → `pg_total_relation_size` cai → busca segue correta.
- Edge case: crash ANTES do commit = abort (a geração nova nunca fica visível → snapshot vê o índice antigo).

#### Tasks
1. Co-localizar as leituras SPI (D4); re-rodar o smoke MVCC.
2. Escrever a suíte de robustez (crash+VACUUM+MVCC).
3. Rodar no e2e-runner contra o binário shipado.

#### TDD
```
RED:  a suíte de robustez FALHA se o índice não sobreviver ao crash, ou o VACUUM corromper, ou o MVCC vazar
GREEN: hardening SPI + o binário shipado passa: CRASH_OK, VACUUM_OK, MVCC_OK
REFACTOR: None expected
VERIFY: (e2e-runner) bash scripts/m140-4-lexical-robustness.sh
```

#### Concurrency tests

**MVCC 2-session concurrent test** (dois backends concorrentes, o padrão do M140.3 smoke): **happens-before observation** via a barreira de COMMIT — a sessão A estabelece o snapshot antes de B commitar; a asserção é sobre o estado observado-após (A não vê o build de B). O crash test exercita a durabilidade sob interrupção (SIGABRT no meio): o crash+replay prova que um estado COMMITADO sobrevive; o pré-commit é abort. É um concurrent test (2 backends) + cancellation implícita (crash = interrupção do build em voo).

#### Acceptance Criteria
- [ ] `bash scripts/m140-4-lexical-robustness.sh | grep -c CRASH_OK` retorna 1 (índice sobrevive a SIGABRT+replay).
- [ ] `bash scripts/m140-4-lexical-robustness.sh | grep -c VACUUM_OK` retorna 1 (`pg_total_relation_size` cai após VACUUM; busca correta).
- [ ] `bash scripts/m140-4-lexical-robustness.sh | grep -c MVCC_OK` retorna 1 (leitor snapshot antigo não vê o build novo, sob RC e RR).
- [ ] `bash scripts/m140-3-bm25-smoke.sh | grep -c OK` retorna 9 (M140.3 sem regressão pós-hardening SPI).

#### DoD
- [ ] `bash scripts/m140-4-lexical-robustness.sh` exit code 0 (CRASH_OK + VACUUM_OK + MVCC_OK) no e2e-runner.
- [ ] `cargo check --features "pg18 spike-lexical"` exit code 0.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 2: Probe de threads #153 (regressão de CI)

**Objective:** materializar a disciplina de thread-safety #153 como teste stock no núcleo pgrx-free.

### T2.1 — `probe_directory_threads` no núcleo

#### Objective
Um teste que instrumenta o `Directory` do Tantivy, registra os `ThreadId` das chamadas num index+search real, e
prova que o store no caminho das threads (`MemStore`) é pgrx-free (nenhum SPI/pg_sys numa worker thread).

#### Why this step (action + reasoning)
1. **What this step does** — cria `lexical_core/src/probe.rs`: um `SegmentStore` wrapper que registra `ThreadId`, e um teste que roda um build multi-thread do Tantivy e assere a separação.
2. **Why it is necessary now** — o item #153 (probe→CI); sem ele, uma regressão que ponha SPI numa worker thread passaria silenciosa.

#### Evidence
issue #153 (o item probe→CI), `lexical_core/src/lib.rs` (o `SegmentStore`/`PgDirectory` a instrumentar), M139 review (o probe descobriu que o Tantivy chama de 4 threads).

#### Files to edit
```
theodb_rs/lexical_core/src/probe.rs — (NEW) ThreadRecordingStore + o teste probe_directory_threads
theodb_rs/lexical_core/src/lib.rs — #[cfg(test)] mod probe; (ou pub mod probe atrás de cfg)
```

#### Deep file dependency analysis
- `probe.rs` (NEW) implementa `SegmentStore` envolvendo um `MemStore`, registrando `std::thread::current().id()` num `Mutex<HashSet<ThreadId>>`. O teste roda um `Index::create` + writer multi-thread + search, e assere que (a) as chamadas vieram de >1 thread (o Tantivy usa workers), (b) o store é o `MemStore` puro (nenhum SPI — garantido por ser núcleo pgrx-free, que não linka pgrx).
- Downstream: nenhum; é regressão.

#### Deep Dives
- O probe prova a SEPARAÇÃO ESTRUTURAL: como o núcleo é pgrx-free (não linka pgrx), é **impossível** um `SegmentStore` do núcleo tocar PG de qualquer thread. O teste documenta+trava isso: registra as threads, assere multi-thread, e o fato de compilar no crate pgrx-free É a prova de que nenhum SPI está no caminho.
- Invariante: o caminho das threads do Tantivy toca só memória (o `MemStore`); o flush/load (pgrx) é main-thread (fora do núcleo).
- Edge case: o Tantivy pode usar 1 thread se o corpus for minúsculo → o teste usa docs suficientes p/ disparar workers, mas assere ">=1 thread" tolerante (o ponto é a ausência de PG, não o nº).

#### Tasks
1. RED test (o probe registra threads; assere separação).
2. Implementar `ThreadRecordingStore` + o teste.
3. REFACTOR: None.

#### TDD
```
RED:  test_directory_calls_are_pgrx_free_across_threads() — build multi-thread; store registra threads; nenhum pgrx no caminho (garantido por compilar no núcleo)
RED:  test_probe_records_multiple_threads_on_larger_corpus() — corpus maior dispara workers; >1 thread registrada
GREEN: implementar probe.rs
REFACTOR: None expected
VERIFY: cd theodb_rs && cargo test -p theodb_lexical probe
```

#### Concurrency tests

Este é um **concurrent test** por construção: um `Mutex<HashSet<ThreadId>>` registra as threads reais que chamam o `Directory` durante um build multi-thread do Tantivy (workers reais escrevendo no set sob `Mutex` — **atomic-counter invariant** sobre o conjunto de `ThreadId`s). O assert é sobre a SEPARAÇÃO estrutural (o store é pgrx-free → nenhuma thread pode tocar PG). Race-aware: múltiplas threads reais do Tantivy, sincronizadas pelo `Mutex` do `HashSet`.

#### Acceptance Criteria
- [ ] `cargo test -p theodb_lexical probe` exit code 0.
- [ ] O teste registra ≥1 `ThreadId` chamando o `Directory` num build real (documenta que o Tantivy chama de threads).
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0 (o probe não introduz pgrx — a prova estrutural).
- [ ] `wc -l theodb_rs/lexical_core/src/probe.rs` ≤ 140.

#### DoD
- [ ] `cd theodb_rs && cargo test -p theodb_lexical probe` exit code 0.
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 3: Consumidor theo-lens (proof + caminho aditivo)

**Objective:** provar que o theo-lens consome a busca BM25 (evidência) + o caminho aditivo no repo do theo-lens.

### T3.1 — Proof do consumidor + caminho bm25 aditivo no theo-lens

#### Objective
`scripts/m140-4-consumer-theolens.sh`: carrega span-data no formato do theo-lens (input||output por span, keyed por
trace) numa TheoDB, `bm25_build`, `bm25_search`, prova que retorna os traces corretos. + um caminho bm25 aditivo
(feature-flag) no `trace-read-repository.ts` (sem remover o `ts_rank` default).

#### Why this step (action + reasoning)
1. **What this step does** — o proof executável do consumidor + o hook no theo-lens.
2. **Why it is necessary now** — o DoD pede o "primeiro consumidor real com evidência"; alimenta o anchor do M141.

#### Evidence
`trace-read-repository.ts:329-365` (o `ts_rank`/`search_tsv` a espelhar — o corpus = input_value||output_value do span), `m140-3-bm25-smoke.sh` (o padrão de proof SQL).

#### Files to edit
```
scripts/m140-4-consumer-theolens.sh — (NEW) span-data theo-lens-shaped → bm25_build → bm25_search → traces certos
../theo-lens/packages/core/src/infrastructure/db/observability/trace-read-repository.ts — caminho bm25 aditivo (feature-flag), ts_rank default intocado
```

#### Deep file dependency analysis
- O script cria uma tabela `spans(trace_id, body)` com body = input||output (o mesmo corpus do `search_tsv` do theo-lens), `bm25_build`, e busca por um termo distintivo → assere o trace_id certo.
- O `trace-read-repository.ts`: um branch `opts.lexicalEngine === 'bm25'` que usa `bm25_search` em vez do `ts_rank` (feature-flag; o default `ts_rank` intocado — honestidade, cutover=M141).

#### Deep Dives
- Corpus do proof: mesmo shape do theo-lens (span.input_value || span.output_value por trace). Indexar via `bm25_build` (id=trace hash), buscar via `bm25_search`, assere o trace certo no topo.
- O caminho no theo-lens é ADITIVO: uma flag; o `ts_rank` continua default até o M141 medir o cutover.
- Edge case: o theo-lens roda contra PG plain hoje → o caminho bm25 só ativa quando o DB tem theodb_rs (feature-flag + guard); não quebra o default.

#### Tasks
1. Escrever o proof SQL do consumidor.
2. Adicionar o caminho bm25 aditivo no trace-read-repository (feature-flag).
3. Rodar o proof no e2e-runner.

#### TDD
```
RED:  o proof FALHA se bm25_search não retornar o trace certo p/ o termo distintivo
GREEN: o proof passa: CONSUMER_OK (trace certo no topo)
REFACTOR: None expected
VERIFY: (e2e-runner) bash scripts/m140-4-consumer-theolens.sh
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `bash scripts/m140-4-consumer-theolens.sh | grep -c CONSUMER_OK` retorna 1 (o trace certo é retornado p/ um termo distintivo do span).
- [ ] `grep -c ts_rank ../theo-lens/packages/core/src/infrastructure/db/observability/trace-read-repository.ts` ≥ 1 (o `ts_rank` default intocado; o bm25 é aditivo).
- [ ] O proof usa o MESMO shape de corpus do theo-lens (`body = input_value || output_value` do span).

#### DoD
- [ ] `bash scripts/m140-4-consumer-theolens.sh` exit code 0 (CONSUMER_OK) no e2e-runner.
- [ ] O caminho aditivo no theo-lens não remove o `ts_rank` default.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 4: ADR-0055 + report

**Objective:** consolidar o modelo de robustez + o boundary do consumidor num ADR + report com as 3 evidências.

### T4.1 — ADR-0055 + report

#### Objective
`docs/adr/0055-m140-4-lexical-robustness-consumer.md` (modelo de robustez contra o binário shipado + o boundary
consumidor-proof-vs-M141) e `docs/benchmarks/m140-4-robustness-consumer.md` (as 3 evidências: crash/VACUUM/MVCC,
probe, consumidor).

#### Why this step (action + reasoning)
1. **What this step does** — documenta o veredito e as evidências.
2. **Why it is necessary now** — fecha o M140; o M141 (dogfood) constrói sobre o anchor.

#### Evidence
os 3 artefatos das Phases 1-3, issue #153, `dogfood-golden-rule.md` (o anchor do M141).

#### Files to edit
```
docs/adr/0055-m140-4-lexical-robustness-consumer.md — (NEW)
docs/benchmarks/m140-4-robustness-consumer.md — (NEW)
```

#### Deep file dependency analysis
- Documentos; M141 (dogfood) cita.

#### Deep Dives
- ADR: modelo de robustez (binário shipado, D1), o probe (D2), o boundary consumidor (D3: proof agora, cutover M141), o hardening SPI (D4). Alternativas + consequências.
- Report: as 3 evidências (crash/VACUUM/MVCC OK, probe OK, consumidor OK) + o boundary honesto (M140.4 = engine provada+consumida; os 30 dias = M141).
- Edge case: se alguma evidência não fechar → honest-negative, não maquiar.

#### Tasks
1. Escrever o ADR + report a partir das 3 evidências.

#### TDD
```
RED:  (docs — check_xrefs resolve as citações)
GREEN: escrever ADR + report
REFACTOR: None expected
VERIFY: python3 .claude/scripts/check_xrefs.py 2>&1 | tail -3
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `grep -cE "Alternatives|Consequências|M141" docs/adr/0055-m140-4-lexical-robustness-consumer.md` ≥ 3 (Decisão + alternativa + consequências + boundary M141).
- [ ] `grep -cE "CRASH_OK|VACUUM_OK|MVCC_OK|probe|CONSUMER_OK" docs/benchmarks/m140-4-robustness-consumer.md` ≥ 3 (as 3 evidências com outputs reais).
- [ ] `python3 .claude/scripts/check_xrefs.py` retorna Overall PASS.

#### DoD
- [ ] ADR + report escritos a partir das evidências reais.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Coverage Matrix

| # | Gap / Requirement (DoD ROADMAP M140.4) | Task(s) | Resolution |
|---|---|---|---|
| 1 | MVCC, VACUUM e crash-safety provados pelas suítes de isolamento + crash contra o binário shipado | T1.1 | `m140-4-lexical-robustness.sh` (CRASH_OK+VACUUM_OK+MVCC_OK) no binário shipado |
| 2 | Disciplina de thread-safety #153: probe no CI; nenhum toque em pg_sys/SPI no caminho das threads; panic=unwind gateado | T2.1 | `probe_directory_threads` (núcleo pgrx-free → separação estrutural provada) + panic=unwind já gateado (M140.2) |
| 3 | theo-lens consome a busca BM25 (migra de ts_rank) — primeiro consumidor real com evidência; alimenta o M141 | T3.1 | proof `m140-4-consumer-theolens.sh` (CONSUMER_OK) + caminho bm25 aditivo no trace-read-repository; cutover=M141 (D3) |
| 4 | Hardening do M140.3 LOW (SPI co-location) | T1.1 | co-localização das 2 leituras SPI num snapshot (D4) |

**Coverage: 4/4 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] `cd theodb_rs && cargo test -p theodb_lexical` exit code 0 (cache + probe, stock); `cargo tree` zero-pgrx.
- [ ] (e2e-runner) `bash scripts/m140-4-lexical-robustness.sh` exit 0 (CRASH_OK+VACUUM_OK+MVCC_OK, binário shipado).
- [ ] (e2e-runner) `bash scripts/m140-4-consumer-theolens.sh` exit 0 (CONSUMER_OK).
- [ ] `cargo check --features "pg18 spike-lexical"` + `cargo build` (default) + `cargo clippy -D warnings` exit 0.
- [ ] O smoke M140.3 segue 9/9 após o hardening SPI (sem regressão).
- [ ] File-size budget respeitado (probe ≤ 140 LoC; ADR ≤ 500).
- [ ] CHANGELOG.md atualizado sob `[Unreleased]`.
- [ ] Backward compat — o núcleo pgrx-free; o `ts_rank` do theo-lens intocado (default); M140.3 verde.
- [ ] Plan-specific: ADR-0055 + report com as 3 evidências reais.
- [ ] Plan archived após merge.

## Failure scenarios (I/O external)

`bm25_build`/`bm25_search` (SPI) + o crash test (cluster real) + o consumidor (SQL).

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| PostgreSQL (cluster) | crash real (SIGABRT) no meio | `m140-4-lexical-robustness.sh` kill -ABRT + restart | índice COMMITADO sobrevive (WAL replay); crash pré-commit = abort (índice antigo visível) |
| heap `lexical_files` (VACUUM) | espaço morto após rebuild | build → rebuild → VACUUM | `pg_total_relation_size` cai; busca segue correta (sem corrupção) |
| PostgreSQL (SPI, RC) | commit concorrente de geração entre read_generation e load | o hardening D4 (snapshot co-localizado) | tag do cache == conteúdo; nunca serve versão errada (airtight sob RC e RR) |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validar as 3 evidências ponta-a-ponta no binário shipado.

### Execution
```
cd theodb_rs && cargo test -p theodb_lexical              # cache + probe (stock)
cargo tree -p theodb_lexical | grep -c pgrx               # 0
# e2e-runner (pgrx 0.19 + PG18, cargo pgrx install):
bash scripts/m140-4-lexical-robustness.sh                 # CRASH_OK + VACUUM_OK + MVCC_OK
bash scripts/m140-3-bm25-smoke.sh                         # 9/9 (sem regressão pós-hardening)
bash scripts/m140-4-consumer-theolens.sh                  # CONSUMER_OK
cargo check --features "pg18 spike-lexical" && cargo build && cargo clippy --features "pg18 spike-lexical" --no-deps -- -D warnings
```

### Acceptance Criteria
- [ ] `cargo test -p theodb_lexical` verde (cache + probe); zero-pgrx.
- [ ] robustez: CRASH_OK + VACUUM_OK + MVCC_OK no binário shipado.
- [ ] M140.3 smoke 9/9 (sem regressão).
- [ ] consumidor: CONSUMER_OK.
- [ ] check spike+default + clippy RC=0.

### If Validation Fails
1. Separar falhas do plano vs pré-existentes.
2. Priorizar o crash/VACUUM (a robustez é o core do milestone) e o hardening SPI (o airtight MVCC).
3. Re-rodar a cadeia.
