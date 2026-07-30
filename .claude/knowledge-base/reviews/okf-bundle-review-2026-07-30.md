# Review: bundle OKF (`.claude/knowledge-base/okf/`)

**Data:** 2026-07-30
**Revisores:** 5 agentes em paralelo (citation-verifier, contract-compliance, cross-validation,
taxonomy-recoverability, numeric-integrity)
**Ground truth:** `.claude/rules/okf-knowledge-base.md` — **não há plano**; o bundle foi construído ad-hoc
**Achados:** 34 (BLOCKER 4 · HIGH 11 · MEDIUM 12 · LOW 7)
**Verdict:** `NEEDS_FIXES`

## Desvio declarado do cycle-review canônico

As pré-condições **falharam**: não existe plano, `implement-validate` nem audit de `/code-quality` para este
artefato. A regra inviolável diz "review sem plano é vibes". Prossegui porque **existe** ground truth — o
contrato `rules/okf-knowledge-base.md` — e troquei o conjunto de agentes: `architecture`/`tests`/`wiring` não se
aplicam a um bundle de markdown. Isto está registrado para que ninguém leia este review como um `cycle-review`
canônico.

Todo achado consequente foi **reverificado por mim em disco** antes de entrar aqui, conforme
`techniques/nenhuma-alegacao-sem-medicao` — inclusive os que me favoreciam.

## BLOCKER (4)

### B1 — `sbq-sem-vantagem-in-ram` INVERTE a conclusão do ADR que cita
- **Achado por:** cross-validation + citation-verifier (independentes)
- **O conceito diz:** "a vantagem do SBQ … só aparece **sob pressão de RAM** … Medir in-RAM responde a pergunta errada."
- **A fonte diz** (`docs/adr/0018:18`, `m57-sbq-superiority.md`): *"consistentemente mais lento (0.35–0.77×) em
  **TODOS os regimes medidos — in-RAM E sob pressão de RAM** até 1.3 GB"*. Sob pressão: 0,73× e 0,77×.
- **Mecanismo que a fonte publica e o conceito ressuscita como hipótese:** *"o HNSW tem localidade de acesso → o
  índice f32 não thrasha sob pressão; a premissa 'índice não cabe → I/O por query' NÃO vale."*
- **Impacto:** o conceito instrui o leitor a re-rodar o experimento que o M57 já fechou. É um honest-negative que
  reabre a aposta que ele deveria fechar.
- **Ação:** reescrever "A nuance que salva a técnica"; o ganho do SBQ é **footprint**, e a hipótese
  "pressão converte footprint em QPS" está **falsificada com mecanismo**. Renomear o slug (o "in-ram" codifica um
  recorte que a medição não sustenta).

### B2 — `sbq-sem-vantagem-in-ram`: a tripla `1480 / 1582 / 1641` não existe em artefato algum
- **Achado por:** citation-verifier + numeric-integrity
- **Verificado:** `grep -rn "1480\|1582\|1641" docs/` só acha `m59-raw/m59v4_smoke.json` — que é **M59, não M57**,
  **n=20.000, não 5k**, e com os **rótulos trocados**: 1582 é ponto do **SBQ**, 1641 é do **f32**. O **1480 não
  existe em nenhuma curva vetorial**.
- **Inconsistência interna:** 1480/1582 = **0,935×**, fora da faixa 0,31–0,77 declarada no mesmo arquivo.
- **Ação:** remover a linha; usar o par real do ADR-0018 (in-RAM 500k: SBQ 90 · f32 256 = 0,35×), declarando escala.

### B3 — `pgduckdb-sobre-heap-e-mais-lento`: a faixa `0,52-0,78×` é fabricada
- **Achado por:** cross-validation + numeric-integrity
- **Medido** (`m61-columnar-adoption.md:26-28`, 3 escalas × 3 runs): **0,89× (100k) · 0,66× (1M) · 0,63× (5M)**
  → faixa real **0,63–0,89×**.
- **A conclusão qualitativa sobrevive** (todas < 1); os dois extremos publicados, não. O número está no **título**.
- **Ação:** corrigir para 0,63–0,89× em título, frontmatter, corpo e índice; acrescentar que **piora com a escala**.

