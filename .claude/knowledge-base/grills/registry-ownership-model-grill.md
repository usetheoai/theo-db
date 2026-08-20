---
slug: registry-ownership-model
date: 2026-08-20
questions_asked: 6
decisions_resolved: 6
verdict: READY_FOR_PLAN
revised: 2026-08-20
scope: local-only
items_unblocked: [B-077, B-079, B-080]
---

# Grill: o modelo de propriedade do registro

Desbloqueia o [[B-077]] (17 dos 18 blockers do `/backlog-review`) e a classe
`raw_with_evidence` (26 majors). Escopo: quem é dono de um item e o que governa o status.
Fora de escopo, deliberadamente: o `status` do [[B-021]] — item único, não decisão de modelo.

## Revisão de 2026-08-20 — decisão do owner, e o que ela retirou

**Escopo: apenas este consumidor. O repositório do kit (`squad`) não é tocado.**

O owner recusou a D5 como eu a havia escrito, e estava certo. Duas medições novas sustentam a recusa:

| # | Medição | Resultado |
|---|---|---|
| M10 | Como `check_backlog_structure.py` decide `unroutable_repo` | `_known_repos()` (`:144-174`) monta a **união de todos os repos de todas as linhas** e testa **pertinência** (`:245`). **Nunca pergunta qual domínio.** Logo os 17 blockers caem com a tabela apenas — zero linha de código |
| M11 | O que o instalador do kit diz sobre `rules/` e `agents/` | *"`rules/` and `agents/` are exactly where a project's own configuration lives: the routing table…"* (`install.sh:76`). Recusa sobrescrever sem `--force`, faz snapshot antes, e na pós-instalação manda rodar `detect_domains --write .claude/rules/cycle-backlog.md` |

A tabela de roteamento é **ponto de extensão por projeto**, e eu a tratei como código do kit. Erro meu de
categoria, não de medição.

### A forma de tabela que dispensa as mudanças de código

Levando a restrição a sério, existe uma escrita da tabela em que a ambiguidade nunca aparece: **cada repo
figura em EXATAMENTE UMA linha**, e os pilares que não possuem repo ficam com a coluna `Repos` vazia.

| Domain | Repos | Specialist |
|---|---|---|
| `engine-pgrx` | `theo-db` | `agents/theo-pgrx.md` |
| `arnes` | `theodb-bench` | `agents/arnes.md` |
| `vetorial` | *(vazio)* | `agents/theo-recall.md` |
| … | | |

Ela entrega as três coisas de uma vez: `known_repos` continua sendo a união, então os 17 blockers caem (M10);
`route_domain <repo>` fica **determinístico**, porque nenhum repo está em duas linhas; e os pilares seguem
sendo a unidade semântica dos itens (D1 preservada).

**O preço, declarado e não escondido:** a coluna `Repos` deixa de significar *"quais repos este pilar toca"* e
passa a significar *"qual repo roteia para cá por omissão"*. Um pilar de coluna vazia é alcançável pelo campo
`domain:` do item, nunca por repo. Isso tem de estar escrito na própria tabela, ou o próximo leitor infere o
sentido antigo.

### O que a revisão RETIRA

- **D2 sai.** Ela existia para desambiguar um repo em vários domínios. Com um repo por linha, não há ambiguidade
  a desambiguar. O defeito do primeiro-casamento silencioso (M4) **continua existindo no tool** — apenas deixa
  de ser alcançável por esta tabela. Fica registrado no [[B-080]] como defeito latente, não consertado aqui.
