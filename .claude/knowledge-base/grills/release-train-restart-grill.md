---
slug: release-train-restart
date: 2026-08-20
questions_asked: 5
decisions_resolved: 5
verdict: READY_FOR_PLAN
scope: theo-db (promoção + release); theodb-bench (só promoção)
---

# Grill: destravar o trem de release

## O sintoma que abriu o tema

O registro tem **36 itens `planned` e zero `shipped`**. Não é trabalho pendente — o trabalho está feito.
É a saída que não abre: `shipped` é a única transição deste schema que depende de algo fora do
repositório, e ela está travada desde 2026-07-29.

## Medições feitas antes de cada pergunta

| # | Medição | Resultado |
|---|---|---|
| M1 | Última tag × `main` × `develop` | `v0.158.0` · `[0.158.0]` · `[0.160.0]` — **duas versões escritas e nunca cortadas** |
| M2 | PR #227 (`develop → main`) | Aberto desde 2026-08-12, `MERGEABLE`, **100 commits**, título diz *v0.159.0* |
| M3 | Checks do #227 | **10 FAILURE contra 6 SUCCESS**. Não está esperando aprovação — está VERMELHO |
| M4 | Quando os checks rodaram | 2026-08-16, não fósseis. `develop` na mesma data: `lint-rust` e `publish` em falha |
| M5 | Causa do `lint-rust` | **`cargo fmt`** — diferença de formatação num `#[pg_test(error = …)]` |
| M6 | Causa do `publish` | Passo *"Trivy vulnerability scan (CRITICAL + HIGH gate)"*. Sem imagem, os 7 jobs que dependem dela caem por consequência |
| M7 | O que o Trivy achou | **22 vulnerabilidades — 21 HIGH, 1 CRITICAL** — em `stdlib` do Go **1.24.6**. `CVE-2025-68121`: validação incorreta de certificado em `crypto/tls` |
| M8 | De onde vem o Go | Nenhum no `Dockerfile`. Base é `postgres:18-bookworm` — imagem oficial, que embarca binário Go (`gosu`, hipótese não confirmada) |
| M9 | `.trivyignore` | **Nenhuma entrada ativa** |
| M10 | `workspace → develop` (theo-db) | 39 commits: 64 arquivos `.claude/`, 10 `wiki/`, CHANGELOG, BACKLOG. **Zero produto** |
| M11 | `develop → main` (theo-db) | **57 arquivos de produto**, +2.532 / −8.320. A carga do trem está aqui |
| M12 | Commits de release em `develop` | `80e53c3` = 0.159.0 · `b7ecc41` = 0.160.0 — identificáveis |
| M13 | Head do PR #227 | `0e2b96b` = head atual de `develop`. Ele ACOMPANHA a branch; só título e corpo estão velhos |
| M14 | `theodb-bench` | **Nunca lançou**: zero tags, CHANGELOG com uma única seção `[Unreleased]` |

## Decisões resolvidas

1. **D1 — O tema é destravar o trem**, slug `release-train-restart`. O que decidiu foi M1+M11: a release
   que importa (57 arquivos de produto) está pronta em `develop` desde 2026-08-16 e não sai.
2. **D2 — CVE herdado: medir antes de escolher, em ordem fixa.** (a) **Rebuild** contra a base atual —
   `postgres:18-bookworm` flutua e as Official Images são reconstruídas; se o upstream já publicou com
   `gosu` novo, o portão fica verde com zero linha de código (parsimony, degrau 1). (b) Só então
   `.trivyignore` **com sunset**, pelo `deps-audit-golden-rule.md`: CRITICAL ≤ 30 dias, HIGH ≤ 60,
   entrada em `[Unreleased] § Changed` e **ADR** justificado por **alcançabilidade MEDIDA** — se o
   binário é o `gosu`, ele nunca faz TLS e o `CVE-2025-68121` é inalcançável, mas isso se mede.
   (c) **Nunca** baixar `severity` nem ligar `--ignore-unfixed`: trocar um portão que reprova por um que
   não olha é a classe de defeito que este dia inteiro consertou.
3. **D3 — Duas tags retroativas, e o `[Unreleased]` NÃO vira versão.** Depois do merge, `80e53c3` e
   `b7ecc41` viram ancestrais de `main`; etiquetá-los devolve a propriedade que falta, porque **uma seção
   de CHANGELOG sem tag é afirmação não verificável**. O `[Unreleased]` não muda o artefato (M10), e um
   número novo sobre binário idêntico faz o número parar de significar algo. Como o conserto do `fmt`
   toca `theodb_rs`, o corte novo é **`v0.160.1` (patch)**, não o `minor` que a derivação do CHANGELOG
   daria — a derivação lê o CHANGELOG e não o diff, e as 14 entradas `Added` descrevem medições e
   portões, não capacidade nova do banco. Divergência declarada, não silenciosa.
4. **D4 — `theodb-bench`: promoção agora, primeira release depois e separada.** Promover exercita pela
   primeira vez a proteção criada hoje, que é o único jeito de saber se ela funciona antes de precisar
   dela. A primeira versão de um repo sem nenhuma tag é escolha de identidade (`v0.1.0` × `v1.0.0` dizem
   coisas diferentes sobre um arnês cujos números serão publicados) e merece plano próprio. Custo
   declarado: [[B-059]] e [[B-064]] seguem `planned` até lá.
5. **D5 — Autonomia total até a release, concedida pelo owner em 2026-08-20.** Recomendei o contrário —
   merge e veredito de CVE fora do meu alcance — e o owner reafirmou. A autorização humana existe, dada
   em avanço, e é ela que torna o gate do `cycle-release` satisfeito em vez de auto-concedido.
   **Um limite permanece, e não por autoridade:** se a medição mostrar o caminho vulnerável ALCANÇÁVEL,
   não há allowlist. Autonomia para decidir não é autonomia para decidir contra a evidência.

## Premissas registradas, não perguntadas

- **PR #227 é mantido e reescrito, não fechado.** M13 mostra que ele já carrega tudo; fechar não ganha
  nada e apaga o registro de que ficou 8 dias aberto e vermelho — que é a informação, não o ruído.
- **O conserto do `fmt` nasce em `workspace`**, como toda mudança (`git-safety.md § 1`).

## Q&A log

### Q1: O tema é destravar o trem de release?
**Recomendado**: sim, slug `release-train-restart` — 36 `planned` e zero `shipped`.
**Decisão do usuário**: aceito.

### Q2: Qual a política para CVE em binário herdado da imagem base?
**Recomendado**: rebuild primeiro, allowlist com sunset + ADR por alcançabilidade medida depois, nunca
afrouxar o gate.
**Decisão do usuário**: aceito.

### Q3: Como etiquetar, com duas versões escritas e não cortadas?
**Recomendado**: duas tags retroativas nos próprios commits; `[Unreleased]` não vira versão; o corte novo
é patch por não haver mudança de capacidade.
**Decisão do usuário**: aceito.

### Q4: `theodb-bench` entra neste trem?
**Recomendado**: promoção agora, primeira release em decisão separada.
**Decisão do usuário**: aceito.

### Q5: Onde fica a fronteira de execução?
**Recomendado**: eu levo até a porta do merge; merge e aceite de CVE são do owner.
**Decisão do usuário**: **override** — autonomia total até a release.

## O que este grill NÃO resolveu

- A **primeira versão do `theodb-bench`** (D4, deliberadamente fora).
- **Qual binário Go** carrega os 22 CVEs e se o caminho é alcançável — é medição, não decisão, e a D2
  fixa a ordem em que ela acontece.
- **[[B-081]]** segue aberto e independente deste trem.
