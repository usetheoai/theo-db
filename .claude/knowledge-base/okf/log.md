---
type: Log
title: Histórico do bundle
description: Registro cronológico de quando cada bloco de conhecimento entrou e o que o motivou.
tags: [okf, historico]
timestamp: 2026-07-30T00:00:00Z
---

# Log

## 2026-07-30 — criação do bundle

Motivador imediato: uma sessão de trabalho no M169 em que **seis** alegações minhas foram derrubadas por medição
(#219, #220 duas vezes, EC-2, "q20 nunca observado", linha fabricada do EC-1, custo do ADR-5), mais **quatro**
defeitos de instrumentação numa única medição de memória. Nenhum deles era novo em espécie — todos tinham
precedente registrado em memória do projeto, e nenhum estava num lugar que disparasse no momento certo.

Fontes consolidadas: 67 arquivos de memória do projeto (M46→M169), o desk-check do M168, as notas de
implementação do M169, e as mensagens de commit da série.

Escopo deliberadamente **não** incluído: planos, reviews, ADRs e audits históricos. Eles continuam em
`knowledge-base/`, no formato do ciclo. Este bundle é sobre **método e invariantes**, não sobre o rastro de
execução.

## 2026-07-30 — o bundle ganha contrato, validador e gates

Criar o bundle não bastava: um bundle que ninguém lê é pior que nenhum, porque produz a sensação de cobertura
sem a cobertura. Três mecanismos foram acrescentados no mesmo dia:

| Peça | O que faz | Grau |
|---|---|---|
| `rules/okf-knowledge-base.md` | o contrato — quando ler, quando escrever, o que é máquina e o que não é | contrato |
| `scripts/check_okf.py` | valida 4 invariantes estruturais (C1 `type`, C2 links, C3 índices, C4 raiz) | **determinístico** |
| `hooks/stop-validation.sh` gate 5 | BLOQUEIA em bundle inválido, e em número publicado sem `Measurement` | **hard gate** |
| `hooks/userpromptsubmit-inject.sh` | injeta o ponteiro a cada turno, ao lado da parsimony ladder | injeção |

O validador tem **controle positivo**: um bundle deliberadamente quebrado tem de produzir exit 1, e produz
(C1+C2+C3 detectados). Sem isso ele seria o `cobertura-alegada-sem-execucao` que este mesmo bundle documenta.

Durante a construção dos testes, **dois** dos meus próprios modos de falha catalogados reapareceram — e é o dado
mais interessante do dia: capturei `$?` de um `tail` num pipeline (`falso-verde-de-script`) e testei o gate de
benchmark com um arquivo não-rastreado, que `ALL_FILES` estruturalmente não vê (`instrumento-cego-a-arquitetura`).
O catálogo pegou os dois porque eu tinha acabado de escrevê-los.

## 2026-07-30 (2) — auditoria de cobertura: 7 lacunas reais encontradas e fechadas

O owner perguntou "todos os aprendizados estão no OKF?". Eu tinha **afirmado** consolidar 67 arquivos de memória
sem nunca verificar entrada por entrada — o `cobertura-alegada-sem-execucao` aplicado ao próprio bundle.

Medido: 10 memórias sem rastro algum. Lidas uma a uma e classificadas:

| Veredito | Quantas | Ação |
|---|---|---|
| lacuna real | **7** | conceito escrito |
| corretamente fora (§ 4.2 — rastro de execução, ou credencial) | 2 | nenhuma |
| falso negativo da minha própria busca | 1 | nenhuma (`m140-4` está coberto sob `Spi`) |

Conceitos acrescentados: `benchmark-nao-prova-que-o-produto-funciona`, `teste-que-passa-pela-razao-errada`,
`fail-open-por-omissao`, `bgworker-transaction-segura-snapshot`, `worker-nao-ve-set-de-sessao`,
`datafusion-sum-int64-faz-wrapping`, `customscan-scanrelid-zero-e-aggref-pullup`.

**Ressalva que fica registrada porque é o dado mais honesto daqui:** a heurística que usei erra nos DOIS sentidos.
Deu falso negativo em `m140-4` (busquei termos do slug; a lição vive sob `Spi::get_one`), e "com rastro" para as
outras 56 significa apenas que **uma palavra apareceu em algum lugar** — não que a lição virou conceito. Logo
**56/66 é teto, não medida**, e a cobertura real das 56 continua não auditada. Além disso, os 110 blueprints e as
mensagens de commit da série **nunca foram varridos** — é superfície maior que a das memórias.

## 2026-07-30 (3) — mineração dos transcripts do projeto irmão

O owner apontou `projects/-home-paulo-Projetos-usetheo-theo-data-theo-db/memory` como fonte de aprendizados.

**Primeiro achado, e ele nega a premissa:** aquela memória é um **subconjunto estrito** da que já foi consolidada
— 64 de 65 arquivos **byte-idênticos**, e o `theo-cloud` ainda tem 2 arquivos a mais. Zero aprendizado novo ali.

**O que de fato não fora minerado:** os **562 MB de transcripts** do mesmo diretório (10 sessões, 4→27 de julho).
Extração de parágrafos com marcador de aprendizado: 497 distintos; 439 após descartar repetição de conceito já
coberto. Sete viraram conceito novo, dois atualizaram conceito existente:

| Novo | O que é |
|---|---|
| `nohup-em-ssh-nao-sobrevive` | `nohup &` dentro de `ssh` morre com o canal — exige `setsid` + verificação de PID. Custou duas corridas perdidas |
| `durable-rename-fsync-do-diretorio-pai` | 5 fsyncs em ordem estrita; o do diretório-pai é o load-bearing. E `durable_rename` NÃO faz PANIC |
| `dados-sinteticos-degenerados` | uniforme satura recall em 1.0 com `probes=1`; sem cluster despenca a 0.033. Nenhum dos dois mede o índice |
| `sbq-nao-ganha-qps-em-regime-algum` | tese ≥2× falsificada: 0,31-0,77× do f32; a vantagem é memória, sob pressão de RAM |
| `pgduckdb-sobre-heap-e-mais-lento` | 0,52-0,78× do row-executor nativo, com plano DuckDB e resultado correto |
| `min-max-texto-e-colacao` | byte-min ≠ collation-min; determinismo não basta. Teto estrutural de ~35-39/43 no ClickBench |
| `juri-adversarial-precision-039` | 11 de 18 achados descartados pelo júri — ~1/3 de acionáveis é o esperado |

| Atualizado (regra § 4.3 — nunca bifurcar) | O que ganhou |
|---|---|
| `deriva-de-box-m168` | a instância do **M46: +122%** de deriva no controle de binário inalterado — 40× maior, e um ano antes |
| `superioridade-vetorial-vs-scann` | a causa-raiz é **problema de pesquisa** (grafo satura em 0,974 a 500k) e **3 levers já refutados** por medição |

**O mais desconfortável:** `nohup-em-ssh-nao-sobrevive` descreve um padrão que **usei várias vezes nesta própria
sessão** para lançar cargas na box de medição. Funcionou por sorte — a lição existia, registrada, e não estava
onde dispararia.

## 2026-07-30 (4) — review adversarial de 5 agentes: 34 achados, todos aplicados

`/review` sobre o próprio bundle, com 5 revisores em paralelo. Pré-condições canônicas falharam (não há plano),
então o ground truth foi `rules/okf-knowledge-base.md`. **34 achados: 4 BLOCKER · 11 HIGH · 12 MEDIUM · 7 LOW.**
Todos aplicados; nenhum dispensado por ADR.

**Os 4 BLOCKER, e o padrão que eles formam:**

| Conceito | O defeito |
|---|---|
| SBQ | **invertia a conclusão do ADR que citava** — a pressão de RAM foi medida e o SBQ perdeu lá também |
| SBQ | a tripla `1480/1582/1641` **não existe em artefato algum** (rótulos trocados de um smoke do M59) |
| pg_duckdb | faixa `0,52-0,78×` **fabricada** — o medido é 0,63-0,89× em 3 escalas |
| `mwm` | `×7` é **×8**; o `~510 MB` pertencia à coluna `mwm=64MB`; "a fórmula previu os dois" era falso |

**Três** dos quatro, mais 4 dos HIGH, concentram-se no commit `5c38eee` (o quarto — o `mwm` — vem do commit fundador `239d487`; a alegação "os quatro" era generalização não contada, corrigida no re-review) — o que minerou **transcripts**. Isso virou o
conceito [crenca-intermediaria-congelada](failure-modes/crenca-intermediaria-congelada.md): transcript é
deliberação em andamento; memória consolidada e artefato são conclusão.

**Correções estruturais além do conteúdo:**

- **C5 no `check_okf.py`** — o valor de `type` tem de estar no conjunto fechado. A porta de entrada declarava
  `type: OKF Bundle`, sexto tipo sem o ADR que o § 2 LOCKED exige, e o C1 (só presença) não pegava. Com controle
  positivo.
- **Dois gatilhos novos na regra § 3.2 e no ponteiro injetado** — "aceitar um verde como evidência" (servido por
  4 `failure-mode`, roteado por zero) e "rodar processo longo em máquina remota". A divergência ponteiro-vs-regra
  (`build` faltando no ponteiro) era o `gate-desligado-em-silencio` aplicado ao próprio bundle.
- **Duas origens herdadas corrigidas na fonte**: `CLAUDE.md` (384 → 151/205/431 `unsafe`) e o **issue público
  #221** (o `×7`), porque corrigir só o bundle deixaria a origem intacta.
- Fronteiras arrumadas (§ 4.3): os casos do M168 voltaram para a casa certa; o protocolo git deixou de ser
  duplicado; `acervo-local-antes-da-web` virou `Technique` (era `Invariant`, e o gatilho nunca disparava para
  pesquisa).

**O que o review confirmou como sólido:** as 20 citações `arquivo:linha` resolvem **e sustentam** (as 4 do pgrx
com números idênticos em dois trees independentes); `deriva-de-box-m168` passou nos cinco eixos estatísticos; a
geomean do gap vs ClickHouse é exata ao dígito; `ChunkDirEntry` 48 B/44 B confirmado no código.

**Durante a aplicação, o gate C2/C3 pegou dois defeitos que eu introduzi** ao renomear um conceito — links
mortos e índice dessincronizado. O mecanismo funcionou contra quem o escreveu.

## 2026-07-30 (5) — re-review: minhas correções introduziram 3 defeitos, um BLOCKER

Re-verificação adversarial do commit `217d449`. Dos 34 achados: **24 corrigidos, 4 parciais, 3 não aplicados,
3 defeitos NOVOS**. Todos tratados.

**O defeito novo que importa (BLOCKER):** ao substituir a faixa fabricada do `pg_duckdb` pela medida, **inverti
as colunas** — publiquei 23,6 ms como DuckDB e 26,4 ms como PG. Com esses rótulos o DuckDB fica *mais rápido*,
contradizendo o próprio título; e a razão 0,89 só fecha com os rótulos da fonte. **É a mesma espécie de defeito
("rótulos trocados") que eu havia imputado ao original.**

Dois defeitos novos auto-referenciais, e num bundle sobre honestidade epistêmica isso pesa: a nota de correção do
SBQ **citava o slug novo como se fosse o antigo**, e a alegação "os 4 BLOCKER concentram-se em `5c38eee`" era
**generalização não contada** — são **3 de 4** (o `mwm` nasceu no commit fundador `239d487`). Repetida em três
lugares, sobre a causa dos defeitos: exatamente a espécie que `crenca-intermediaria-congelada` existe para
prevenir.

**Duas omissões de propagação:** o `~510 MB` foi corrigido no conceito-fonte mas não em `medir-antes-de-filar`,
que passou a afirmar `×8` e `~510 MB` na mesma frase; e o `ARM=stream` saiu de `falso-verde-de-script` mas
ficou duplicado entre `gate-desligado-em-silencio` e `medicao-vacuosa-aceita` — a duplicação mudou de par em vez
de ser eliminada.

**C6 implementado** — o buraco que o review pediu e que eu não fechei (usei o slot "C5" para o achado do `type`).
`resource:` agora é validado, e ele já tinha **duas** vítimas vivas: `rules/reference-provenance.md` e um
`docs/adr/0035` truncado que **seis revisores não pegaram**. C5 e C6 ganharam normalização (aspas, comentário
YAML, âncora de seção) depois que um probe mostrou que rejeitavam YAML legal.

**Lição de método, registrada porque se repetiu duas vezes hoje:** um `str.replace` cuja âncora não casa **falha
em silêncio** — foi assim que a correção do C6 não entrou na primeira tentativa (indentação de 16 espaços, eu
presumi 20). Edit erra alto; `replace` não. E o meu primeiro controle positivo do C6 era **inválido**: copiar o
bundle para `/tmp` quebra a resolução de todo caminho relativo ao repo, então os 6 "achados" eram artefato do
teste, não do gate.
