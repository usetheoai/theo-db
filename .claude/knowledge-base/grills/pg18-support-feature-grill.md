---
slug: pg18-support
milestone_id: M135
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M135 suporte a PostgreSQL 18

## Q1 — O que é e por que AGORA?

Suporte a PostgreSQL 18. **O que mudou:** o PG18 é o release estável atual há ~10 meses e o projeto
só compila no 17. A sondagem de compilação (2026-07-21, PG18.4 instalado via `cargo pgrx init --pg18
download` na droplet) mediu o custo real: **27 erros**, e o PRD §338 já registrava a intenção
("o MVP mira PostgreSQL 17 e adiciona PostgreSQL 18 em seguida") sem execução.

Evidência: `cargo check --features pg18` → 27 erros; `GenericXLog` (54 refs) e `IndexAmRoutine`
compilam **limpos** — a hipótese inicial de que a dor viria do WAL foi FALSIFICADA pela medição.

## Q2 — Dependências

M134 `[x]` (milestone `[x]` mais recente). Sem dependência funcional sobre outros milestones — a
mudança é transversal ao código de extensão, não a uma feature.

## Q3 — Decisões do owner (2026-07-21)

| Decisão | Escolha | Razão dada |
|---|---|---|
| 17+18 juntos vs migrar | **Migrar só para o 18** | "ainda não tem ninguém usando, essa é a oportunidade" — sem base instalada, não há custo de migração para terceiros, e evita dívida permanente de `#[cfg]`-branching |
| Bitmap scan no TAM colunar | **Portar de verdade** para o contrato novo | mantém paridade funcional; não deixa buraco silencioso |
| DoD | **Os quatro itens** | crash/MVCC no 18 + benchmark de sanidade + flags antigas resolvidas + packaging |

## Q4 — Riscos NOVOS

(a) **Os 119 artefatos de benchmark foram medidos no PG17.** Migrando para 18-only, eles passam a
descrever uma configuração que não distribuímos — comparabilidade quebrada, e qualquer alegação
futura que os cite fica desamparada.

(b) **O rework do bitmap toca o caminho de recheck do MVCC.** Um erro aqui NÃO aparece como falha de
compilação nem em teste de happy path: produz resultado **errado** (linha perdida ou duplicada) sob
página lossy, que é justamente o caso raro.
