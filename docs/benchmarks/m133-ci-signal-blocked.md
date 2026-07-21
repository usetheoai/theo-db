# M133 — #140: por que o sinal de CI está morto (evidência primária, não inferência)

> Coletado em 2026-07-21 contra `usetheodev/theo-db`, run `29849809633` (workflow `ci-canary`, job `88699507161`).

## A causa, na palavra do próprio GitHub

O primeiro diagnóstico ("nenhum log ⇒ provavelmente billing") era **inferência**. A causa verbatim vem da
annotation do check-run — `GET /repos/usetheodev/theo-db/check-runs/88699507161/annotations`:

```json
[{"path":".github","annotation_level":"failure",
  "message":"The job was not started because recent account payments have failed or your spending limit
             needs to be increased. Please check the 'Billing & plans' section in your settings"}]
```

Isso é evidência de primeira ordem. Não é "achamos que é billing".

## O que descarta as hipóteses do lado do repositório

| Hipótese | Teste | Resultado |
|---|---|---|
| Workflow quebrado (YAML, matriz, secrets) | `ci-canary.yml` reduzido a **um único `echo`** | falha igual |
| Actions desabilitado no repo | `GET /actions/permissions` | `{"enabled": true, "allowed_actions": "all"}` — correto |
| Falha dentro de algum step | `jobs[].steps` do run | **array vazio** — nenhum step existiu |
| Runner alocado e quebrado | `runner_name`, `runner_group_name` | ambos `""` — runner nunca foi atribuído |
| Timeout / job travado | `started_at` → `completed_at` | 16:42:39 → 16:42:41 (**2 s**) |

Dez check-runs (`canary`, `pg-regression`, `hybrid-search`, `ai-sql`, `bm25-measure`, `columnar-measure`,
`harness-unit`, `image-and-bench`, `migration-smoke`, `nl-sql`) falham de forma idêntica — todos antes de qualquer
step. Um workflow de um `echo` não tem como falhar por mérito próprio.

## A ação que destrava (só o dono da conta pode executar)

`usetheodev` é uma conta do tipo **User**, não Organization, então o controle NÃO está em settings de organização:

**Settings da conta → Billing and plans → Spending limit → Actions** — regularizar o pagamento e/ou elevar o
limite. Alternativa equivalente: tornar o repositório **público** (minutos de Actions são gratuitos em repos
públicos).

## Por que M133 não foi "resolvido" com um self-hosted runner

Seria um workaround no sentido literal: mascarar um problema de cobrança com infraestrutura. Pior, o sinal
resultante seria falso — um runner self-hosted na box de dev não executa o que o CI diz executar (ambiente,
isolamento e matriz diferentes), então o verde não significaria "o projeto passa". A regra do projeto é
BLOCKED honesto acima de PASS falso (Regra Inquebrável 3).

## Estado

**M133 = BLOCKED-on-owner.** Não há mudança neste repositório que altere o resultado. Os outros 63 milestones do
roadmap estão `[x]`; este é o único `[ ]`, e por isso `ROADMAP_COMPLETED` **não** pode ser declarado.
