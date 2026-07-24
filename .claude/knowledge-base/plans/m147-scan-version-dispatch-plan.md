---
slug: m147-scan-version-dispatch
milestone_id: M147
created_at: 2026-07-24
goal: Refatorar am/scan.rs em 3 eixos (enum de versão, Result+?, kernel Stage-1 compartilhado) com A/B in-PG provando resultado byte-idêntico nos 6 caminhos IVF v3/v4/v5/v6/v7/v8
---

# Plano: M147 — Refactor version-dispatch de `am/scan.rs` (enum + Result+? + Stage-1 compartilhado)

> **v1.1** (2026-07-24) — MUST-FIX EC-1/EC-2/EC-3 do edge-case-plan absorvidos: o A/B cobre **6 caminhos (v3..v8)**, não 5 (o refactor de dispatch afeta o v3, que hoje é o fallback do if-ladder); `ivf_version` é **estrito** (versão desconhecida → `Err`) e checa `len >= 8` antes de ler [4..8] (evita panic XX000, regressão do M146); o A/B usa um baseline **capturado** num arquivo committado, não dois binários vivos.

## Goal

Refatorar `theodb_rs/src/am/scan.rs` em três eixos — (1) if-ladder de versão IVF → `enum IvfVersion` lido uma vez, (2) 8 gather helpers `Vec` → `Result + ?`, (3) kernel Stage-1 `ah_score_block` compartilhado in-memory — com **métrica observável: A/B in-PG provando que o top-k retornado (ids + distâncias) é byte-idêntico ao baseline nos 6 caminhos v3/v4/v5/v6/v7/v8**, sem violar a ADR-2 do M145.

## Context

O issue #170 é consenso de 5 pilares sobre três hotspots em `am/scan.rs` (hoje 1567 LoC): a if-ladder `ivf_is_v4/v5/v6/v7/v8` (`scan.rs:535`, dentro de `scan_ivf_structured`) emite até 5 releituras redundantes do bloco 0; ~46 `match { Ok=>v, Err=>err_* }` C-style vivem nos 8 gather helpers; e o kernel Stage-1 (`ah_score_block`, `vec/ah.rs:375`) é copiado byte-a-byte em 5 corpos `scan_ivf_aq_*`. O M147 refatora os três **preservando comportamento byte-idêntico**.

O blueprint de discovery (`.claude/knowledge-base/discoveries/blueprints/m147-scan-version-dispatch-blueprint.md`, SHIPPABLE_WITH_CAVEATS 89) fixou o padrão a partir de pgvectorscale (dispatch OCP por enum, tipo lido uma vez, decode isolado por-impl), lance (isolamento de corpos em módulo `previous/` + recusa fail-closed = a ADR-2 na prática), e pgvector (o contraste single-version). As 3 recomendações do blueprint mapeiam 1:1 aos 3 bullets do DoD.

A restrição inquebrável: **ADR-2 do M145** (`theodb_rs/src/am/page/mod.rs:571`) — os corpos de decode on-disk permanecem separados; unificá-los arrisca misparse→data-loss. O refactor compartilha só o kernel **in-memory** (Stage-1 scoring), recebendo `codes_off` (conhecimento on-disk por-versão) como parâmetro do chamador — nunca recomputa.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | git sha | Papel | O que muda |
|---|---|---|---|---|
| `theodb_rs/src/am/scan.rs` | 1567 | 19efcbe | dispatch + 8 gather helpers + 5 corpos IVF-AQ | if-ladder→enum; gathers→Result+?; Stage-1 hoistado |
| `theodb_rs/src/am/page/ivf.rs` | 1212 | 03fdab6 | decode on-disk + os 5 predicados `ivf_is_v*` | os predicados são substituídos por 1 função `ivf_version(rel)`; **corpos de decode INTOCADOS (ADR-2)** |
| `theodb_rs/src/vec/ah.rs` | 413 | c6025d3 | `ah_score_block`/`build_lut16` (o kernel) | **intocado** — o kernel já é a função certa; o M147 só o CHAMA de um lugar |
| `theodb_rs/examples/ivf_codec_check.rs` | ~150 | 19efcbe | teste-que-executa (convenção do projeto) | ganha teste da lógica pura do dispatch (se extraível sem pg_sys) |

### Current callers / dependents

