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
3 defeitos NOVOS**.

> **A frase "Todos tratados" que estava aqui era FALSA** — o round 3 achou três valores refutados ainda vivos
> (`×7` em `chunk-group`, `~510 MB` em `medir-antes-de-filar`, `~31%` no índice de `measurements`). Corrigido, e
> a classe virou conceito: [correcao-nao-propagada-pelo-grafo](failure-modes/correcao-nao-propagada-pelo-grafo.md).

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

## 2026-07-30 (6) — round 3: a classe dominante era a PROPAGAÇÃO, não o conteúdo

Dois revisores independentes (um sobre as correções do round 2, um sem priores). **Três BLOCKER, todos da mesma
classe:** a correção do `×7 → ×8` landou no conceito-fonte e **não** nos três que citavam o valor — inclusive no
`measurements/index.md`, que é a porta de entrada do gatilho "isso já foi medido?". E o `log.md` afirmava
"Todos tratados".

Isso virou [correcao-nao-propagada-pelo-grafo](failure-modes/correcao-nao-propagada-pelo-grafo.md), com regra
operacional mecanizável: **antes de fechar uma correção numérica, `grep` o valor antigo no bundle inteiro**; e
regenerar índices **depois** das edições, nunca antes.

**Dois HIGH que são o bundle prescrevendo o defeito que documenta:**

- `ablacao-mesmo-indice` publicava **2,8×** e **1,2×** — os dois **topos** de faixas medidas (2,4-2,8× e
  1,07-1,22×), maximizando a "correção" narrada. É o arredondamento-para-o-favorável que
  `estatistica-que-nao-sustenta-a-alegacao` condena por nome, cometido na Technique que ensina rigor de ablação.
- `separar-transporte-de-conteudo` prescrevia `rc -ne 0` para detectar falha de canal. **`ssh` devolve o status
  do comando remoto**, e `grep` sem casamento devolve 1 — logo todo poll saudável era contado como
  inacessibilidade. A Technique prescrevia exatamente a confusão transporte↔conteúdo que ela existe para
  prevenir. **E eu rodei esse padrão nos monitores desta sessão.** Corrigido para `rc -eq 255` + `|| true`.

**Achado meu, no meu próprio gate:** o C6 tinha **falso-negativo** — aprovava `../../../.claude/rules/…`, que
resolve só por uma terceira base de fallback que nenhum leitor navega. Restrito às duas bases declaradas
(raiz do repo e `.claude/`), com o `references/` gitignored isento por desenho.

## 2026-07-30 (7) — round 3, segunda varredura: o padrão real era OUTRO

O segundo revisor do round 3 varreu `failure-modes/` e `techniques/` — as categorias que os três rounds
anteriores **não** tinham varrido, porque "os números moram em `measurements/`". Oito achados novos, e um deles
BLOCKER.

**O BLOCKER:** `dados-sinteticos-degenerados` publicava **`0.033`** como recall medido, em quatro lugares, e
certificava a linha como "fiel e medida". **O número não existe em artefato algum** — busca exaustiva em `docs/`
e `benchmarks/` só devolve `0.0333` como **tempo** (cold seconds) do ClickBench; o menor recall de qualquer
artefato vetorial é `0.0634`. Sobreviveu a três rodadas e seis revisores porque ninguém procura número em
`failure-modes/`.

**O diagnóstico dos rounds 1-3 estava incompleto.** Eles concluíram "a origem é transcript"
(`crenca-intermediaria-congelada`) e depois "a classe é propagação" (`correcao-nao-propagada-pelo-grafo`).
Nenhum dos dois explica o `0.033`, o `+11`, o `~3× otimista`, os cinco campos de proveniência ou o M169 sem
audit. A origem desses é outra: **o conceito é escrito com o número que a narrativa pede, e o artefato nunca é
aberto** — e a varredura seletiva por categoria é o que os protegeu.

**Dois casos de o bundle afirmar o que não cumpre:**

- `proveniencia-em-todo-artefato` exigia cinco campos e `cobertura-alegada-sem-execucao` afirmava que "**todo**
  artefato carrega" os cinco. Medido: `m168_collect_all.sh` grava **dois** (`so_md5`, `postmaster`) — `nproc`,
  `free` e `loadavg` dão zero no `grep`. Virou dívida declarada, não regra cumprida.
