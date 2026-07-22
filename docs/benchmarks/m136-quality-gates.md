# M136 — gates mecânicos de qualidade + Postgres cassert no CI (verificado)

> Verificado 2026-07-21 na droplet (165.227.121.20), PostgreSQL **18.4** do pgrx (`--enable-cassert
> --enable-debug`, `USE_ASSERT_CHECKING`, `RANDOMIZE_ALLOCATED_MEMORY`), cargo 1.97.1 + clippy + rustfmt 1.9.
> Burn-down dos baselines: **issue #151** (SUNSET 2026-10-21).

## Headline

O `theodb_rs` (~30k LoC) **nunca tivera gate mecânico de Rust** — 1056 warnings clippy, código nunca
rustfmt-formatado, D1 (AGPL) só por vigilância humana. Este milestone retrofita a fundação: **6 gates, cada um
verificado verde por medição direta**. O padrão é o de retrofit honesto: o **fmt** foi mutirão (`cargo fmt`
deixou o código 100% limpo), o resto do backlog entrou em **baseline-allow com sunset + burn-down (#151)** —
a decisão de DoD "não ficar no meio" resolvida por viabilidade (fmt é 1 comando mecânico; 1056 clippy são 1056
julgamentos, muitos em unsafe/pgrx).

## Gates entregues (medição, não suposição)

| Gate | DoD | Estado verificado | Evidência |
|---|---|---|---|
| D1 license (`cargo deny check`) | item 1 | verde (v0.122.0) | `license-gate.yml` — "licenses ok", 0 AGPL |
| clippy `-D warnings` (baseline 21 cats) | item 2 | **verde** | `cargo clippy --features pg18 --no-deps -- $(.clippy_args)` → `clippy_exit=0, residual_warnings=0` |
| rustfmt `--check` (mutirão) | item 3 | **verde** | `cargo fmt` mutirão (65 arq., commit `c6025d3`); `cargo fmt -- --check` → `0 diffs` no droplet |
| **cassert smoke** (4 AMs + columnar sob Assert) | item 4 | **verde** | `scripts/cassert-smoke.sh` → `CASSERT_SMOKE_OK`, exit 0, 0 asserts/crashes |
| pgspot (SQL de instalação, baseline 6 códigos) | item 5 | **verde** | `pgspot --ignore PS00{1,2,5},PS0{10,16,17}` → `Errors: 0 Warnings: 0` |
| machete + `metadata --locked` + doc | item 6 | **verde** | machete: `arrow` baselined (uso via `datafusion::arrow`); fmt-clean compila (`cargo check` exit 0) |

## O gate de maior valor: Postgres `--enable-cassert`

Em build de release `Assert()` é no-op — a única cobertura de asserção do engine vem de um PG compilado com
`--enable-cassert`. É a lição #1 do paradedb e **a classe exata do crash #143** (stub sem `#[pg_guard]` →
`_URC_END_OF_STACK` → abort no CREATE INDEX). O `scripts/cassert-smoke.sh` exercita os quatro index/table AMs
(`theodb_hnsw`, `theodb_ivfflat`, `theodb_symqg`, `theodb_columnar`) + inserts/queries sob esse PG e FALHA se o
servidor abortar (`TRAP: failed Assert`) ou cair. Verificado: **exit 0, 0 asserts, servidor vivo** — os
caminhos exercitados não violam nenhuma asserção do engine. (`initdb` recusa root, então o smoke roda via
`runuser -u pgtest`, padrão M137.)

## Decisão de baseline (registrada — não improviso)

| Backlog | Escolha | Por quê |
|---|---|---|
| fmt (nunca formatado) | **mutirão** | `cargo fmt` é 1 comando mecânico, preserva semântica (round-trip pelo parser); build verificado (`cargo check` exit 0) |
| 1056 warnings clippy (21 cats) | **baseline + sunset** | corrigir 1 a 1, muitos em unsafe/pgrx, é inviável num passo; `-D warnings` ainda barra categoria NOVA |
| pgspot (198 findings) | **baseline 6 códigos + sunset** | PS002/PS010 inerentes a pgrx (handlers de AM em C, CREATE SCHEMA); PS005 hardening real → #151; gate pega códigos NOVOS |
| `arrow` (machete) | **baseline** | uso via `datafusion::arrow`; remover às cegas arrisca drift de versão → confirmar re-export primeiro (#151) |

Prioridade de burn-down (#151): `unsafe_op_in_unsafe_fn` **antes do M139** (código unsafe novo), depois
`dead_code`/`unused`, `deprecated`, `PS005` (search_path).

## Verificação no CI (os gates barram um PR de verdade)

Ao verificar no CI, descobri que o runner self-hosted **nunca tivera o toolchain Rust+pgrx** para o usuário
`ghrunner` — só o Docker/shell fora provisionado (M133). Por isso **nenhum** job Rust rodava (o `license-gate`,
que eu supunha verde, na verdade falhava com `/home/ghrunner/.cargo/env: No such file or directory`). Provisionei
o ghrunner (rustc 1.97.1, cargo-pgrx **0.19.0** — casa o `Cargo.toml` —, cargo-deny/machete, e `cargo pgrx init
--pg18` que compila um PG **com --enable-cassert**). Correção de infra load-bearing: **todo** job Rust do CI
passa a funcionar, não só os do M136.

Estado final no CI (self-hosted, ghrunner):

| Workflow | Conclusão | O que roda |
|---|---|---|
| `license-gate` | **licenses ok** (exit 0) | `cargo deny check licenses` |
| `lint-rust` | **success** | metadata --locked + fmt --check + clippy(baseline) + machete + doc |
| `cassert-sql-safety` | **success** | `cargo pgrx install` + pgspot(baseline) + cassert-smoke(4 AMs + columnar) |

## Reprodução

```bash
cd theodb_rs
cargo fmt -- --check                                             # item 3 — 0 diffs
cargo clippy --features pg18 --no-deps -- $(grep -v '^#' .clippy_args | tr '\n' ' ')   # item 2 — 0 warnings
PGINST=$HOME/.pgrx/18.4/pgrx-install bash ../scripts/cassert-smoke.sh   # item 4 — CASSERT_SMOKE_OK
pgspot --ignore PS001 --ignore PS002 --ignore PS005 --ignore PS010 --ignore PS016 --ignore PS017 <install.sql>  # item 5
```
