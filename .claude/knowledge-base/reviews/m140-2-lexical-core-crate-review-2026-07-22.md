# Review — M140.2 (crate núcleo lexical pgrx-free) — 2026-07-22

**Verdict:** READY_TO_MERGE

Revisor: `council-rust-pgrx` (a lente de safety Rust/pgrx). Veredito: **a extração é SEGURA e o boundary é
GENUÍNO — ficou mais forte** (a disciplina que era convenção do M139 virou garantia estrutural: o núcleo
literalmente não compila se um tipo pgrx vazar). **Zero BLOCKER/HIGH.** 2 LOW (1 corrigido, 1 registrado) + 1 INFO
(fora de escopo, rastreado p/ M140.3).

## Validação com toolchain real (e2e-runner, pgrx 0.19.0 + PG18)

| Gate | Resultado |
|---|---|
| Núcleo `cargo test -p theodb_lexical` (stock, sem pgrx) | ✅ 6 verdes |
| Gate objetivo pgrx-free: `cargo tree -p theodb_lexical \| grep -c pgrx` | ✅ 0 |
| cdylib consome o núcleo: `cargo check --features "pg18 spike-lexical"` | ✅ 0 erros (Finished 1m16s) |
| Build default (shipado): `cargo check --features pg18` | ✅ 0 erros |
| clippy `-D warnings` (baseline M136), default + spike-lexical | ✅ RC=0 nos dois |
| `cargo fmt --check` | ✅ clean |

## Hard gates (cycle-review.md) — todos ✅

branch=develop · sem `Co-Authored-By` · sem secrets · CHANGELOG atualizado · `code-quality` NOOP (Rust não habilitado; o gate mecânico Rust é o CI M136, validado acima no box).

## Auditoria council-rust-pgrx — 6 itens + disposição

| # | Item auditado | Veredito |
|---|---|---|
| 1 | Boundary pgrx-free genuíno? | ✅ Confirmado: deps diretas do núcleo = só `tantivy`; zero refs pgrx em código (só prosa em doc-comments `lib.rs:7,10`); **zero `unsafe`**; assinaturas públicas só tipos std |
| 2 | Thread-safety do spike preservada? | ✅ E reforçada: `MemStore` = `RwLock<HashMap>` `Send+Sync` inalterado; uma thread worker do Tantivy **não tem como** alcançar o PG a partir do núcleo (não linka pgrx) — antes era code-review, agora é o grafo de deps |
| 3 | `panic="unwind"` garantido no crate separado? | ✅ Sólido por 3 razões: (a) `[profile]` do workspace root (`theodb_rs`) aplica a todo o grafo incl. o membro; (b) `unwind` é default de qualquer forma; (c) o test harness força unwind. `lexical_core` não tem `[profile]` próprio → sem conflito |
| 4 | Reconciliação com ADR-0009 sólida? | ✅ Não é reversão: ADR-0009 restringe a camada de `#[pg_extern]`; o núcleo tem zero externs → outra camada (DIP) |
| 5 | Feature wiring correto? | ✅ `mod lexical` é `#[cfg(feature="spike-lexical")]` → default não puxa tantivy; **uma** tantivy 0.26.1 no tree |
| 6 | 6 testes byte-idênticos? | ⚠️ `git` classifica `R091` (rustfmt reflow), **não** byte-idêntico — mas **semanticamente idêntico** (6 testes presentes, assertions inalteradas). Ver LOW-2 |

## Findings e disposição

| Sev | Finding | Disposição |
|---|---|---|
| — | BLOCKER/HIGH: nenhum | — |
| LOW-1 | ADR-0053:33 dizia que os `lexical_spike_*` estão sob `mod theodb_rs` de `api.rs` — na verdade são externs **bare top-level** (schema `public`), herança do spike M139 | **CORRIGIDO** — frase reescrita; a reconciliação (núcleo tem zero externs) segue válida |
| LOW-2 | Claim "movido verbatim / byte-idêntico" (no commit message + plano) é impreciso — o `git mv` fez `R091` (rustfmt reflow) | **REGISTRADO** — os docs commitados (ADR/CHANGELOG) **não** alegam byte-idêntico; só o commit message (imutável). Termo preciso: **behavior-idêntico, reformatado pelo rustfmt**. Sem reescrever história |
| INFO | `.expect()`/`.unwrap()` densos no boundary de `pg_backing.rs` (converte erro Tantivy/SPI em panic) | ACEITO — código de spike **pré-existente** (M139), fora do escopo da extração; sob `panic="unwind"` desenrola→`ereport` (seguro). **Rastreado p/ M140.3**: na promoção da superfície permanente, virar `Err` tipado→`pg::error!` (padrão `error-handling.md`) |

## DoD do milestone (ROADMAP M140.2) — verificação

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | Crate núcleo sem dep pgrx; testes puros rodam em `cargo test` | ✅ | `theodb_lexical` (dep só tantivy), 6 testes stock, `cargo tree` zero-pgrx |
| 2 | ADR-1 reconcilia com o ADR-0009 | ✅ | `docs/adr/0053` (núcleo zero externs → outra camada) |
| 3 | `theodb_rs` consome o núcleo atrás da feature; build shipado + CI verdes | ✅ | workspace + dep sob spike-lexical; `cargo check` spike+default RC=0, clippy RC=0 no box pgrx 0.19+PG18; CI `lint-rust` ampliado (teste do núcleo + gate zero-pgrx + check spike-lexical) |

## Nota de infraestrutura (honestidade)

O build pgrx **não** foi validado localmente (sem sudo p/ flex/bison/readline → PG18 não compila local) nem
dependeu do self-hosted runner frágil (jobs presos na fila). Foi validado no droplet **e2e-runner
(165.227.121.20, 32GB, cargo-pgrx 0.19.0 + PG18 real)** — o caminho confiável, com evidência medida (RC=0). O CI
`lint-rust.yml` foi ampliado para cobrir o path `spike-lexical` daqui pra frente.

## Conclusão

Merge-ready. A extração é segura (a auditoria adversarial confirmou o ponto sutil do `panic="unwind"` no crate
separado) e entrega o valor do milestone: os testes do núcleo lexical agora rodam com `cargo test` stock, sem o
link pgrx que os prendia (M139). **Gate M140.2 PASSA → M140.3.**