- `cobertura-alegada-sem-execucao` atribuía dois caps ao **M169** e afirmava o desfecho "tirou os dois caps e deu
  `PASS_WITH_CAVEATS`". **Não existe audit de code-quality do M169** — o milestone está em voo. Reatribuído aos
  que têm audit (M161/M163-M165, M146) e o desfecho removido.

**E o revisor rejeitou um achado próprio** — o subagente dele reportou "33 repos" como errado por ter contado 34
diretórios com `.git`; ele verificou que o `CLAUDE.md` separa deliberadamente os **33 peers** do `FlameGraph`
(ferramenta, não peer), e recusou o achado. Relatar os nove teria sido o `diagnostico-aceito-sem-reproduzir` que
este bundle documenta.

## 2026-07-30 (8) — varredura dos 139 artefatos de benchmark: +14 conceitos

Auditoria de cobertura mostrou que a § 4.1 do contrato estava sendo violada em escala: **139 artefatos de
benchmark, 8 citados**. Três extratores varreram os 53 que carregam veredito, com a regra "todo número vem do
arquivo, com `arquivo:linha`".

**Decisão de desenho:** 139 artefatos **não** viram 139 conceitos. A § 4.1 exige que o número seja
**recuperável**; a § 4.3 manda atualizar em vez de bifurcar; a § 6 avisa que enchimento envenena. Os extratores
classificaram em `HONEST_NEGATIVE` / `MEASUREMENT` / `EXECUTION_TRACE`, e o terceiro balde — "o milestone N
entregou X e o DoD passou" — foi descartado por § 4.2.

**O achado de maior consequência é um ERRO NO BUNDLE, e a cadeia dele:**

| Elo | Diz | Certo? |
|---|---|---|
| artefato `gap1:39` | "**~5× o `ef`** → **~1,8× mais lento**" | ✅ duas grandezas |
| ADR-0035:21 | "**~1,8× o `ef`**" | ❌ fundiu as duas |
| conceito OKF | citava o ADR, **fielmente** | ❌ herdou |

O ADR **cita o artefato que o contradiz**. Meu conceito passava em qualquer verificação de citação — o `resource:`
resolve, a linha diz aquilo. O defeito estava **um elo acima**, e nenhum gate alcança. Virou
[numero-comprimido-na-cadeia-de-citacao](failure-modes/numero-comprimido-na-cadeia-de-citacao.md).

**Os 14 novos, por tipo:**

- `Invariant` (3, o tipo mais escasso): o rename `attrs`→`compact_attrs` do PG18 que **compila** lendo struct de
  104 B sobre array de 16 B; o stub `extern "C-unwind"` sem frame de guarda que derruba **a instância** (e
  `#[pg_guard]` **não pode** ser aplicado em `macro_rules!`); e `maintenance_work_mem` que **não capa** RSS de
  Rust.
- `Failure Mode` (5): número comprimido na cadeia; oráculo que não compara a chave (epoch de 10.957 dias); o A/B
  prova o espaço de **dados** e o review o de **tipos** (5 milestones seguidos); `EXPLAIN ANALYZE` como
  instrumento **assimétrico**; conflação ranker×candidate-set (3 instâncias).
- `Technique` (2): braço de controle **inalterado** (+122% de deriva no binário que não mudou); a **forma da
  curva** diagnostica a causa antes do profiler.
- `Measurement` (2): o limite de escala a 100M (**19/43 vs 43/43** — a taxa de conclusão é o veredito, não a
  razão); os **três** contadores de "cobertura" que não se contradizem.
- `Honest Negative` (2): códigos quantizados **co-locados** não reduzem I/O (e o mesmo quantizador dá 2,3-5,1×
  noutro carrier); híbrida é **dataset-dependente** (p=0,253 vs p=0,0099, e a perna lexical explica).

**Fontes ainda NÃO varridas, para não virar cobertura presumida:** 58 ADRs, 110 blueprints, 173 reviews, 44
implementations, 1601 mensagens de commit.

