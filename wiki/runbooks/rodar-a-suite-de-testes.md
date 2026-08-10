---
type: Runbook
title: Como rodar a suíte de testes do TheoDB (os 439, incluindo os #[pg_test])
description: A receita completa, medida em 2026-08-10, para uma suíte que passou meses sem executar — cinco peças, cada uma resolvendo um bloqueio distinto que os outros não resolvem.
tags: [runbook, testes, pgrx, b-001, ambiente]
generated: { by: claude-code/opus-5, at: 2026-08-10T03:30:00Z }
---

A suíte tem **439 testes** e passou meses sem executar nenhum. O item B-001 rastreou o problema e a
investigação de 2026-08-09/10 encontrou **três bloqueios independentes**, empilhados — resolver um só não
produz nenhum teste rodando, o que explica por que sete hipóteses anteriores "não funcionaram".

# A receita

```bash
# 1. LINK — o binário de teste não linka sem isto
export RUSTFLAGS="-Clink-arg=-Wl,--unresolved-symbols=ignore-all"

# 2. CARREGAMENTO — src/pg_test_stubs.rs (já no repo, sob #[cfg(test)])
#    define os 16 símbolos do backend que o carregador exige e o binário nunca usa

# 3. INSTALAÇÃO — só para os #[pg_test]; exigem instalar a extensão como outro usuário
apt-get install -y sudo          # o pgrx invoca `cargo pgrx install --sudo`
useradd -m postgres              # postgres se recusa a rodar como root
mkdir -p /pgdata && chown postgres /pgdata
export CARGO_PGRX_TEST_RUNAS=postgres
export CARGO_PGRX_TEST_PGDATA=/pgdata

cargo pgrx test pg18 <filtro>
```

# Os três bloqueios, e por que cada um engana

| # | Sintoma | Causa | O que engana |
|---|---|---|---|
| 1 | `undefined symbol: pfree, palloc0` no **link** | as crates do pgrx referenciam símbolos do backend | parece problema de dependência |
| 2 | `symbol lookup error: CurrentMemoryContext` no **carregamento** | `CurrentMemoryContext` é símbolo de **dado**, e dados não são ligados preguiçosamente — basta serem alcançáveis | resolver o link **não** resolve isto; são camadas diferentes |
| 3 | o harness **trava**, ou falha em `framework.rs:425` | `cargo pgrx install --sudo` sem `sudo` instalado | não há mensagem dizendo "sudo faltando"; o erro mostra o comando inteiro e o motivo real fica na última linha |

**O bloqueio 3 tem um agravante que custou horas:** `cargo pgrx start pg18` **reporta sucesso e não abre a
porta**, sem produzir log algum. O mesmo `data-18` sobe em segundos pelo `pg_ctl` direto. Isso faz o problema
parecer do PostgreSQL quando é da camada do pgrx.

# Estado medido

| | |
|---|---|
| `#[test]` puros | **69 passam** (verificado) |
| `#[pg_test]` | **1 verificado** — `pg_sq8_roundtrips_through_meta_bytes`, em 324 s |
| suíte completa | **NÃO executada** — cada `#[pg_test]` reinstala a extensão, e 439 testes assim levam horas |

O último ponto é honesto e importante: **provei que a receita funciona, não que a suíte passa.** Rodar tudo é
trabalho de CI, e o número de falhas reais é desconhecido — testes que nunca rodaram costumam ter apodrecido.

# Relacionados

- A correção do planner cujos testes isto finalmente executou: [m175](/benchmarks/m175-planner-cost-inversion-verdict.md)
- O levantamento que mediu 419 testes sem execução: [m184](/benchmarks/m184-pilares-superficie-medida-verdict.md)

# Por onde isto deve ir

Esta receita deve virar um alvo no CI e um `make test`. Enquanto for conhecimento de runbook, ela vale para
quem lê o runbook.
