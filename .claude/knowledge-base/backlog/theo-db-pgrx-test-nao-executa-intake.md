---
slug: theo-db-pgrx-test-nao-executa
generated_by: backlog-item
date: 2026-08-09
status: completed
verdict: ITEM_REGISTERED
item: B-001
---

# Intake — `cargo pgrx test` não executa

Primeiro item registrado no `BACKLOG.md` deste repositório.

## G2 — dedup

Registro tinha 0 itens; grep por `pgrx`/`teste`/`suite` só bateu na tabela de roteamento. Sem colisão.

## G5 — sem prior art

A justificativa é inteiramente local: a suíte **deste** repositório não executa, reproduzido **duas
vezes** no builder construído a partir do próprio `Dockerfile`. Nenhuma referência a como outro
projeto faz. Gate não dispara.

## G3 — domínio único

`engine-pgrx` — é a fronteira Rust↔PostgreSQL, e o especialista é o `theo-pgrx`.

## Nota sobre a evidência

O schema diz `evidence: none-yet` no intake, e este item **quebra isso deliberadamente**: a evidência
já existe porque o item nasceu de uma tentativa de trabalho, não de uma suspeita. O `cycle-backlog`
permite registrar evidência oferecida espontaneamente — o que ele proíbe é *exigi-la* no intake.

## Por que as três hipóteses refutadas ficam no item

Elas custaram três ciclos de build de vários minutos cada. Registrá-las no corpo do `B-NNN` evita que
a próxima pessoa — ou eu, numa sessão futura sem este contexto — pague de novo. É o mesmo princípio
dos honest-negatives do acervo, aplicado a um item de manutenção.