## 2026-07-30 (9) — varredura dos 58 ADRs: +4 conceitos, e a corroboração do erro

Dos 58 ADRs, a maioria é **decisão de arquitetura** — que por § 4.2 **não vira conceito** (o ADR já é o registro).
O que vira são os ADRs que carregam **veredito medido**. Dez foram cruzados contra o bundle; seis não tinham
cobertura.

**A corroboração que fecha o achado da varredura anterior:** o `ADR-0031:14` registra o número **certo** —
*"precisa ~2× o `ef` a 100k; ~5× a 500k"*, com pgvector 2,13 ms (ef=100) vs theodb 3,16 ms (ef=200) a iso-recall
0,996. Dois ADRs do mesmo pilar, um correto e um comprimido. Isso prova que o defeito é do **elo ADR-0035**, não
do artefato nem da medição — exatamente o que
[numero-comprimido-na-cadeia-de-citacao](failure-modes/numero-comprimido-na-cadeia-de-citacao.md) descreve.

**Os 4 novos:**

- `Technique` **dod-compara-contra-o-oraculo-de-controle** — duas DoDs (M60, M71) tiveram de ser reescritas
  mid-flight por pedirem um **absoluto** que nem o oráculo atinge no mesmo dado: `recall ≥ 0,99` quando o próprio
  pgvector faz **0,988** ali. Reescrever a DoD por medição não é afrouxar o gate — é corrigir o instrumento.
- `Technique` **medir-o-incremento-isolado-antes-de-pagar-o-caro** — o plano do M89 escolheu FFI do `tuplesort`;
  medir o incremento barato **isolado** mostrou que ele ainda OOMava a 4,21× e que as cópias dominantes eram
  outras (16 GB + ~32 GB). O incremento 2, **sem FFI**, bateu o DoD. O caro virou YAGNI **medido**.
- `Invariant` **build-pica-4x-o-dataset-base** — o teto de escala é o **build**, não a query: 30M OOMou a
  **64,7 GB** enquanto o índice final tinha **15 GB**. Dimensionar a box pelo tamanho do artefato erra por 4×, e
  a track inteira ficou `OUT_OF_RAM_QPS_INCONCLUSIVE` porque o regime alvo não era **construível**.
- `Failure Mode` **cold-medido-uma-vez-por-sweep** — `drop_caches` uma vez por sweep mede a **primeira** query
  fria e 99 quentes. O +21% é limite inferior, e o artefato diz isso; citado liso, vira uma afirmação que o
  experimento não sustenta.

## 2026-07-30 (10) — varredura dos 173 reviews: +5 conceitos, e o C2 pegando um link meu inventado

A § 4.1 chama "uma alegação minha derrubada" de **o material de maior valor da série**. Os reviews são
exatamente isso em volume: **110 dos 197 arquivos** mencionam BLOCKER. Varri os BLOCKER/CRITICAL, cruzei as
classes contra o bundle, e **nenhuma das cinco abaixo estava coberta**.

**O gate mordeu em mim, no mesmo commit.** Escrevi `[positive-control-antes-do-veredito]` de memória; o conceito
real chama-se `controle-positivo`. O C2 reprovou os três links antes do commit. É a demonstração mais limpa que
existe de por que C2 é hard gate e não advisory: eu, escrevendo a regra "citação que não resolve não entra",
inventei um nome de arquivo no ato de escrever um conceito sobre não confiar em verde.

**Os 5 novos:**

- `Failure Mode` **allowlist-por-regex-sobre-linguagem** — a MESMA defesa do NL→SQL caiu **duas vezes seguidas**:
  vírgula-join (`FROM documents, secret` — só a 1ª relação era conferida) e identificador entre aspas
  (`FROM "secret"` — a regex exigia `[a-zA-Z_]`, capturou **zero** relações, e a allowlist virou **no-op**).
  O 2º é pior: a defesa não vazou uma relação, **desligou-se inteira**, em silêncio.
