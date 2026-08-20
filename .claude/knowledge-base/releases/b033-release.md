---
slug: b033-vector-btree
items: [B-033]
date: 2026-08-12
base: 515afa2
head: b7f137e
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — igualdade e ordenação para o tipo `vector`

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

Nenhum gate reprovou. O merge espera aprovação humana — gate **LOCKED** do `cycle-release`, e Regra 4.

## Por que não há corte de versão novo

O `cycle-release` manda **não disparar** quando já existe PR de release aberto. Há dois:

- **#227** (`develop → main`) — aberto às 11:46 de hoje, `MERGEABLE`, sem decisão de review.
- **#228** (`workspace → develop`) — a promoção deste conjunto de trabalho.

O B-033 entra na `[0.160.0]`, versão já cortada no CHANGELOG e coberta pelo #228. Cortar `0.161.0` para uma mudança que ainda não saiu criaria uma versão fantasma: duas seções no CHANGELOG e nenhuma tag para nenhuma das duas.

## O que foi executado

| Passo | Estado |
|---|---|
| Versão | `[0.160.0]`, já cortada — B-033 acrescenta 1 entrada, total 23 |
| Push para `workspace` | `0405526..b7f137e`, 7 commits |
| PR de promoção | **#228 atualizado** — título e corpo passam a cobrir os três itens |
| PR de release | **#227 já aberto**, aguardando |
| Merge | **não executado** — gate humano |
| Tag + GitHub release | **não executados** — dependem do merge em `main` |

## Estado verificado deste ciclo

| Gate | Resultado |
|---|---|
| Suíte | **451 passed, 0 failed** |
| `/code-quality` | `PASS_WITH_CAVEATS`, Rust auditado, 0 achados HARD |
| `/review` | `READY_TO_MERGE`, 11/11 afirmações |
| Imagem | `docker build` exit 0; cinco padrões provados com banco rodando |

## O que o ciclo produziu além do código

**Uma decisão que só a fonte primária evitou.** Eu ia implementar `check_dims` + erro em dimensão diferente, por analogia com as funções de distância deste projeto. O `vector_cmp_internal` do pgvector não faz isso — compara elementos e usa a dimensão como desempate. A implementação errada teria consertado a incompatibilidade antiga **criando outra**, e nenhum teste que eu escreveria pegaria: só um usuário migrando notaria a ordenação diferente.

**Um padrão meu, registrado com números.** Quatro suposições sobre ferramentas neste ciclo; três viraram erro. A única pega antes de custar retrabalho foi a de semântica — e só porque a Regra 8 obriga referência primária. Onde a regra não obrigou, supus e errei: o idioma de asserir erro no pgrx, a forma de comparação da mensagem, e o formato do `pg_describe_object`.

## Followups

- **B-029** — a esteira está vermelha; enquanto durar, **um corte de versão não tem CI que o valide**.
- **B-032** — 2.872 `unsafe_op_in_unsafe_fn`, relevante para o critério "SOTA level" da diretriz de release.

## O que NÃO foi feito

Nenhuma tag criada. Nenhum release publicado. `develop` e `main` intocados. A `0.160.0` existe **apenas como seção do CHANGELOG** em `workspace`.