- `scan_ivf_structured` (`scan.rs:535`) — chamado em `scan.rs:231` (`amrescan`, dispatch top-level após `peek_magic`) e `scan.rs:1486` (re-search iterativo M87). **2 callers de produção.**
- `ivf_is_v4/v5/v6/v7/v8` (`page/ivf.rs:213,369,379,745,755`) — chamados SÓ no if-ladder de `scan_ivf_structured` (`scan.rs:545-563`). **1 caller cada.** Após o refactor, substituídos por `ivf_version(rel)`.
- Os 8 gather helpers (`gather_hnsw_candidates:308`, `gather_symqg_candidates:376`, `scan_ivf_structured:535`, `scan_ivf_aq:653`, `scan_ivf_aq_split:766`, `scan_ivf_aq_split_v7:901`, `scan_ivf_aq_split_sq8:1082`, `scan_ivf_aq_split_rabitq:1191`) — chamados a partir de `amrescan` (`scan.rs:231,247,294`) e do re-search. **Mudança de assinatura `Vec`→`Result<Vec,_>` propaga para esses call-sites.**
- `ah_score_block` (`vec/ah.rs:375`) — chamado nos 5 corpos IVF-AQ (5 call-sites). **Intocado**; o refactor move a *chamada* para o kernel compartilhado.

### Architecture boundaries affected

Per `.claude/rules/architecture.md § 1`: o decode on-disk (`page/ivf.rs`) é camada **adapter**; o scoring in-memory (`vec/ah.rs`) é **domain**. O refactor respeita a direção: o kernel Stage-1 compartilhado (domain) recebe bytes já decodados + `codes_off` (calculado pelo adapter), nunca chama o adapter de volta.

### Domain glossary

