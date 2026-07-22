# ADR 0053 — Núcleo lexical num crate pgrx-free (`theodb_lexical`)

- **Status:** Aceito
- **Data:** 2026-07-22
- **Milestone:** M140.2 (crate núcleo lexical sem pgrx)
- **Relacionado:** ADR 0009 (superfície SQL = `api.rs` único), ADR 0051 (spike M139), `rules/architecture.md` (§1 camadas, §2 DIP)

## Contexto

O spike M139 (ADR 0051) provou que o núcleo do motor lexical — o trait `Directory` do Tantivy sobre um
`MemStore` em memória (buffer-then-flush) — é **pgrx-free por design**: importa só `std` + `tantivy`, zero
`pgrx`/`pg_sys`/`Spi`/`#[pg_extern]`. Mas ele vivia DENTRO do crate `theodb_rs` (cdylib pgrx), então
`cargo test` tentava linkar os símbolos do Postgres e **falhava** (M139 documentou: o teste in-crate não linka;
um crate standalone passa). Os 6 testes do núcleo só rodavam via `cargo pgrx test` (que não linka na droplet).

## Decisão

**D1 — O núcleo lexical vive num crate próprio `theodb_lexical` (rlib, dep só `tantivy`, SEM pgrx); o
`theodb_rs` (cdylib pgrx) o consome atrás da feature `spike-lexical`.**

**D2 — `theodb_rs` é o workspace root** (`[workspace] members = [".", "lexical_core"]`); o núcleo em
`theodb_rs/lexical_core/`. `cargo pgrx` opera sobre o membro `theodb_rs` como antes.

## Relação com o ADR 0009 (não há reversão silenciosa — honestidade Regra 3)

O ADR 0009 decidiu que a **superfície SQL** de `theodb_rs` é um único módulo `api.rs` (facade), porque **todos
os `#[pg_extern]` compartilham um único `#[pg_schema] mod theodb_rs`** (o schema SQL vem do ident do módulo);
distribuí-los exigiria N declarações do mesmo ident de schema, um padrão pgrx não-validado.

**Este ADR NÃO contradiz o 0009.** O núcleo lexical tem **zero `#[pg_extern]`** — não é superfície SQL, é
**lógica pura** (outra camada, `architecture.md §1`). A restrição do 0009 é sobre a camada de externs; separar
uma camada de lógica pura por **testabilidade** (o problema de link pgrx do M139) é ortogonal. Os `#[pg_extern]`
do spike (`lexical_spike_*` em `pg_backing.rs`) continuam no único `mod theodb_rs` de `api.rs`/`pg_backing`, sob
o 0009. É DIP (`§2`): o núcleo define o trait `SegmentStore`; a camada pgrx (`pg_backing.rs`) o implementa
sobre o heap.

## Alternativas consideradas

- **Manter o núcleo dentro de `theodb_rs`, testar só via `cargo pgrx test`.** Rejeitado: `cargo pgrx test` não
  linka na droplet (M139) — os testes do núcleo ficariam presos; a testabilidade stock é o ganho do milestone.
- **Workspace no repo root** (englobando `benchmarks/`, docs). Rejeitado: arrasta diretórios não-Rust para um
  workspace cargo; o workspace dentro de `theodb_rs/` é mais simples (KISS).
- **Crate publicado (crates.io).** YAGNI — uso interno, path dep basta.

## Consequências

- **Habilita:** `cargo test -p theodb_lexical` stock (sem pgrx) — 6 testes do núcleo verdes localmente; M140.3/
  M140.4 testam a query/scoring puras sem o link pgrx. O CI (`lint-rust.yml`) agora roda o teste do núcleo + o
  gate objetivo `cargo tree | grep -c pgrx == 0` + `cargo check --features spike-lexical` (o cdylib consome o núcleo).
- **Restringe:** o núcleo é genuinamente pgrx-free — se um tipo pgrx vazar para lá, o crate para de compilar (o
  `Cargo.toml` sem pgrx é o gate objetivo). O build shipado (default, sem `spike-lexical`) continua sem tantivy.
- **Custo:** `theodb_rs` vira workspace; validado que `cargo pgrx` opera normalmente (CI cassert-sql-safety +
  lint-rust no self-hosted runner com PG18).

## Referências

- ADR 0051 (M139 — o spike que provou o núcleo pgrx-free)
- ADR 0009 (a superfície SQL única — a decisão que ESTE ADR reconcilia, não reverte)
- `.claude/knowledge-base/plans/m140-2-lexical-core-crate-plan.md` (o plano; D1/D2 acima = ADRs D1/D2 do plano)
- `rules/architecture.md` §1 (camadas), §2 (DIP)
