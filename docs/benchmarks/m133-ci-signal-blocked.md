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


## Adendo (2026-07-21) — o billing NÃO é o único bloqueio do M133

Validei os workflows localmente, sem depender de minutos do Actions. Duas descobertas, ambas medidas:

**1. As definições dos workflows estão corretas.** `actionlint 1.7.7` sobre `ci.yml` + `ci-canary.yml` → **exit 0,
zero achados**. O YAML, as expressões, os inputs das actions e os labels de runner estão válidos.

**2. Mas o job `harness-unit` reprovaria hoje, por mérito próprio.** Ele é o único job reproduzível sem Docker, e
rodei os passos exatamente como o CI faria (`working-directory: benchmarks`, Python 3.12):

| Passo do CI | Antes | Depois |
|---|---|---|
| `ruff check theodb_bench tests` | **exit 1** — 24 erros (20×E702, 2×F541, 1×F401, 1×E741) | **exit 0** |
| `vulture theodb_bench --min-confidence 80` | **exit 3** | **exit 0** |
| `pytest -m "not integration" -q` | **trava** (>10 min sem terminar) | **ainda trava** — ver abaixo |

O lint foi corrigido (mudança puramente sintática: `a; b` → duas linhas, `f""` sem placeholder, import morto, `l`
→ `ln`; todos os arquivos recompilam). O `entry_sql` que o vulture acusava é uma **fixture do pytest** — pedir a
fixture é o que dispara o guard de skip-offline —, então foi ignorada por nome em `[tool.vulture]` em vez de
deletada, o que teria desativado o guard silenciosamente.

**O terceiro passo continua quebrado e o conserto é uma decisão de design, não mecânica.**
`pytest -m "not integration"` seleciona **265 dos 527** testes, e boa parte deles conecta num Postgres real:
**34 arquivos** em `tests/` usam `connect()`, e ~20 deles não têm marker `integration` em nível de arquivo. O job
`harness-unit` não sobe container nenhum, então esses testes dão ERROR — e `test_am_crash.py` fica **pendurado**
esperando o banco, o que estoura o timeout do job antes de qualquer relatório.

Consequência prática: **liberar o billing não deixa o CI verde.** Ele apenas troca "falha por cobrança" por
"falha por teste que precisa de banco num job que não tem banco". O M133 tem, portanto, dois bloqueios
independentes — um do dono da conta, um de código — e o segundo agora está medido em vez de suposto.