- **codes_off** — offset em bytes onde começam os códigos AQ dentro do blob da lista, calculado por-versão (v4 `8n+entry_f32·n`; v5/v6/v8 `8n`; v7 `8n+n·label_bytes`). É conhecimento ON-DISK.
- **Stage-1** — o scoring rápido via LUT (`ah_score_block`) que produz candidatos aproximados; in-memory.
- **Stage-2** — o rerank exato (f32/SQ8/RaBitQ) que diverge genuinamente por-versão; NÃO compartilhado.
- **IVF-AQ vN** — o formato on-disk IVF com quantização AQ, versão N em bytes [4..8] do bloco 0 (magic `TIVS` compartilhado).

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.19` (workspace-pinned) | rust | Já é a base do `theodb_rs`; o refactor não muda a versão nem adiciona feature. Rule 9: reusa o que existe. |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | — | — | — | O M147 é refactor puro de código próprio (`am/scan.rs`); reusa `ah_score_block`/`build_lut16` (`vec/ah.rs`, já existentes) e o padrão de erro do M146 — **zero dependência nova** (parsimony rung 4: reusar antes de adicionar). |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Prior Art & Related Work

- **Blueprint interno:** `.claude/knowledge-base/discoveries/blueprints/m147-scan-version-dispatch-blueprint.md` — o padrão de dispatch e isolamento (pgvectorscale, lance, pgvector).
- **M145** — a metodologia A/B byte-idêntico + a ADR-2 (corpos on-disk separados). O M147 reusa o A/B e honra a ADR-2.
- **Referências:** pgvectorscale `storage.rs`/`scan.rs` (dispatch OCP), lance `version.rs:62`/`previous/mod.rs` (isolamento).
- **Baseline map:** o agente de baseline mapeou a fronteira decode-on-disk vs scoring-in-memory em cada um dos 5 corpos (registrado no blueprint § Coverage Corner 4).

## ADRs

### D1 — O `enum IvfVersion` mapeia o `u32` já persistido; zero mudança de formato on-disk

**Decision:** substituir os 5 predicados `ivf_is_v*` (cada um relê o bloco 0) por uma única `ivf_version(rel) -> Result<IvfVersion, String>` que lê o bloco 0 **uma vez** e mapeia o `u32` de bytes [4..8] para `enum IvfVersion { V3, V4, V5, V6, V7, V8 }`. O dispatch em `scan_ivf_structured` vira um `match version { … }` exaustivo (sem `_ =>`).

**Rationale:** o discriminante de versão já existe on-disk (bytes [4..8], magic `TIVS` compartilhado). Mapeá-lo num enum não muda nenhum byte persistido — crash/upgrade/VACUUM-safety são preservados por construção (não escrevemos formato novo). Modelo: lance `try_from_major_minor` (bytes→enum centralizado uma vez, `version.rs:62`); pgvectorscale `enum StorageType` com discriminante estável.

**Alternatives considered:** (a) magic distinto por versão — rejeitado: mudaria o formato on-disk, exigiria upgrade script + quebraria índices existentes; (b) manter a if-ladder e só deduplicar a leitura — rejeitado: não resolve o OCP do bullet 1 (adicionar versão continuaria editando um switch). Cita `.claude/rules/parsimony-ladder.md` (per-version = essencial; a deduplicação da LEITURA é acidental removível).

**Consequences:** adicionar uma versão futura passa a ser "adicionar 1 variante ao enum + 1 arm ao match" (OCP). O compilador força atualizar o match (exaustivo).

### D2 — Corpos de decode ficam separados; só o kernel Stage-1 in-memory é compartilhado, recebendo `codes_off`

**Decision:** extrair o loop de bloco + `ah_score_block` num kernel `stage1_score(lut, bytes, codes_off, n, pairs, …) -> Vec<Candidate>` que **recebe `codes_off` do chamador**. Cada corpo `scan_ivf_aq_*` calcula seu `codes_off` (decode on-disk, permanece separado) e chama o kernel. O Stage-2 (rerank) permanece por-versão.

**Rationale:** honra a ADR-2 (corpos separados) + o padrão lance (`previous/` isolado, dispatch decide uma vez) + pgvectorscale (kernel `next<S: Storage>` genérico, decode isolado por-impl). O kernel NUNCA recomputa `codes_off` — se recomputasse, precisaria conhecer o layout por-versão, reintroduzindo o risco misparse que a ADR-2 barra. Cita `.claude/rules/error-handling.md` (fail-fast na fronteira: bounds-check antes do slice).

**Alternatives considered:** (a) kernel que recebe `IvfVersion` e recomputa `codes_off` internamente — rejeitado: viola a ADR-2 (conhecimento on-disk vaza para o kernel domain); (b) não compartilhar (status quo) — rejeitado: é o bullet 3 do DoD (a duplicação é o hotspot do #170). Cita SRP (`.claude/rules/architecture.md § 3`): o kernel tem uma responsabilidade (scoring), o corpo tem outra (decode).

**Consequences:** o v4 é o outlier (defere tid+f32 ao Stage-2 de bytes cacheados) — seu contrato de candidato difere de v5/v6/v7/v8; o kernel aceita isso via parâmetro de política, não via branch de versão.

### D3 — `Result + ?` é refactor interno acima da fronteira C

**Decision:** mudar as 8 assinaturas de gather para `Result<Vec<(i64,f64)>, ScanError>`, converter os ~46 `match { Ok=>v, Err=>err_* }` em `?`, e hoistar a conversão-para-ereport (`err_corrupt`/`err_input`/`lut_error`) para os call-sites de dispatch no `amrescan`.

**Rationale:** provado no blueprint (Q5) independente da versão do pgrx — os callbacks C retornam `bool`/ponteiro; a conversão é Rust-side, acima do callback. Idioma do crate: `columnar_agg.rs:939` (closure-IIFE `Result`). Modelo: pgvectorscale `next<S>` retorna `Option`, callback traduz. Cita `.claude/rules/error-handling.md` (typed errors) — o `?` propaga o erro tipado até UM boundary, em vez de N ereports espalhados.

**Alternatives considered:** (a) manter os `match` C-style — rejeitado: é o bullet 2 (~46 sítios repetitivos são o smell do #170); (b) `panic!` como pgvectorscale — rejeitado explicitamente (blueprint § "NÃO transferir"): panic através de C = XX000 (lição M146).

**Consequences:** um `enum ScanError { Corrupt(String), Input(String) }` (ou reuso do padrão existente) carrega a classe até o boundary, que escolhe `err_corrupt`/`err_input` — preservando a taxonomia XX002/22023 do M146.

## Dependency Graph

```
Fase 1 (enum IvfVersion + dispatch)  ─┐
                                      ├─▶ Fase 3 (kernel Stage-1) ─▶ Fase 4 (A/B validation)