- `Invariant` **dois-parsers-da-mesma-string-discordam** — `endpoint_host` parseava a URL pela RFC (userinfo →
  host `api.openai.com`, aprovado) enquanto o cliente HTTP não implementa userinfo e resolvia
  **`169.254.169.254`** na porta 80: o metadata service. Estar certo pela norma é irrelevante — o atacante
  escolhe a string onde os dois discordam.
- `Failure Mode` **assert-que-e-uma-identidade** — duas formas no mesmo review: o assert algebricamente
  equivalente dos dois lados (não podia falhar) e o gate de recall que **não isolava o quantizador** porque
  carrier f32 + rerank dominavam. Ambos verdes, ambos vazios.
- `Failure Mode` **guard-antes-de-materializar-o-pendente** — `scan_ivf_structured` retornava cedo em centroides
  vazios **antes** do fold: índice criado vazio + INSERTs → **zero linhas para dados que existem**, sem erro.
  Resposta errada com cara de certa.
- `Invariant` **granularidade-do-relogio-menor-que-o-evento** — o `pitr-smoke` capturava o alvo no **mesmo
  segundo** do stop do backup e o `--type=time` compara com estritamente-menor. Nenhum retry conserta: só muda a
  probabilidade. O marcador causal (LSN/xid) é exato onde o tempo é aproximado.

Fontes ainda NÃO varridas: 110 blueprints, 44 implementations, 1601 mensagens de commit.

## 2026-07-30 (11) — varredura dos 110 blueprints: +6 conceitos

Os blueprints são **prior art de investigação**. A maior parte deles já foi destilada em ADR ou em código — e
duplicá-la seria o enchimento da § 6. O sinal está nos que carregam **veredito que derruba a própria premissa**
e nos que carregam **causa-raiz medida**. Filtrei por marcadores (`FALSIFIC`, `does NOT`, `não reproduz`,
`BLOCKED`, `NOT repeat`) e cruzei contra o bundle.

**Os 6 novos:**

- `Technique` **primeiro-checkbox-do-dod-e-a-medicao-que-mata** — o padrão **positivo** mais valioso da série. O
  M36 e o M38 escreveram a premissa como **checkbox #1** do DoD, e as duas premissas caíram antes de qualquer
  implementação. O M38 escreveu junto a **cláusula de escalada**, antes de saber o resultado — por isso a
  falsificação virou decisão preparada, não crise.
- `Measurement` **custo-do-scan-vetorial-nao-e-a-distancia** — o número que matou o M36: reads **~50%**, sort
  **~36%**, distância f32 **~15%**, estável em 5 runs × 3 pontos de probes. Qualquer lever sobre o cálculo tem
  teto de 15%; quantizar vale pelo I/O, não pelo score — a confusão entre as duas coisas foi o escopo errado.
- `Failure Mode` **erro-generico-torna-o-bug-irreproduzivel** — o #132 **não reproduziu** (5/5 embeds OK). O
  defeito real era `last_error='embed/upsert failed'` apagando a causa, e um lote de **zero linhas contado como
  sucesso**. O que falhou foi a diagnosticabilidade.
- `Technique` **canario-minimo-separa-codigo-de-plataforma** — 30+ jobs morrendo em 2-3 s com **zero steps**; um
  workflow de um único `echo` também falhou → hipótese "nosso código" **falsificada** em um experimento. Reportado
  BLOCKED, não contornado. Mesmo formato de sinal do runbook Blacksmith (`runner_name` vazio por 24 h).
- `Failure Mode` **benchmark-que-mede-uma-copia-do-codigo** — o bench do pgvectorscale re-implementa a estrutura
  de candidatos como cópia standalone, **sem teste de equivalência**: passa enquanto a produção regride. A saída
  é a costura DIP, que dá código real + grafo byte-idêntico + mesmo processo.
- `Failure Mode` **o-sintoma-nomeia-a-fase-errada** — o #135 dizia "18 min de hang no **planner**" e propunha
  guard por largura. O gdb mostrou recursão no **deparse do EXPLAIN**: das 43 queries só 2 travam, sem `ORDER BY`
  planeja em 27 ms, e a query **executa em 0,537 s**. O guard proposto teria desligado o roteamento colunar
  justamente nas tabelas largas — sem tocar a recursão.

