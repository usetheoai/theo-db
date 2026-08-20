---
slug: registry-ownership-model
date: 2026-08-20
questions_asked: 6
decisions_resolved: 6
verdict: READY_FOR_PLAN
items_unblocked: [B-077, B-079, B-080]
---

# Grill: o modelo de propriedade do registro

Desbloqueia o [[B-077]] (17 dos 18 blockers do `/backlog-review`) e a classe
`raw_with_evidence` (26 majors). Escopo: quem é dono de um item e o que governa o status.
Fora de escopo, deliberadamente: o `status` do [[B-021]] — item único, não decisão de modelo.

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
2. **D2 — `route_domain.py` resolve por domínio, com queda para repo.** O alvo casa primeiro
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
5. **D5 — A mudança nasce no KIT, e o consumidor vem depois.** O `route_domain.py` está hoje
   idêntico nos dois (M9); editar aqui primeiro cria divergência no núcleo da mudança, que é o
   defeito que o `check_install_drift.py` existe para denunciar. E tabela por pilar escrita
   antes de o tool saber rotear por domínio é tabela que o tool não honra — trocaria `UNROUTED`
   por rota silenciosamente errada. As 9 divergências de M9 vão junto: são a metade pendente
   do [[B-079]].
6. **D6 — A derivação passa a aceitar repo em vários domínios; a recusa muda de lugar.**
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