Fase 2 (Result + ? nos gathers)  ────┘
```

Fase 1 e Fase 2 são independentes (tocam pontos diferentes: dispatch vs assinaturas de erro) e podem ser feitas em qualquer ordem. Fase 3 depende de ambas (o kernel compartilhado usa o enum e retorna `Result`). Fase 4 (A/B) depende de tudo.

## Phase 1 — enum IvfVersion + dispatch lido uma vez (bullet 1)

### Task 1.1 — `enum IvfVersion` + `ivf_version(rel)` substituindo os 5 predicados

#### Why this step

**Ação:** criar `enum IvfVersion { V3, V4, V5, V6, V7, V8 }` e `ivf_version(rel) -> Result<IvfVersion, String>` em `page/ivf.rs`, lendo o bloco 0 uma vez e mapeando o `u32` de [4..8]. Substituir a if-ladder de `scan_ivf_structured` (`scan.rs:545-563`) por `match ivf_version(rel)? { … }`.

**Política do mapeamento (EC-2, EC-3):** o `match` é **estrito** — `3=>V3, 4=>V4, 5=>V5, 6=>V6, 7=>V7, 8=>V8, other => Err`. Uma versão desconhecida vira erro tipado (não cai silenciosamente no v3 como o else atual). Antes de ler [4..8], `ivf_version` checa `if m.len() < 8 { return Err("theodb ivf: truncated header") }` — um bloco com magic TIVS mas truncado NÃO pode panicar no `try_into().unwrap()` (seria XX000, regressão do M146). O erro tipado é roteado por `lut_error`/`err_corrupt` no chamador (corrupção → XX002).

**Raciocínio:** a if-ladder faz 5 releituras redundantes do mesmo bloco 0 (cada `ivf_is_v*` relê) — é o hotspot OCP do #170. O padrão comprovado (D1, blueprint Q1/Q2) é ler o discriminante uma vez e materializar num enum. Como o `u32` já está persistido, é zero mudança de formato. O v3 grava `3u32` explícito (`page/ivf.rs:66`), então o mapeamento estrito o reconhece por `3=>V3` — não como fallback (EC-1/EC-2).

#### Files to edit

- `theodb_rs/src/am/page/ivf.rs` — adicionar `enum IvfVersion` + `ivf_version`; deletar os 5 `ivf_is_v*` (dead após o refactor).
- `theodb_rs/src/am/scan.rs` — `scan_ivf_structured` usa `match ivf_version(rel)?`.

#### Deep file dependency analysis

Os `ivf_is_v*` têm 1 caller cada (o if-ladder). `ivf_version` lê os mesmos bytes ([0..4] magic + [4..8] versão) que `peek_magic` (`page/mod.rs:551`) já lê para [0..4] — reuso da leitura de bloco. Nenhum outro módulo referencia `ivf_is_v*` (grep confirmado no Baseline).

#### TDD

RED: `examples/ivf_dispatch_check.rs` (NEW) — a lógica de mapeamento `(u32, len) → Result<IvfVersion>` é extraível como função pura (sem pg_sys — recebe os bytes já lidos), incluída via `#[path]`. Assertar: `map_version(3, 8) == Ok(V3)`, `(4,8)==Ok(V4)`, `(8,8)==Ok(V8)`, `(99,8)==Err` (versão desconhecida — EC-2), `(4, 6)==Err` (bloco truncado < 8 bytes — EC-3). Não-vacuidade: mutar o gate `< 8` para `< 4` faz o teste do bloco truncado falhar.
GREEN: implementar o enum + mapeamento estrito + gate de len.
REFACTOR: garantir match exaustivo sem `_ =>` (só o `other => Err` explícito).

#### Concurrency tests

(none — single-threaded) — `ivf_version` só lê o bloco 0 sob o mesmo `index_shared` que o scan já segura; o refactor não altera a serialização.

#### Acceptance criteria

- `grep -c "ivf_is_v" theodb_rs/src/` = 0 (os 5 predicados removidos).
- `scan_ivf_structured` despacha via `match ivf_version(rel)?` exaustivo (só `other => Err`, sem `_ =>` mudo).
- Um `u32` de versão desconhecida (ex.: 99) retorna `Err` tipado, não panic (EC-2).
- Um bloco com magic TIVS mas < 8 bytes retorna `Err`, não panica no `try_into` (EC-3) — provado por `corrupt_index.sh AM=theodb_ivfflat`.
- `cargo clippy --features pg18 --no-deps -- $(.clippy_args)` exit 0.

#### DoD

- Build no droplet (`cargo pgrx install`) exit 0.
- A validação de comportamento é a Fase 4 (A/B).

## Phase 2 — gather helpers: Result + ? (bullet 2)