Fontes ainda NÃO varridas: 44 implementations, 1601 mensagens de commit — ambas majoritariamente **rastro de
execução**, que a § 4.2 exclui por construção.

## 2026-07-30 (12) — as duas fontes restantes, verificadas antes de excluir

**Não declarei exclusão sem olhar.** As 44 `implementations`: **2** mencionam lição/surpresa — as outras 42 são
rastro de execução, que a § 4.2 exclui **por construção**, não por falta de tempo. As **1601** mensagens de
commit: filtradas por `falsific|honest-negative|não reproduz`, **30** casam, e todas menos uma já tinham conceito
(m118, m57, m59, m60, m73).

A exceção virou o conceito que fecha a varredura:

- `Honest Negative` **rerank-de-segunda-ordem-piorou** — `ai.rerank` **PIOROU** o nDCG@10 em **3,8 pt**
  (0,7327 → 0,6947) e custou **1953 ms p50** por query, em três runs **idênticos** (determinístico, logo não é
  ruído). O `Recall@50` é **igual** nos dois braços — o reranker só reordena, então todo o Δ vem da ordenação, e
  ela piorou. Foi shipado assim mesmo, com o veredito na própria release (v0.55.0 rotulada honest-negative).

**Estado da cobertura.** Todas as fontes que a § 4.1 obriga foram varridas: 67 memórias, 139 benchmarks (53 com
veredito), 58 ADRs, 173 reviews, 110 blueprints, 1601 commits (filtrados), 44 implementations (verificadas). O que
resta fora é o que a § 4.2 exclui: rastro de execução e decisão de arquitetura.

## 2026-07-30 (13) — a lição dominante do M169: corrigir a instância e não a classe

Escrito porque aconteceu **cinco vezes numa sessão**, e as cinco foram pegas pelo sistema, não pela minha atenção.

O formato: um revisor exibe UM caso; o fix fecha aquele caso; os irmãos do mesmo formato continuam vivos — agora
com a falsa sensação de já terem sido revisados. É invisível pelo mesmo motivo que o original: se o revisor
tivesse visto os irmãos, teria apontado os irmãos.

| Mostraram | Corrigi | O irmão que ficou |
|---|---|---|
| `timeout=60` no `wc -l` de 69,7 GB | o `wc` | **`_psql_int`** com os mesmos 60 s — e `count(*)` a 100M leva ~2100 s, então a checagem de dataset **nunca podia** funcionar na escala para a qual foi escrita |
| — | nada (achei o `_sh` correto) | rotulou mal **três** comandos, incluindo `systemctl is-enabled` de uma unidade **corretamente mascarada** |
| parâmetro novo em `_psql_int` | a assinatura de `_psql` | o **corpo** não repassava; quebrou só na box |

A regra que fica é **varrer, não prestar mais atenção**: ao receber um achado, nomear a CLASSE antes de escrever o
fix, `grep` por ela, e dizer no commit quantos irmãos foram encontrados — inclusive zero.

Apliquei a regra ao próprio commit: varri as três classes nos meus arquivos do M169. Resultado: 1 irmão por classe
já corrigido, nenhum novo (o `wc` passa `WC_TIMEOUT_S`; `nproc`/`free`/`df` são sub-segundo; nenhum outro comando
do coletor codifica estado no código de saída; os 3 chamadores de `_psql` passam o timeout).

Duas de formato vizinho ficaram registradas dentro do conceito, porque não são instância-vs-classe e sim
"saber não impede": nome de conceito escrito de memória (**três** vezes, três vezes pego pelo C2) e comando longo
em foreground remoto (documentei o invariante de manhã, repeti o erro uma hora depois).

## 2026-07-30 (14) — a varredura de consistência me pegou contradizendo a própria disciplina

Apliquei a regra do `correcao-nao-propagada-pelo-grafo` aos números que publiquei hoje, e ela achou um caso — em
mim, não no bundle.

Eu havia **decidido explicitamente não publicar** o tempo do `count(*)` a 100M, porque as duas observações tiveram
janela sobreposta com outro processo (a primeira com um órfão, a segunda com o resto da primeira). Depois escrevi
**"~35 min MEASURED"** em três comentários do `m169_box_attest.py`.