### B4 — `amplificacao-maintenance-work-mem`: o multiplicador, a previsão e a alegação de previsão
- **Achado por:** numeric-integrity + cross-validation
- **(a) `mwm × 7` é `mwm × 8`** — exato e independente de unidade:
  `1 (linhas pendentes) + 3,6 (cabeçalhos) + 1 (payload) + 2,4 (alocador) = 8,0`. Os próprios valores publicados
  somam 16,2/2,0 = **8,1**.
- **(b) `~510 MB` pertence à coluna `mwm=64MB`** do issue #221. Ao escrever o conceito troquei o cabeçalho para
  `128MB` e **mantive o valor**. O correto é ~1,0 GB.
- **(c) "A fórmula previu os dois" é falso** — 16,2 GB **subestima** o observado (23,4 GB) em ~31%.
- **Propagação:** o `×7` está também em `config-do-operador-que-inviabiliza-a-medicao` **e no issue público #221**.
- **Ação:** corrigir os três; comentar a correção no #221.

## HIGH (11)

| # | Conceito | Achado |
|---|---|---|
| H1 | `superioridade-vetorial-vs-scann` | "**3 levers** refutados" — a memória consolidada diz **7**; os 4 omitidos são justamente os que um planejador proporia como "não tentados" |
| H2 | `superioridade-vetorial-vs-scann` | "satura em 0,974 a 500k" **superado** pelo ADR-0034 (→ 0,990) — e o próprio conceito credita M60 como alcançado. Contradiz a si mesmo |
| H3 | `estatistica-que-nao-sustenta-a-alegacao` | "3 caminhos independentes" — a fonte diz que o Bonferroni **deixou de derrubar** com a 6ª coleta; **só o clustering** decide |
| H4 | `dados-sinteticos-degenerados` | propaga o erro do SBQ (B1) como "corolário medido" |
| H5 | `durable-rename-fsync-do-diretorio-pai` | "**5 fsyncs**" — são **4** (`fd.c:793, 809, 847, 850`). A conclusão load-bearing se sustenta |
| H6 | `panic-atraves-da-fronteira-c` | "**384 blocos unsafe**" não reproduz: hoje 151 `unsafe {` · 205 `unsafe fn` · 431 tokens. **Herdado do `CLAUDE.md`** |
| H7 | `licenca-agpl-e-study-only` | `rules/reference-provenance.md` **não existe no theo-db** (só no umbrella) — as outras 6 citações `rules/*` resolvem, então a base implícita é inequívoca |
| H8 | `resume-from-discarded-m118` | `ADR-0033` **não registra** o veredito do M118 (nenhum ADR menciona M118); o veredito vive em `m118-resume-discarded.md`. **Herdado da memória** |
| H9 | roteamento | **nenhum gatilho** para "aceitar um verde como evidência" — cenário servido por 4 failure-modes, roteado por zero |
| H10 | roteamento | o ponteiro injetado **omite `build`**, que a regra § 3.2 lista — divergência ponteiro-vs-regra |
| H11 | `nohup-em-ssh-nao-sobrevive` | sem link de entrada de **nenhum conceito** e sem gatilho que roteie ao cenário dele |

## MEDIUM (12) — resumo

Taxonomia e porta de entrada: `index.md` declara `type: OKF Bundle`, **sexto tipo fora da taxonomia LOCKED, com
0 ADRs** autorizando; e a tabela da raiz nomeia os tipos em `minúsculo-hífen` enquanto o disco usa `Title Case` —
filtrar por `type: failure-mode` acha **0 de 17**. Fronteiras vazando: os mesmos dois incidentes do M168 em
`falso-verde-de-script` **e** `medicao-vacuosa-aceita`; protocolo git duplicado em `duas-sessoes-num-checkout` e
`git-switch-nao-checkout`. Proveniência: **2** Measurements sem âncora (`juri-adversarial-precision-039` e
`gap-vs-clickhouse-m159`) — contra a própria `technique/proveniencia-em-todo-artefato`. Tipo errado:
`acervo-local-antes-da-web` é `Technique`, não `Invariant` (e o gatilho de `invariants` nunca dispara para
pesquisa). Rastro: `documentacao-retroativa-como-gate` é majoritariamente estado do M168/M169. Atribuição:
`symqg-in-pg` credita "page tax" ao residual 2,6-3,9×, mas a fonte diz que o page tax foi **mitigado** (8,5× → +2,3×)
e o residual é assimetria de maturidade. Derivação: o teto "~35-39/43" de `min-max-texto-e-colacao` não tem origem
localizável. Nuance: `bm25-na-fusao-rrf` diz "não vence" — no NFCorpus a perna mais forte foi **ativamente pior**
(p=0,0168).

