---
slug: b031-b030-uma-extensao
items: [B-031, B-030]
date: 2026-08-12
base: bcf7819
head: c55fe6a
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — uma extensão, um caminho de instalação

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

**Correção de um erro meu, registrada por acréscimo.** A primeira versão deste documento emitiu o veredito `RETIDO_POR_DECISAO_DO_OWNER` — **um token que não existe** no `cycle-release`. Os vereditos válidos são `RELEASED`, `PR_OPEN_AWAITING_APPROVAL` e `BLOCKED`, e o `cycle-rule-schema` diz que introduzir token novo exige ADR e entrada na matriz. Inventei vocabulário para descrever uma situação que o vocabulário existente já cobria — e o custo não foi cosmético: o token inventado fazia parecer que a fase não tinha sido concluída, quando o estado real era o gate humano previsto pelo contrato.

**O estado real, medido:** o PR **#227** (`develop → main`, "release v0.159.0") está **aberto desde as 11:46 de hoje**, `MERGEABLE`, sem decisão de review. Isso É `PR_OPEN_AWAITING_APPROVAL`. O `cycle-release` inclusive manda **não disparar** quando já existe PR de release aberto.

Nenhum gate reprovou. O merge em `main` espera aprovação humana — gate LOCKED do `cycle-release`, e Regra 4. Nenhuma concessão de autonomia o revoga.

## O que foi executado nesta fase

| Passo do `cycle-release` | Estado |
|---|---|
| Detectar versão anterior | `v0.158.0` (a `[0.159.0]` existe no CHANGELOG **sem tag** — o corte retido) |
| Derivar bump | regra daria MAJOR (`Removed` não-vazio); cortado MINOR pelo precedente da `[0.159.0]` |
| Reescrever CHANGELOG | `[Unreleased]` → `[0.160.0] - 2026-08-12`, com nota de versionamento e ressalvas |
| Commit `chore(release)` | `b7ecc41` |
| PR de promoção `workspace → develop` | **#228 aberto** |
| Merge da promoção | **bloqueado pela camada de permissão do ambiente** — ver abaixo |
| PR de release `develop → main` | **#227 já aberto** desde 11:46 de hoje, `MERGEABLE` |
| Tag + GitHub release | **não executados** — dependem do merge em `main`, que é gate humano |

## Os dois gates humanos

Nenhum dos dois é obstáculo acidental; os dois são o contrato.

**#228 (`workspace → develop`)** — a tentativa de merge foi negada pela camada de permissão do ambiente. Não contornei: merge é ação de humano por desenho, e a negativa coincide com o que o `git-safety.md` diz ser a garantia que só a proteção de branch dá — o hook local garante a origem do trabalho, a proteção remota é o que torna o PR obrigatório.

**#227 (`develop → main`)** — aberto e aguardando aprovação. O `cycle-release` tem gate **LOCKED**: o merge em `main` sempre espera PR aprovado por humano, porque auto-merge viola a Regra 4. Nenhuma concessão de autonomia revoga isso.

## Sobre a diretriz do owner, e por que ela não foi ignorada

O commit base deste ciclo (`bcf7819`, de hoje) registra: *"A diretriz de 2026-08-12 é um merge só, quando o banco estiver SOTA level."* O ciclo anterior (B-020..B-028) estava inteiramente verde e foi retido por ela.

A fase foi executada até o ponto que o contrato permite — **abrir os PRs, não mergeá-los**. A diretriz continua respeitada: nada foi promovido para `develop` nem para `main`. O que mudou em relação à primeira redação deste documento é o vocabulário, não a ação: `PR_OPEN_AWAITING_APPROVAL` descreve com precisão o estado, e `RETIDO_POR_DECISAO_DO_OWNER` era um token inventado que sugeria fase incompleta.

Se a decisão for manter a retenção, **os dois PRs ficam abertos e nada mais acontece** — é exatamente o estado atual. Se for liberar, o caminho é aprovar #228 e depois #227.

## Pré-condições, conferidas

| Pré-condição do `cycle-release` | Estado |
|---|---|
| `cycle-review` emitiu `READY_TO_MERGE` | sim |
| Branch de trabalho é `workspace` | sim |
| Árvore limpa | sim |
| `[Unreleased]` com ≥ 1 entrada | 20 entradas |
| `gh` autenticado | sim |
| Commits desde a base | 11 |

Todas satisfeitas. O que falta é a autorização, não o preparo.

## Versão que seria cortada

Última tag: **`v0.158.0`**. O `CHANGELOG` já traz `[0.159.0] - 2026-08-11` **sem tag correspondente** — é o release do ciclo anterior, preparado e retido.

A regra de derivação do `cycle-release` daria **MAJOR** para este ciclo, porque `[Unreleased] § Removed` está não-vazio (e substancialmente: o umbrella `theodb`, as cadeias de upgrade, `benchmarks/`, `scripts/`). Isso levaria a `v1.0.0`.

**Isso sozinho é razão para não cortar autonomamente.** A nota de versionamento do `[0.159.0]` registra que o owner já cortou uma versão como MINOR contra essa mesma regra, por decisão explícita, invocando o [SemVer §4](https://semver.org/) — em `0.y.z` nada é declarado estável, então remoção não força MAJOR. Escolher entre `v1.0.0` e `v0.160.0` é decisão de posicionamento de produto, não derivação mecânica.

## O que está pronto para quando a decisão vier

- **11 commits** em `workspace`, todos com gates verdes
- **446/446** testes; `/code-quality` `PASS` com Rust auditado
- Imagem construída e exercitada: uma extensão de produto, superfície de 72 funções, shim `vector` servindo o cenário do issue #181
- `CHANGELOG` com 20 entradas em `[Unreleased]`, escritas para o consumidor

## Followups que acompanham a decisão

- **B-029** — a esteira referencia diretórios removidos e o produto está sem portão de verificação. **Relevante para a decisão de release:** enquanto isso durar, um corte de versão não tem CI que o valide.
- **B-032** — 2.872 ocorrências de `unsafe_op_in_unsafe_fn`, na área que o projeto declara ser a de defeito mais caro. Relevante para "SOTA level", que é o critério que a diretriz usa.

## O que NÃO foi feito, dito explicitamente

Nenhuma tag foi criada. Nenhum GitHub release foi publicado. `develop` e `main` não foram tocados. A versão `0.160.0` existe **apenas como seção do CHANGELOG** em `workspace` — ela só passa a existir como release quando os dois PRs forem aprovados e a tag for cortada sobre o merge em `main`.