- **D6 sai.** Ela existia para permitir derivar a tabela do backlog. A tabela passa a ser escrita à mão, que é
  o que o próprio gerador autoriza no cabeçalho que emite (*"edit it by hand when ownership does not follow the
  directory layout"*). `detect_domains --from-backlog` continua recusando, e isso deixa de importar.
- **D5 é reescrita** (ver abaixo).

### O que a revisão NÃO retira

D1, D3 e D4 seguem inteiras: pilar como unidade, os 26 redistribuídos em quatro destinos com dois especialistas
a escrever, e a evidência governando o status.

## Medições que fundamentaram as recomendações

Todas feitas neste repo em 2026-08-20, antes de cada pergunta.

| # | Medição | Resultado |
|---|---|---|
| M1 | Quem lê o campo `domain:` dos itens | **Ninguém.** `route_domain.py:48` extrai só `repo:` (`ITEM_REPO_RE`); `check_backlog_structure.py:247` julga por repo; `detect_domains.py --from-backlog` lê os pares só para derivar |
| M2 | Domínios declarados × agentes em disco | 7 dos 8 domínios já têm especialista escrito com outro nome (`engine-pgrx`→`theo-pgrx`, `vetorial`→`theo-recall`, …). Só `theo-db` (26 itens) não tem |
| M3 | Agentes sem item | `theo-auditor` e `theo-concurrency` |
| M4 | `route()` com repo em N domínios | Devolve o **primeiro** em silêncio (`route_domain.py:96-100`) |
| M5 | `tests/test_route_domain.py` no consumidor | **Não existe** — `install.sh` não copia `tests/`. Existe no kit |
| M6 | Composição dos 26 `domain: theo-db` | arnês 13 · método 4 · governança 6 · build local 2 · engine 1 |
| M7 | `raw` com evidência × `raw` com `none-yet` | **26 × 9.** Por `source`: `discover-evolve` 9, `human` 9, `discover-review` 6, `discover-live-test` 2 |
| M8 | Repo do kit | Existe em `/home/paulo/Projetos/squad`, `workspace`, limpo. Carrega `sync_consumers.py`, `check_install_drift.py`, `install.sh`, `patch_install.sh` |
| M9 | Drift do commit `11581e2` (36 arquivos) contra o kit | **27 idênticos, 9 divergentes, 0 ausentes** |

M8 e M9 **retratam** a evidência que o [[B-079]] carregava: ela afirmava que não havia checkout
do kit nesta máquina, e isso saiu de um glob errado (`~/Projetos/*/squad*`). O item foi corrigido.

## Decisões resolvidas

1. **D1 — A unidade de propriedade é o PILAR, não o repo.** Por repo há dois alvos, e o maior
   mandaria 66 de 80 itens para um agente genérico que não existe: o gate responderia
   `routed: true` sem nomear ninguém, que é a mesma vacuidade do exit 3. Por pilar, sete dos
   oito domínios já apontam para especialista escrito (M2).
2. **D2 — ~~`route_domain.py` resolve por domínio, com queda para repo.~~ RETIRADA na revisão de
   2026-08-20 — a forma da tabela dispensa a mudança de código. Texto original preservado abaixo.** O alvo casa primeiro
   como domínio; se nenhum casar, cai para repo; e o caminho por repo **recusa nomeando a
   ambiguidade** quando o repo está em mais de um domínio, em vez de devolver o primeiro (M4).
   A precedência preserva consumidores de modelo por-repo — o kit é compartilhado ([[B-079]]).
3. **D3 — Os 26 itens de `domain: theo-db` se redistribuem em quatro destinos** (M6):
   `metodo` → `theo-auditor.md` (4, especialista existe e o remit é idêntico); `arnes` →
   `agents/arnes.md` (13, **a escrever**); `governanca` → `agents/governanca.md` (8 = os 6 do
   kit + B-039 + B-054, **a escrever**); `vetorial` → `theo-recall.md` (1, B-076). O domínio
   `theo-db` desaparece — nome de repo não é pilar, e era ele que criava a rota quebrada.
   **Pré-requisito, não trabalho posterior:** criar domínio sem escrever o especialista
   reproduz o `BROKEN ROUTE` que a mudança existe para consertar.
4. **D4 — Quem governa o status é a EVIDÊNCIA, não a procedência.** Invariante nos dois
   sentidos: `status: raw` ⟺ `evidence: none-yet`. Não é conceito novo — o `--sweep` já
   registra `triaged` com evidência sem passar pelo intake; isto é a generalização honesta.
   Os 26 migram para `triaged` (M7), **item a item com leitura do campo**: o que não for
   medição vira `evidence: none-yet`, não `triaged`. Migrar por script trocaria um registro
   impreciso por um impreciso e confiante.
5. **D5 — ~~A mudança nasce no KIT, e o consumidor vem depois.~~ REESCRITA na revisão de 2026-08-20:
   a mudança é INTEIRAMENTE LOCAL e o `squad` não é tocado. `rules/` e `agents/` são configuração por
   projeto, e o instalador diz isso (M11). Texto original preservado abaixo.** O `route_domain.py` está hoje
   idêntico nos dois (M9); editar aqui primeiro cria divergência no núcleo da mudança, que é o
   defeito que o `check_install_drift.py` existe para denunciar. E tabela por pilar escrita
   antes de o tool saber rotear por domínio é tabela que o tool não honra — trocaria `UNROUTED`
   por rota silenciosamente errada. As 9 divergências de M9 vão junto: são a metade pendente
   do [[B-079]].
6. **D6 — ~~A derivação passa a aceitar repo em vários domínios.~~ RETIRADA na revisão de 2026-08-20 —
   a tabela é escrita à mão, que é o que o gerador autoriza. Texto original preservado abaixo.**
   `detect_domains --from-backlog` recusa hoje na derivação (`detect_domains.py:213-217`) e foi
   o que bloqueou derivar a tabela deste projeto. Derivar `theo-db` sob sete pilares é dado
   correto; o que não se pode é *perguntar por repo* contra essa tabela — e D2 já cobre isso.
   Uma recusa no ponto onde a ambiguidade morde, em vez de duas no ponto errado.
   **Consequência declarada antes do diff:** `test_one_repo_in_two_domains_is_refused` muda de
   sentido — passa a afirmar que a derivação aceita e o roteamento por repo recusa.

## Premissas registradas, não perguntadas

- **Os rótulos de domínio não são renomeados** para casar com o nome do arquivo do agente
  (`engine-pgrx` fica `engine-pgrx`, apontando para `theo-pgrx.md`). A tabela mapeia domínio →
  especialista explicitamente, então renomear 25 itens compraria só simetria cosmética —
  degrau 5 da parsimony ladder.
- **`repo:` continua obrigatório** em todo item. `domain:` diz *quem*, `repo:` diz *onde*; a
  coluna Repos da tabela passa a ser informativa.

## Q&A log

### Q1: A unidade de propriedade é o repo ou o pilar?
**Recomendado**: pilar — roteamento existe para nomear quem faz o trabalho, e por repo o maior
alvo manda 66 de 80 itens para um agente inexistente. Contraponto declarado: por repo não custa
código, só reescrever a tabela — mas compra o verde do gate com a perda da informação.
**Decisão do usuário**: aceito.

### Q2: Como o `route_domain.py` passa a resolver?
**Recomendado**: por domínio com queda para repo, e recusa por ambiguidade no caminho por repo.
Contraponto declarado: mexe em tool do kit, prendendo o conserto ao [[B-079]].
**Decisão do usuário**: aceito.

### Q3: Como os 26 itens de `domain: theo-db` se redistribuem?
**Recomendado**: `metodo` (4) · `arnes` (13) · `governanca` (8) · `vetorial` (1); dois
especialistas a escrever como pré-requisito.
**Decisão do usuário**: aceito.

### Q4: O que governa o status — procedência ou evidência?
**Recomendado**: evidência, com `raw` ⟺ `evidence: none-yet`; migração item a item.
**Decisão do usuário**: aceito.

### Q5: A migração nasce no kit ou no consumidor?
**Recomendado**: kit primeiro, consumidor depois. Contraponto declarado: se preferir desbloquear
este repo antes, a tabela fica escrita e inerte até o kit chegar, e isso precisa estar registrado.
**Decisão do usuário**: aceito.

### Q6: O que a derivação faz com repo em vários domínios?
**Recomendado**: deriva; a recusa migra para a resolução por repo.
**Decisão do usuário**: aceito.

## O que este grill NÃO resolveu

- **[[B-021]]** — o bloco registra `resolvido` com o DoD inteiro entregue e o status diz
  `triaged`. Item único, não modelo; fica para leitura do owner.
- **A ordem de ataque** entre kit e consumidor está fixada (D5), mas o *quando* não.
- **Nenhuma linha de código foi escrita.** O contrato de saída deste arquivo é o `/to-plan`.

## Adendo à revisão — o que sobra fora do escopo, e por quê

| Assunto | Situação após a revisão |
|---|---|
| Meus dois arquivos novos (`--rule` no `check_intake_gates.py` + testes herméticos) | Commitados e verdes aqui. Portar ao kit vira **opcional**, não pendência |
| Os 7 arquivos em que este consumidor está **ATRÁS** do kit | Pré-existente e não causado por nada deste ciclo. Registrado no [[B-079]], não bloqueia |
| Defeito do primeiro-casamento em `route()` (M4) | Continua no tool; deixa de ser alcançável por esta tabela. Registrado, não consertado |
