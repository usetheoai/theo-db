# Review — M136 (gates mecânicos de qualidade + cassert no CI) — 2026-07-21

**Verdict:** READY_TO_MERGE. Milestone de fundação (compra a propriedade "erros de classe conhecida param de
chegar em develop sem intervenção humana"), não capacidade de produto.

## DoD — verificação item a item (cada gate verificado verde por medição direta)

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | `deny.toml` + `cargo deny check` (D1) | ✅ (v0.122.0) | `license-gate.yml` verde no CI — "licenses ok", 0 AGPL |
| 2 | clippy `-D warnings`, arquivo único, decisão warnings | ✅ | `.clippy_args` (21 cats baseline) → `clippy_exit=0, residual_warnings=0`; decisão baseline-com-sunset registrada (#151) |
| 3 | `rustfmt.toml` + `cargo fmt --check` | ✅ | mutirão `cargo fmt` (65 arq., `c6025d3`) → `cargo fmt -- --check` 0 diffs no droplet |
| 4 | Entrada cassert (`--enable-cassert`) | ✅ | `cassert-smoke.sh` → `CASSERT_SMOKE_OK` exit 0, 4 AMs + columnar, 0 asserts/crashes |
| 5 | `pgspot` sobre a SQL de instalação | ✅ | `pgspot` baseline 6 códigos → `Errors: 0 Warnings: 0` |
| 6 | `metadata --locked` + `doc` + `machete` | ✅ | no `lint-rust.yml`; `arrow` baselined (uso via `datafusion::arrow`) |

## Decisão de baseline (registrada, não improviso — exigência do DoD)

- **fmt**: mutirão (`cargo fmt` é 1 comando mecânico, preserva semântica; build verificado).
- **clippy / pgspot / machete / doc**: baseline-allow com SUNSET 2026-10-21 + burn-down (#151), porque o backlog
  (1056 clippy + 198 pgspot) é inviável de zerar num passo e boa parte mora em código unsafe/pgrx. O gate ainda
  barra categorias/códigos NOVOS. Prioridade de burn-down: `unsafe_op_in_unsafe_fn` **antes do M139**.

## Verificação no CI + correção de infra descoberta

Ao verificar, achei que o runner self-hosted **nunca tivera o toolchain Rust+pgrx** para o `ghrunner` (só
Docker/shell — M133), então NENHUM job Rust rodava (o `license-gate` falhava, não passava como eu supunha).
Provisionei o ghrunner (rustc 1.97.1, cargo-pgrx 0.19.0, cargo pgrx init pg18 **cassert**). Resultado: os 3
gates Rust ficaram **verdes no CI** — `license-gate` (licenses ok), `lint-rust` (success), `cassert-sql-safety`
(success). Correção load-bearing: todo job Rust do CI passa a funcionar. A serialização do runner único (#149)
permanece como limitação de throughput, não de sinal.

## Achados colaterais filados

- #151 — burn-down dos baselines (clippy/pgspot/machete/doc).
- pgspot PS005 (search_path em ~7 funções SQL) — hardening real, rastreado em #151.

## Conclusão

Merge-ready. A fundação de gates mecânicos de Rust existe pela primeira vez, cada um verificado funcional. O
gate de maior valor (cassert) prova que os 4 AMs + columnar não violam asserção do engine — a rede que teria
pego o #143. Backlog pré-existente honestamente baselined com sunset.