### Task 2.1 — 8 assinaturas `Vec` → `Result`, ~46 arms → `?`, boundary no amrescan

#### Why this step

**Ação:** mudar as 8 assinaturas de gather (`scan.rs:308,376,535,653,766,901,1082,1191`) para `Result<Vec<(i64,f64)>, ScanError>`; converter os ~46 `match { Ok=>v, Err=>err_* }` em `?`; hoistar a conversão-para-ereport para os 3 call-sites de dispatch no `amrescan` (`scan.rs:231,247,294`).

**Raciocínio:** ~46 sítios `match` C-style repetidos são o smell de manutenibilidade do #170. O idioma do crate (`columnar_agg.rs:939`) e o padrão pgvectorscale (D3) é propagar `Result` com `?` até UM boundary. A taxonomia XX002/22023 do M146 é preservada porque o `ScanError` carrega a classe (Corrupt vs Input) até o boundary.

#### Files to edit

- `theodb_rs/src/am/scan.rs` — as 8 assinaturas + os arms + os 3 call-sites do `amrescan`; definir `enum ScanError { Corrupt(String), Input(String) }` (ou reusar padrão existente) com um `fn into_ereport(self) -> !`.

#### Deep file dependency analysis

A mudança de assinatura propaga para os call-sites: `amrescan` (`scan.rs:231,247,294`) e o re-search (`scan.rs:1486`). O `lut_error` (`scan.rs:36`) vira uma variante de `ScanError::Input`/`Corrupt` conforme `codebook_dim`. Nenhum símbolo fora de `scan.rs` chama os gathers (são `fn` privados do módulo).

#### TDD

RED: o A/B da Fase 4 é o RED discriminante (o comportamento não pode mudar). Adicionalmente, um teste de que a variante errada de `ScanError` produz o SQLSTATE errado (o M146 já tem os probes de taxonomia no `cassert-smoke.sh` — reusar: dim errada → 22023, corrupção → XX002).
GREEN: converter os arms para `?`.
REFACTOR: um único `match err.into_ereport()` por call-site.

#### Concurrency tests

(none — single-threaded) — a conversão de erro não toca locks nem estado compartilhado.

#### Acceptance criteria

- `grep -c "Err(e) => crate::pg::err_corrupt\|Err(e) => lut_error\|Err(e) => crate::pg::err_input" theodb_rs/src/am/scan.rs` cai de 55 para ≤ 9 (os que ficam fora dos 8 gathers: amrescan + scan_blob).
- O `cassert-smoke.sh` (taxonomia): dim errada → 22023, corrupção de índice → XX002 (preservados do M146).
- `cargo clippy` exit 0; `cargo fmt --check` exit 0.

#### DoD

- Build exit 0; taxonomia preservada (medida no droplet).

## Phase 3 — kernel Stage-1 compartilhado (bullet 3)

### Task 3.1 — extrair `stage1_score` recebendo `codes_off`, chamado pelos 5 corpos

#### Why this step

**Ação:** extrair `fn stage1_score(lut, bytes, codes_off, n, pairs, policy) -> Vec<Candidate>` — o loop de bloco + `ah_score_block` — que **recebe `codes_off` do chamador**. Cada corpo `scan_ivf_aq_*` calcula seu `codes_off` (decode on-disk, permanece separado) e chama o kernel.