`MEASURED` superestima o rigor: o que existe é uma **ordem de grandeza** sólida e um número exato que não é. Um
teto de timeout precisa apenas da ordem de grandeza, então o código foi corrigido para dizer "tens of minutes" e
explicar por que a precisão está ausente — em vez de emprestar autoridade que a medição não tem.

A lição de método: a disciplina declarada numa mensagem de commit não se propaga sozinha para os comentários de
código escritos meia hora depois. É a mesma classe do conceito, aplicada ao meu próprio texto.

## 2026-07-30 (15) — testei os GATILHOS contra os erros da própria sessão, e achei o buraco

O bundle existe para morder no momento de uso. A pergunta que fecha o ciclo não é "o conceito existe?" mas
"algum gatilho aponta para ele **quando eu preciso**?". Testei os sete gatilhos declarados contra os cinco erros
que cometi hoje:

| Erro | Roteado? |
|---|---|
| `timeout=60` para operação de dezenas de minutos | **parcial** — "processo longo em máquina remota" leva ao invariante do ssh, não à escolha do teto |
| `CASE ... ELSE 1/0` dobrado no planejamento | **nenhum** |
| nome de conceito escrito de memória (3×) | nenhum — mas o gate C2 pega, determinístico |
| "MEASURED" sobre número contaminado | **sim** — "publicar qualquer número" |
| corrigir a instância e não a classe | **nenhum** |

**O último é o achado.** O conceito `corrigir-a-instancia-e-nao-a-classe` foi escrito hoje, e nada apontava para
ele no momento em que ele serviria — que é *antes de escrever o fix de um achado de review*. Um conceito que
existe e para o qual nada aponta é um conceito que não morde. Foi a classe que se repetiu **cinco vezes** numa
sessão, sempre pega pelo revisor, nunca por mim.

Dois gatilhos novos: **"corrigir um achado de review / medição"** (aponta direto ao conceito) e **"escolher um
timeout, um teto de recurso ou um tamanho de box"** (aponta a `measurements` — a escala pode já estar medida).

E um `Invariant` novo, do erro que eu peguei antes de rodar: o PostgreSQL **dobra expressões constantes no
planejamento**, então `CASE ... ELSE 1/0` dispara mesmo quando o ramo não é tomado. A direção do erro é a cara:
o gate reprova SEMPRE, antes e depois do fix, e o sintoma se lê como *"o fix não funcionou"* — mandando caçar um
defeito inexistente no código recém-corrigido. É a forma invertida do `teste-que-passa-pela-razao-errada`.

## 2026-07-30 — `contagem-agregada-mistura-classes-de-falha` (Failure Mode, novo)

Escrito **durante** o baseline do M169, aos 24 de 43, e deliberadamente **antes** do número final. O baseline
mediu 6 falhas, das quais **apenas 1** (o q20, `byte array offset overflow`) é a classe que o M169 conserta.

A métrica-resumo escolhida pelo milestone — "N de 43 completam" — soma as duas classes. Se o T4.1 reportar
`19 → 21`, duas unidades podem ter mudado por qualquer coisa menos o streaming. O conceito registra a regra:
capturar o discriminador por unidade na mesma corrida, publicar o delta **por classe**, e declarar o recorte
antes do resultado.

### Corrigido na mesma sessão, antes de qualquer consumo

A primeira redação deste conceito dizia *"5 das 6 falhas não entram no caminho colunar"*, lendo `agg_routed=False`
como "não roteia". **Errado.** O booleano chaveia em `theodb_columnar_agg` e responde apenas *"entrou no pushdown
AGREGADO?"*. Lendo o SQL: q19 é um scan (`SELECT UserID WHERE UserID = …`) e q23 é um top-k
(`SELECT * … ORDER BY … LIMIT 10`, a forma do M158/M168) — em ambos `False` é o valor **esperado por construção**,
não evidência de nada.

