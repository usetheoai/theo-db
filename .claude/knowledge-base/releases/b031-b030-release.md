---
slug: b031-b030-uma-extensao
items: [B-031, B-030]
date: 2026-08-12
base: bcf7819
head: c55fe6a
verdict: RETIDO_POR_DECISAO_DO_OWNER
---

# Release — uma extensão, um caminho de instalação

## Veredito: `RETIDO_POR_DECISAO_DO_OWNER`

**Nenhum gate reprovou.** Este não é `BLOCKED` (não houve falha) nem `PR_OPEN_AWAITING_APPROVAL` (nenhum PR foi aberto). A release está pronta e **não foi cortada**, por decisão registrada do owner.

## Por que não cortei

O commit base deste ciclo — `bcf7819`, de hoje — registra a diretriz:

> *"Verdict RETIDO_POR_DECISAO_DO_OWNER — não é BLOCKED (nenhum gate reprovou) nem PR_OPEN_AWAITING_APPROVAL (nenhum PR novo aberto). **A diretriz de 2026-08-12 é um merge só, quando o banco estiver SOTA level.**"*

O ciclo anterior (B-020..B-028) estava **inteiramente verde** — suíte 442/0 confirmada duas vezes, clippy exit 0, fmt sem diffs, upgrade 1.4.0→1.5.0 com 4/4 cenários contra o install original extraído da imagem publicada — e ainda assim não foi liberado. Cortar agora contrariaria uma decisão que o owner tomou hoje, sobre exatamente esta situação.

A autonomia concedida cobre implementar e verificar. Não cobre revogar uma decisão do owner sobre quando o produto sai.

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

## O que fiz em vez de cortar

Empurrei `workspace` para `origin`. Isso persiste o trabalho na branch de trabalho e **não é release**: não abre PR, não toca `develop` nem `main`, não cria tag. É o mesmo que o ciclo anterior fez antes de ser retido.
