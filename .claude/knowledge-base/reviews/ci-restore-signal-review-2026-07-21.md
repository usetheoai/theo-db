# Review — M133 (restaurar sinal de CI, fix #140) — 2026-07-21

**Verdict:** READY_TO_MERGE. Milestone de infra/CI, não capacidade de produto.

## DoD — verificação item a item

| # | Item DoD | Estado | Evidência |
|---|---|---|---|
| 1 | Causa-raiz identificada com evidência, registrada em #140 | ✅ | Comentário #140: bloqueio de billing atinge só runners HOSPEDADOS; GitHub despacha para self-hosted mesmo assim → runner `theodb-do-1` |
| 2 | ≥1 run completo onde os steps executam (não-vazio + log) | ✅ | `ci-canary` success/3 steps; `schema-drift-gate` success/6 steps; jobs pesados renovando no `_diag` |
| 3 | Conclusão triada; verde fecha; vermelho on-merit vira issue próprio | ✅ | canary/drift verdes; falhas → #148 (harness-unit) + #149 (tracking suite-wide) |
| 4 | Notificação de falha (hook workflow_run) | ✅ | `.github/workflows/ci-failure-notify.yml` — abre/atualiza issue em `workflow_run` failure |
| 5 | #140 fechado com comentário de evidência | ✅ | #140 CLOSED com o comentário de destravamento + o comentário de fecho de loop |

## Fronteira honesta

O sinal está **restaurado e provado**. A limitação remanescente — o runner self-hosted **único** serializa os
9+ jobs, e um run pesado completo (múltiplos `docker buildx`) demora — é **capacidade**, não ausência de sinal:
problema menor e distinto do #140, rastreado em #149. Por isso a triagem do suite pesado é filada como issues de
follow-up (o escopo do M133 é "restaurar sinal + triar", não "consertar N quebras latentes" — risco (b) explícito
do milestone).

## Net-new de código

`ci-failure-notify.yml` (o notifier). Tudo o mais (evidência, triagem, fecho do #140) é artefato do GitHub, não
código. Deliverable coerente com um milestone de reparo de CI.

## Conclusão

Merge-ready. Sinal de CI de volta, com rede (notify) para não morrer em silêncio de novo, e as falhas acumuladas
honestamente triadas em issues próprios.