A conclusão macro sobreviveu (1 de 6 é do M169); o raciocínio que a sustentava, não — e uma conclusão certa por
motivo errado não avisa quando o motivo deixa de valer. O conceito ganhou a instância meta (escrevi a regra e a
violei na mesma iteração), um item de regra novo — *escreva a pergunta exata que o discriminador responde* — e
o corolário: **não existe discriminador para o caminho top-k**, então hoje o harness não sabe dizer se o q23
roteou e é lento ou se declinou.

## 2026-07-31 — o gêmeo heap: um `Permission denied` que apontava para o arquivo errado

+2 conceitos, ambos de uma única falha: a recarga do `hits_heap` a 100M abortou lendo um TSV de 70 GB em
**644, world-readable** — porque o diretório-pai era `/root`, modo `700`. O erro nomeia o **arquivo**, e quem
confere o `ls -l` dele conclui que permissão não é a causa; a resposta está um nível acima, onde a mensagem não
olha. Virou [invariant/ler-arquivo-exige-x-em-todo-o-caminho](invariants/ler-arquivo-exige-x-em-todo-o-caminho.md),
com o corolário de que `\copy` (cliente) e `COPY` (servidor) leem por processos diferentes — trocar um pelo outro
troca quem precisa da permissão, e uma carga que passou como `root` falha como `pgtest` sem que nada tenha
regredido no produto.

O agravante não foi a permissão: foi a **ordem**. O script dropou e recriou `hits_heap` **antes** de provar que
conseguia ler a fonte, e o aborto deixou uma tabela de **0 linhas** onde antes não havia tabela nenhuma. O gate
de atestação — escrito nesta mesma sessão — lê os dois estados de formas diferentes: ausente é `hits_heap_absent`
(tolerável), 0 linhas é `hits_heap_rowcount_mismatch` ("a carga perdeu linhas"), que manda perseguir um bug de
COPY inexistente. A falha converteu *não fiz* em *fiz errado*. Virou
[failure-mode/destruir-antes-de-provar-a-precondicao](failure-modes/destruir-antes-de-provar-a-precondicao.md),
explicitamente distinguido do vizinho `guard-antes-de-materializar-o-pendente`: lá o guard roda cedo e **julga**
estado parcial; aqui o passo destrutivo roda cedo e **cria** estado parcial — a correção de um é mover para
depois, a do outro é mover para antes.

Terceiro fato, contra a minha própria intuição: mover 70 GB de `/root` para `/srv` foi **instantâneo** — mesmo
filesystem, `rename(2)`, `df` inalterado em 105 GB usados. "É grande demais para mover" era suposição, não
medição.

## 2026-07-31 (2) — o watcher que esperava por si mesmo

+1 conceito. Para aguardar um build remoto usei `until ! pgrep -f "cargo build"; do sleep 20; done`. O build
terminou em 2m11s com 0 erros e o laço **continuou girando**: `pgrep -f` casa contra a linha de comando inteira de
todo processo, inclusive a do shell que executa o laço — cujo `argv` contém a string por construção. O watcher se
enxergava e concluía que o alvo estava vivo.

O que torna este caro é o sintoma ser **ausência**: nada falha, nada loga, o exit code nunca chega, e "ainda
rodando" é a leitura mais plausível. Além disso é 100% reprodutível, então repetir confirma a conclusão errada.
Virou [invariant/pgrep-f-casa-com-o-proprio-watcher](invariants/pgrep-f-casa-com-o-proprio-watcher.md), ligado à
classe maior — o instrumento que se inclui na própria medição, a mesma de
`vmrss-de-backend-pg-inclui-shared-buffers`.

**Nota sobre mim:** ao escrever os "Relacionados" inventei de novo um nome de arquivo
(`o-instrumento-nao-observa-a-coisa-que-se-quer-medir`; o real é `instrumento-cego-a-arquitetura`). É a quarta
ocorrência da mesma classe nesta base, e as quatro foram pegas pelo gate C2, nunca por mim. O gate é o que
funciona aqui; a intenção não é.

## 2026-07-31 (3) — a pergunta que o benchmark não faria

+1 `Measurement`. O M169 fez o agregado consumir por chunk-group, o que muda a ORDEM de acumulação — e adição
IEEE-754 não é associativa. Se `sum(float8)` dependesse do tamanho do chunk-group, o milestone teria trocado um
defeito **barulhento** (o overflow de offsets) por um **silencioso**, que é pior.