## LOW (7) — resumo

`resume-from-discarded-m118` publica 1,2× onde a fonte diz **1,07–1,22×** (o mesmo arredondamento-para-o-topo que
o bundle condena); `q17-pushdown` diz "um terço" para 37%; `bm25-na-fusao-rrf` diz "storage do **heap**" onde a
fonte diz "**índice**"; citação de `arrow-array` é sensível à versão e não a declara; `gap-vs-clickhouse-m159` não
carrega a ressalva `[NO-BASELINE-COMPARABLE]` da fonte; `deriva-de-box-m168` poderia registrar que 13,6% é n=24.

## O que passou, e merece registro

- **As 20 citações `arquivo:linha` resolvem E sustentam** — incluindo casos exigentes: o literal do regex em
  `check_phase_completeness.py:41`, a linha JSONL verbatim, a distinção entre a cópia theo-db (`:230`) e a do
  umbrella (`:235`) do mesmo script, e as 4 citações pgrx, que batem com **números idênticos em dois trees
  independentes** (acervo v0.19.1 e registry 0.19.0).
- **`deriva-de-box-m168` passou nos cinco eixos estatísticos**: os três rho de Spearman recalculados
  (+1,000 / +0,943 / +0,714), o valor crítico 0,886 a n=6, as **três** quebras de monotonia, os 2,9 pontos, e o
  72/72. E a checagem que o próprio conceito sinaliza como a mais importante — se 13,6% é o **pool** ou a coleta
  mais lisonjeira — **é o pool** (n=24).
- **`ChunkDirEntry` 48 B em memória / 44 B serializado** confirmado no código real (`columnar_codec.rs:108-120`
  e `:27`) — os dois números certos para coisas diferentes, como o conceito diz.
- **A geomean do gap vs ClickHouse é internamente exata**: 32 cobertas a 7,54× + 11 não-cobertas a 303× → 19,40×,
  ao dígito.
- **Nenhum conceito de enchimento**, nenhum ADR travestido de conceito, nenhuma contradição contra
  `parsimony-ladder` / `testing` / `git-safety` / `discover-phd-rigor`.

## O padrão, e ele é o achado mais importante deste review

**Três dos 4 BLOCKER e 4 dos 11 HIGH concentram-se nos conceitos do commit `5c38eee`** — o que minerou 562 MB de
transcripts do diretório irmão em vez do corpus consolidado. Cada um congelou uma **crença intermediária** que a
medição posterior derrubou. (O quarto BLOCKER — o `mwm` — nasceu no commit **fundador** `239d487`, não neste;
corrigido no re-review depois de eu generalizar sem contar.)

| Conceito | A crença congelada | O que a medição depois disse |
|---|---|---|
| SBQ | "só falta medir sob pressão" | a pressão foi medida — 0,73× / 0,77× |
| pg_duckdb | uma faixa que ninguém mediu | 0,63–0,89×, em 3 escalas |
| levers do HNSW | "3 refutados" | 7 refutados |
| saturação 0,974 | estado corrente | superado pelo ADR-0034 |

Transcript é **deliberação em andamento**; memória consolidada e artefato são **conclusão**. Minerar o primeiro
sem cruzar com os segundos produz conceitos que parecem verificados e registram o que se acreditava no meio do
caminho. É `diagnostico-aceito-sem-reproduzir` — aplicado à construção do próprio bundle.

**Dois defeitos são herdados, não criados aqui:** "384 unsafe" vem do `CLAUDE.md` (commit 5ca80b8) e
"M118 → ADR-0033" vem do arquivo de memória. Corrigir só o bundle deixa a origem intacta.

## Buraco de gate identificado

`check_okf.py` passa (60 conceitos, 158 links, exit 0) porque **C2 valida apenas links markdown internos**.
Caminhos citados em prosa e no frontmatter `resource:` não são verificados por nada — foi exatamente ali que o
`rules/reference-provenance.md` inexistente passou. Candidato a **C5**, com superfície de falso-positivo zero.

## Verdict

`NEEDS_FIXES` — 4 BLOCKER. Por `cycle-review.md`, BLOCKER não merge sem correção ou dispensa por ADR explícito.
**Não dispenso nenhum**: os quatro são números ou conclusões que serão citados em decisão, que é precisamente o
risco que o bundle existe para eliminar.