**Raciocínio:** o kernel Stage-1 é copiado byte-a-byte em 5 corpos (o hotspot do #170, bullet 3). O padrão (D2, blueprint) é um kernel genérico que NÃO conhece o layout on-disk — recebe `codes_off` já calculado. Isso honra a ADR-2: o decode (que calcula `codes_off`) fica separado por-versão; só o scoring é compartilhado.

#### Files to edit

- `theodb_rs/src/am/scan.rs` — adicionar `stage1_score`; os 5 corpos passam a chamá-lo com seus `codes_off` respectivos.

#### Deep file dependency analysis

Os 5 corpos calculam `codes_off` diferente (mapeado pelo agente de baseline: v4 `8n+entry_f32·n` em `scan.rs:706`; v5 `8n` em `:832`; v7 `8n+n·label_bytes` em `:967`; v6/v8 `8n`). O `ah_score_block` (`vec/ah.rs:375`) é chamado idêntico nos 5 (mesmos 4 args). O v4 difere no Stage-2 (defere tid+f32) — o kernel aceita via `policy` param, não via branch de versão.

#### TDD

RED: o A/B da Fase 4 é o RED discriminante — se o kernel recomputar `codes_off` errado, o top-k diverge e o A/B falha. Adicionalmente, se a aritmética de `codes_off` for extraível (pura), um teste no example: `codes_off_v4(n, entry_f32) == 8*n + entry_f32*n`, etc.
GREEN: implementar o kernel recebendo `codes_off`.
REFACTOR: os 5 corpos ficam mais curtos (só decode + call + Stage-2).

#### Concurrency tests

(none — single-threaded) — o kernel opera sobre bytes já lidos, sem I/O nem lock.

#### Acceptance criteria

- `grep -c "ah_score_block" theodb_rs/src/am/scan.rs` cai de 5 (call-sites diretos) para 1 (dentro do kernel).
- O kernel NUNCA recomputa `codes_off` (grep: `codes_off` não é calculado dentro de `stage1_score`, só recebido).
- `cargo clippy` exit 0.

#### DoD

- Build exit 0; o A/B da Fase 4 prova byte-identidade.

## Phase 4 — Integration Validation (A/B byte-idêntico in-PG)

### Task 4.1 — A/B recall×QPS byte-idêntico nos 6 caminhos v3/v4/v5/v6/v7/v8

#### Why this step

**Ação:** construir um índice IVF em cada versão (**v3, v4, v5, v6, v7, v8** — o v3 incluído porque o refactor de dispatch o afeta, EC-1) com dataset determinístico (`setseed`), **capturar** o top-k do binário BASELINE (build do commit `74fe445`) num arquivo committado, depois rodar as mesmas queries no binário NOVO e diffar contra o arquivo capturado — assertando top-k (ids + distâncias arredondadas) **idêntico** e QPS sem regressão > 5%.

**Raciocínio:** é a métrica do Goal e o DoD bullet 4. O padrão (blueprint Corner 1) combina o scaffold parametrizado do pgvectorscale (`build.rs:1179`) com o `#[rstest] #[values(…)]` do lance. O baseline **capturado num arquivo fixo** (EC-4) segue o padrão "corpus versionado" do lance (`test_data/` + assert de proveniência) — reproduzível, sem a ambiguidade de dois binários vivos. A metodologia A/B byte-idêntico é a do M145.

#### Files to edit

- `theodb_rs/isolation/ab_scan_versions.sh` (NEW) — o harness A/B: constrói cada versão, captura/compara o top-k contra o baseline fixo.
- `docs/benchmarks/m147-ab-baseline.txt` (NEW) — os resultados capturados do baseline `74fe445` (ids+distâncias por versão), committado como referência (EC-4).

#### Deep file dependency analysis

O harness reusa o padrão dos `crash_*.sh`/`corrupt_index.sh` (initdb → CREATE INDEX WITH (…) por versão → query → diff). Cada versão exige as reloptions certas: **v3 f32** (sem `pq_subspaces` — o default AQ-less); v4 `pq_subspaces`; v5 `separate_storage=1`; v6 `+refine=sq8`; v7 `+label column`; v8 `+refine=2`. O baseline é capturado uma vez do binário `74fe445` (que já contém os fixes do M146) antes de qualquer mudança da Fase 1-3.

#### TDD

RED: rodar o A/B contra um binário com um bug deliberado (ex.: `codes_off` recomputado errado no kernel, OU o mapeamento de v3 quebrado) → o diff DEVE acusar divergência (exit 1). Prova de não-vacuidade nos 6 caminhos.
GREEN: com o refactor correto, o diff é vazio nos 6 caminhos (exit 0).
REFACTOR: n/a (é validação).

#### Concurrency tests

(none — single-threaded) — o A/B mede o read-path single-thread; o QPS multi-cliente não é objeto do M147 (comportamento preservado, não performance nova).

#### Failure scenarios

(none — no external I/O touched) — o A/B é in-PG, sem HTTP/DB driver/queue externos.

#### Acceptance criteria

- Para cada **v3/v4/v5/v6/v7/v8**: o top-k (ids + distâncias) do binário novo é **byte-idêntico** ao baseline capturado de `74fe445` (EC-1).
- QPS não regride > 5% em nenhum caminho (medido, mean ± std de ≥ 3 runs).
- Não-vacuidade: um bug deliberado no `codes_off` (ou no mapeamento de v3) faz o A/B acusar divergência.
- `cassert-smoke.sh` (taxonomia + guard #177) verde.

#### DoD

- Os 6 caminhos (v3..v8) byte-idênticos, medido no droplet, registrado em `docs/benchmarks/m147-ab-byte-identical.md`.
- CHANGELOG `[Unreleased]` atualizado.

## Coverage Matrix

| Requisito (DoD do ROADMAP M147) | Task(s) |
|---|---|
| Bullet 1: if-ladder → dispatch-table/enum (OCP) | Task 1.1 |
| Bullet 2: 8 gather helpers → Result+? com UM boundary | Task 2.1 |
| Bullet 3: Stage-1 `ah_score_block` compartilhado in-memory | Task 3.1 |
| Bullet 4: comportamento preservado, A/B byte-idêntico nos 6 caminhos (v3..v8) | Task 4.1 |
| Bullet 5: zero mudança de superfície SQL; CHANGELOG | Task 4.1 (DoD) + todas (nenhuma toca `#[pg_extern]`) |
| ADR-2 preservada (corpos on-disk intocados) | Task 3.1 (D2 — kernel recebe codes_off) |

**Cobertura: 6/6 requisitos mapeados (100%).**

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Owner |
|---|---|---|---|---|
| R1 | O kernel Stage-1 compartilhado recomputa `codes_off` errado → misparse → top-k divergente (data-loss silencioso) | ALTA | D2: `codes_off` é PARÂMETRO do chamador, nunca recomputado no kernel; o A/B byte-idêntico (Task 4.1) acusa qualquer divergência; grep de acceptance confirma que `codes_off` não é calculado dentro do kernel | paulohenriquevn |
| R2 | Regressão de QPS ao hoistar o kernel (indireção de função no hot-path) | MÉDIA | Task 4.1 mede QPS mean±std ≥3 runs; teto de regressão 5%; se exceder, `#[inline]` no kernel ou reverter o hoist daquele corpo (honest-negative aceito, como M138) | paulohenriquevn |
| R3 | O v4 (outlier: defere tid+f32 ao Stage-2) não encaixa no contrato de candidato de v5-v8 | MÉDIA | D2: o kernel aceita a política via parâmetro, não via branch de versão; se o v4 não couber, fica FORA do kernel compartilhado (4 de 5 já é ganho) e isso é declarado | paulohenriquevn |
| R4 | A conversão `Result+?` altera a taxonomia de erro (regressão do M146) | MÉDIA | Task 2.1: o `cassert-smoke.sh` do M146 (probes de dim-errada→22023, corrupção→XX002) roda como gate; o `ScanError` carrega a classe até o boundary | paulohenriquevn |

## Unresolved Questions

- Q1: A lógica de mapeamento `u32→IvfVersion` e a aritmética de `codes_off` são extraíveis como funções puras (sem `pg_sys`) para um teste de example que executa? — **A resolver na Task 1.1/3.1**; se não forem, o A/B da Fase 4 é o único RED discriminante (aceitável, é a convenção do crate).
- Q2: O v4 encaixa no kernel compartilhado ou fica de fora? — **A resolver na Task 3.1 por medição**; ambos os desfechos são aceitáveis (4/5 já fecha o bullet 3 com honestidade declarada).

## Global Definition of Done

- [ ] Os 3 eixos do refactor implementados (Tasks 1.1, 2.1, 3.1).
- [ ] A/B byte-idêntico nos 6 caminhos v3/v4/v5/v6/v7/v8, medido no droplet (Task 4.1), com prova de não-vacuidade.
- [ ] QPS não regride > 5% em nenhum caminho (mean±std ≥3 runs).
- [ ] Taxonomia de erro do M146 preservada (`cassert-smoke.sh` verde).
- [ ] Zero mudança de superfície SQL (nenhum `#[pg_extern]` tocado).
- [ ] Corpos de decode on-disk de `page/ivf.rs` INTOCADOS (ADR-2) — grep confirma.
- [ ] `cargo clippy --features pg18 --no-deps -- $(.clippy_args)` exit 0; `cargo fmt --check` exit 0.
- [ ] Arquivos ≤ 500 LoC de delta por arquivo tocado (o refactor REDUZ `scan.rs`, não aumenta).
- [ ] `/code-quality` verdict ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG `[Unreleased]` atualizado; benchmark em `docs/benchmarks/m147-ab-byte-identical.md`.