Medido: **idêntico bit a bit** (`sum=2.00000000000001e+17`, `avg=8000000000000.04`) sobre dado adversarial —
`0.1`, que não é representável em binário, com `1e17` esparso, onde `ulp(1e17)=16` faz a ordem decidir se os `0.1`
sobrevivem. Controle positivo de 1 ULP aborta o gate, que é o que impede o "idêntico" de ser um verde vazio.
Registrado com o limite explícito: uma forma medida, não prova para toda entrada.

O detalhe que vale reter: **o meu próprio RED mascarava isso**. Ele comparava `round(avg(f)::numeric, 9)`, e o
arredondamento apaga exatamente a divergência procurada. O gate novo compara `::text` com
`extra_float_digits = 3` — no PG ≥ 12 essa é a representação mais curta com round-trip exato, então igualdade de
texto é igualdade de bits. Instância de
[o A/B prova o espaço de dados, não o de tipos](failure-modes/ab-prova-o-espaco-de-dados-nao-o-de-tipos.md): as
colunas de SUM/AVG do ClickBench são todas inteiras, então nenhum volume de dados dele faria essa pergunta.

## 2026-07-31 (4) — a pergunta larga, no mesmo binário

O conceito de float ganhou a metade larga em vez de virar arquivo novo (é a mesma classe: *o streaming mudou algum
resultado?*, em escopos diferentes). `benchmarks/columnar_type_ab.py` rodou com a GUC no default **on**, então todo
caso atravessou o caminho novo: **35/35 como esperado**, `positive_control diverged=2`, cobrindo int2/int4/int8,
float4/float8, bool, texto, temporais, colação nomeada, `IN`-list, `const_out`, group-expr e top-k.

Duas coisas operacionais foram anexadas ao conceito porque já custaram tempo. O harness **DROPa e recria `hits`** —
apontá-lo para a base do ClickBench destrói a tabela, o que aconteceu duas vezes no M167; hoje há um guard que
recusa, e ele funcionou. E rodá-lo como `pgtest` a partir de `/root/theo-db` bate exatamente no invariante escrito
horas antes ([bit x em todo o caminho](invariants/ler-arquivo-exige-x-em-todo-o-caminho.md)) — corrigi o TSV e não
a árvore inteira, que é a
[instância corrigida sem a classe](failure-modes/corrigir-a-instancia-e-nao-a-classe.md) outra vez.

Terceiro, menor e recorrente: escrevi `cmd | tail; echo $?` e reportei `RC=0` para um script que falhara — `$?`
depois de um pipe é o status do **último** comando, o `tail`. É `falso-verde-de-script` na forma mais barata de
cometer.

## 2026-07-31 (5) — a hipótese que virou número, e o incentivo escondido no custo

+1 `Measurement`. Eu havia escrito que **não** afirmaria por que o guard da recarga levava 35 minutos, porque era
palpite. Medi: `count(*)` sobre as mesmas 99.997.497 linhas leva **11,4 s** com `theodb.enable_columnar_agg = on`
e **>948 s** sem — e o número rápido saiu *sob contenção*, com o backend lento ocupando um núcleo inteiro, então é
teto e não melhor caso.

O que fecha o diagnóstico não é o tempo, é o `pg_stat_activity`: **99,9% de CPU com `wait_event` nulo** elimina
I/O e aponta materialização linha a linha — a mesma conclusão do flamegraph do M148, agora por um segundo caminho
independente.

A lição de segunda ordem é a que vale reter: **um guard de 35 minutos é um guard que alguém vai pular**. Custo de
execução alto não é neutro — ele compra o incentivo errado, e o defeito aparece como "a verificação foi
desativada", não como "a verificação era lenta".

**Nota sobre mim, quinta ocorrência:** inventei outro nome de arquivo nos Relacionados
(`configuracao-do-operador-torna-inmedivel`; o real é `config-do-operador-que-inviabiliza-a-medicao`). Diferença
desta vez: conferi com `grep` **antes** de rodar o gate, em vez de descobrir pelo gate. É a primeira das cinco em
que a checagem foi minha.
