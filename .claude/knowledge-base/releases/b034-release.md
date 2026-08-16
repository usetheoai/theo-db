---
slug: b034-guc-alias
items: [B-034]
date: 2026-08-12
base: 0c42144
head: b886d3b
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — os GUCs de ajuste do pgvector passam a ter efeito

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

Nenhum gate reprovou. O merge espera aprovação humana — gate **LOCKED** do `cycle-release`, e Regra 4.

## Por que não há corte de versão novo

O `cycle-release` manda **não disparar** quando já existe PR de release aberto. Há dois: **#227** (`develop → main`) e **#228** (`workspace → develop`). O B-034 entra na `[0.160.0]`, já cortada e coberta pelo #228 — que agora leva quatro itens: B-030, B-031, B-033 e B-034.

Cortar `0.161.0` para algo que ainda não saiu criaria versão fantasma: mais uma seção no CHANGELOG e nenhuma tag para nenhuma.

## O que foi executado

| Passo | Estado |
|---|---|
| Versão | `[0.160.0]`, já cortada — B-034 acrescenta 1 entrada, total 24 |
| Push para `workspace` | `60b5c82..b886d3b` |
| PR de promoção | **#228 atualizado** — título passa a cobrir os quatro itens |
| PR de release | **#227 já aberto**, aguardando |
| Merge / tag / GitHub release | **não executados** — gate humano |

## Estado verificado

| Gate | Resultado |
|---|---|
| Suíte | **457 passed, 0 failed** |
| `/code-quality` | `PASS_WITH_CAVEATS`, Rust auditado, 0 achados HARD |
| `/review` | `READY_TO_MERGE`, 6/6 |
| Produto | aliases registrados; valor válido preservado no bootstrap; inválido detectado |

## O que este ciclo produziu além do código

**Uma correção de descrição já publicada.** O CHANGELOG afirmava que valor fora de faixa "passará a dar erro". A verificação no produto mostrou **duas formas**: erro quando o `SET` vem depois da primeira consulta, e aviso com retorno ao default quando vem antes — que é o caso mais comum, porque aplicações configuram no bootstrap. O segundo é mais suave, mas descarta o valor em silêncio: o usuário fica com 64 achando que pediu 99999.

É o **terceiro ciclo seguido** em que a verificação no produto encontra algo que os testes não pegam. Aqui não foi defeito de código — foi defeito de descrição, e já estava público.

**Uma decisão de desenho tomada contra a opção mais intuitiva.** O pgrx oferece `assign_hook`, que daria "o último `SET` vence". Rejeitado porque o PostgreSQL restaura GUCs no fim de transação disparando hooks, e a ordem entre variáveis independentes não é definida — um rollback poderia deixar o valor efetivo vindo do alias. Trocar um defeito silencioso por outro mais raro não é conserto.

## Followups

- **B-035** — cliente `theodb` no VectorDBBench, que este item desbloqueia: sem os aliases, qualquer varredura de `ef_search` por ferramenta externa produziria curva plana.
- **B-029** — a esteira segue vermelha; enquanto durar, nenhum corte tem CI que o valide.
- **B-032** — 2.872 `unsafe_op_in_unsafe_fn`.

## O que NÃO foi feito

Nenhuma tag criada. Nenhum release publicado. `develop` e `main` intocados.
