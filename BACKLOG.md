# BACKLOG — TheoDB

Registro de trabalho de manutenção deste repositório: uma linha por hipótese, com dono e critério de
fechamento. **Ids são monotônicos, nunca reusados, nunca renumerados** — um item morto guarda o número
dele para sempre, porque o número é o rastro.

Um item aqui é **hipótese, não compromisso**. Ele nasce com `evidence: none-yet` por desenho; provar ou
matar é trabalho do `/discover`.

## Divergência declarada do `cycle-backlog.md`

A regra desenha **um registro único no umbrella**, cobrindo os 21 repositórios, e o `/backlog-init`
recusa dentro de um repo governado — *"a per-repo backlog is the fragmentation the single registry exists
to prevent"*.

**Este arquivo diverge disso, por decisão do owner em 2026-08-08.** Duas razões o sustentam:

1. O `/backlog-init` está **bloqueado no umbrella** — `theo-platform/` não tem `CHANGELOG.md`, e o Step 0.3
   falha. Foram quatro invocações recusadas antes desta decisão.
2. `knowledge-base-location.md` estabelece que **consumidores não compartilham knowledge-base**: cada
   projeto é dono do próprio `ROADMAP.md` e do próprio `.claude/knowledge-base/`. Um backlog local é
   coerente com essa autonomia; o que ela proíbe é um artefato de ciclo referenciar outro repositório.

O custo da divergência é real e fica dito: **um achado que cruze repositórios não tem onde morar aqui.**
Quando o umbrella ganhar `CHANGELOG.md`, este arquivo deve ser reavaliado — migrado ou mantido com escopo
explícito.

## Fronteira com o `ROADMAP.md`

Os dois existem e **não são intercambiáveis**:

| | vai para |
|---|---|
| trabalho de produto, com marco e Definition of done | **`ROADMAP.md`** (`M<N>`) — é o que `cycle-release` e `cycle-acceptance` leem |
| hipótese de manutenção, ainda sem evidência | **este arquivo** (`B-NNN`) |

Um item que já tem escopo de milestone nasce no roadmap, não aqui. Um item que precisa ser **medido antes
de virar escopo** nasce aqui e sobe depois.

## Como um item chega

- **`/backlog-item {slug}`** — humano, hipótese sem evidência
- **`/discover --sweep {pilar}`** — achado já medido, entra com `source: discover-*`

Schema, transições de status, gates G1–G5, vereditos e rollback: `.claude/rules/cycle-backlog.md`. Este
arquivo é dado; o contrato vive na regra.

## Roteamento por pilar

O escopo é um repositório, então o `domain` do schema é **o pilar**, não um repo do ecossistema. Cada um
tem especialista registrado em `.claude/agents/` (fora do versionamento — ver `.gitignore:14`).

| Pilar | Superfície | Especialista |
|---|---|---|
| `vetorial` | `theodb_rs/src/ann/`, `src/vec/`, quantização, recall | `theo-recall` |
| `hot-path` | `src/ann/scan_core.rs`, `src/am/page/`, SIMD, layout | `theo-hotpath` |
| `concorrencia` | `src/am/lock.rs`, `am/scan.rs`, build paralelo, HTAP | `theo-concurrency` |
| `colunar` | `src/am/columnar*.rs`, zonemap, DataFusion, Parquet | `theo-columnar` |
| `lexical` | `lexical_core/`, `src/lexical/`, `src/hybrid.rs`, BM25/RRF | `theo-lexical` |
| `ai-surface` | `src/{ai_op,chat,embed,rerank,nl,vectorizer,egress}.rs`, `sql/` | `theo-ai-surface` |
| `engine-pgrx` | FFI, `unsafe`, crash-safety, superfície SQL, upgrade | `theo-pgrx` |
| `acervo` | `wiki/`, conceitos OKF, proveniência | `theo-wiki` |
| *(transversal)* | auditoria de qualquer número publicado | `theo-auditor` |

Um item que abranja dois pilares **é dois itens** (gate G3).

## Items

## B-001 — `cargo pgrx test` não roda: o binário de teste morre em `CurrentMemoryContext`   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: bug
source: human
evidence: reproduzido em 2026-08-09 no builder do próprio `Dockerfile` — `cargo pgrx test pg18 <filtro>` falha com `symbol lookup error: undefined symbol: CurrentMemoryContext`
why_now: a suíte tem 310 testes e **nenhum deles roda localmente** pelo caminho documentado. Descoberto ao tentar validar 6 testes novos do `parquet.rs`; confirmado como **pré-existente** rodando `cargo pgrx test pg18 sq8` — teste que existe desde antes — com todas as mudanças da sessão revertidas via `git stash`. Uma suíte que só roda no CI é uma suíte cuja regressão só aparece depois do push.
status: raw
dod:
  - `cargo pgrx test pg18 parquet` executa e reporta resultado de teste (passou ou falhou), em vez de morrer no carregamento
  - a correção é verificada num teste pré-existente (`sq8`), não só nos testes novos
  - o caminho que funcionar fica documentado em `scripts/pgrx-test-in-builder.sh`, que hoje descreve uma receita que não chega a executar

> Registered 2026-08-09 by `/backlog-item` (slug: `theo-db-pgrx-test-nao-executa`).

**Três hipóteses já testadas e REFUTADAS** — registradas para que ninguém as repita:

| # | hipótese | resultado |
|---|---|---|
| 1 | falta `.cargo/config.toml` com `-Wl,--unresolved-symbols=ignore-all` | **muda o sintoma, não resolve**: o link passa a completar, e a falha migra de *link-time* (`undefined symbol: FreeErrorData/FlushErrorState/pfree`) para *runtime* (`CurrentMemoryContext`). É progresso diagnóstico, não correção |
| 2 | o bootstrap `pub mod pg_test` está sob `#[cfg(test)]` enquanto os 56 módulos de teste usam `cfg(any(test, feature = "pg_test"))` | **sem efeito.** O desalinhamento é real e provavelmente deve ser corrigido de qualquer forma, mas não é a causa |
| 3 | `crate-type = ["cdylib"]` sem o `"lib"` que o template do pgrx traz | **sem efeito** (rebuild confirmado por mudança de hash do binário) |
| 4 | falta `src/bin/pgrx_embed.rs` com `::pgrx::pgrx_embed!()`, arquivo que a fixture de pacote do `cargo-pgrx` traz | **a macro NÃO EXISTE no pgrx 0.19** — `error[E0433]: cannot find pgrx_embed in pgrx`. A fixture onde a encontrei é de versão antiga: os arquivos irmãos são `expected-0.16.0.toml` e `expected-0.16.1.toml`. **O cargo-pgrx 0.19 ainda distribui fixtures da 0.16**, o que as torna fonte enganosa para a versão em uso |

**Nota de método sobre a hipótese 4** — ela foi a mais informativa antes de cair. Com o `pgrx_embed.rs`
presente, o erro **migrou** de runtime (`CurrentMemoryContext`) de volta para link-time (`errmsg`,
`errfinish`), mostrando que o arquivo mudava de fato o caminho de build. Só ao combiná-lo com a flag de
link (hipótese 1) o erro real apareceu — e era que a macro não existe nesta versão. **Duas mudanças
combinadas revelaram o que nenhuma isolada mostrava.**

**O que a evidência sugere:** o binário de teste é executado como processo standalone em vez de carregado
pelo Postgres. As quatro hipóteses mexeram em **como o binário é construído**; nenhuma tocou **quem o
executa**.

**Por onde a próxima investigação deve começar:** ler o código do `pgrx-tests` 0.19 em
`~/.cargo/registry/.../pgrx-tests-0.19.0/` para ver como ele inicia o servidor de teste — em vez de inferir
por fixtures. A hipótese 4 caiu exatamente por confiar numa fixture de versão antiga que o próprio
`cargo-pgrx` 0.19 continua distribuindo.

> **Atualização 2026-08-09 — 3 hipóteses novas testadas no ambiente correto, o mecanismo ficou claro, o item
> NÃO fechou.** As 4 refutações anteriores foram todas no host, onde o `cargo pgrx init` nunca completou.
> Repetindo no **builder do próprio Dockerfile** (PG18 + `pgrx init` reais):
>
> | etapa | antes | agora |
> |---|---|---|
> | compilar o binário de teste | falhava | **passa** |
> | linkar | `undefined symbol: pfree, palloc0, error_context_stack` | **passa** com `RUSTFLAGS="-Clink-arg=-Wl,--unresolved-symbols=ignore-all"` |
> | executar | `undefined symbol: CurrentMemoryContext` | **continua falhando**, idem como não-root e via `cargo pgrx test` |
>
> **Mecanismo entendido:** o binário de teste é standalone e a lib referencia globais do PostgreSQL. Nenhuma
> flag de link resolve — resolver o link não faz o símbolo existir em tempo de execução. Só executar dentro de
> um backend PG resolve, e é para isso que o `cargo pgrx test` existe — mas no nosso projeto ele roda o binário
> standalone em vez de embarcá-lo, e **por que** é a pergunta que continua aberta.
>
> **Falso rastro descartado:** o `E0133` que aparece no log são *warnings* de `rust_2024_compatibility`, não
> erros. Quase os persegui como causa.
>
> **Hipótese testada em 2026-08-09 — e ela responde a pergunta estrutural.** `cargo pgrx new probe` no MESMO
> builder, mesmo usuário, mesmo `pgrx init`:
>
> | etapa | projeto de referência | **nosso** |
> |---|---|---|
> | carregar o binário de teste | **passa** | falha — `undefined symbol: CurrentMemoryContext` |
> | executar o harness | **passa** — `0 passed; 1 failed` | nunca chega (exit 127) |
> | onde falha | dentro de `pgrx-tests/framework.rs:425`, **já em execução** | no carregamento dinâmico |
>
> São falhas de categorias diferentes: a referência passa do carregamento dinâmico, nós não. **O bloqueador é
> do nosso projeto, não do ambiente** — a primeira resposta direcional que este item tem, depois de 7
> hipóteses refutadas.
>
> O `crate-type` foi comparado e é idêntico (`["cdylib"]` nos dois), então não é ele. A falha própria da
> referência em `framework.rs:425` é provavelmente ambiental (subir postgres no contêiner) e acontece
> **estritamente depois** — não enfraquece a comparação.
>
> **Próximo passo, não feito:** diffar dependências e `lib.rs` entre os dois — o que a nossa crate linka que a
> referência não linka é a superfície onde o símbolo entra. Com o lado do ambiente eliminado, é busca num
> espaço fechado.

## B-002 — O objetivo: definir e medir o que torna o TheoDB **atrativo**, já que superar todo benchmark é impossível   [ ]

domain: acervo
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: o M73 mediu que superar o ScaNN/AlloyDB no vetorial é **não-alcançável** por extensão PG permissiva (gap 25–44× a recall 0.99, causa de paradigma). O owner reformulou o alvo em 2026-08-09: não precisamos vencer todos os benchmarks, precisamos ser atrativos. Hoje não existe artefato que diga **o que** torna o produto atrativo nem **como se mede** isso — e sem essa definição os demais itens deste lote otimizam eixos escolhidos por intuição. O ADR-0033 (reposicionamento do North Star) segue proposto, sem assinatura, desde 2026-07-10.
status: raw
dod:
  - um ADR assinado define os eixos de atratividade e, para cada um, a medição que o sustenta
  - cada eixo tem um número medido ou um `não medido` explícito — nenhum eixo fica em afirmação
  - os itens B-003..B-010 são repriorizados contra esse ADR, e os que não servem a nenhum eixo são mortos

> Registered 2026-08-09 by `/backlog-item` (slug: `objetivo-atratividade-medida`).

## B-003 — Vetorial: o teto é o build, não a busca — ≥100M nunca foi atingido   [ ]

domain: vetorial
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: o M88 mediu 16M como o maior índice viável e sofreu 2 OOM-kills a 30M; o M89 derrubou o pico de 4,21× para 1,28–1,50× do base, mas registrou honestamente que **não** é `O(maintenance_work_mem)` — a cópia 1× de `idx.vectors` continua no pico, e ~51 GB de base a 100M não cabe em RAM commodity. O recall a escala real segue sem medição porque o índice não constrói.
status: raw
dod:
  - build de 100M completa num box de RAM commodity, com pico anon-rss medido
  - recall@10 medido a 100M sobre dados ANN reais, não sintéticos (a recall sintética do M88 foi tie-degenerada, 0.291)
  - zero regressão de recall a ≤1M (A/B same-data)

> Registered 2026-08-09 by `/backlog-item` (slug: `vetorial-teto-build-100m`).

## B-004 — Lexical: qualidade de recuperação nunca foi medida contra um corpus público   [ ]

domain: lexical
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: o pilar entra no binário default em 2026-08-09 (M186), e a partir daí quem instala recebe `bm25_build`/`bm25_search`. O que existe medido é engine (M140.3 — cache MVCC, ganho que escala) e robustez (M140.4 — crash/VACUUM/MVCC contra o binário embarcado). **Qualidade de recuperação não.** O M184 registrou o eixo como estruturalmente aberto para este pilar. Expor superfície pública sem saber sua qualidade foi exatamente o defeito que o M184 mediu no SymQG.
status: raw
dod:
  - nDCG@10 medido sobre pelo menos um dataset do BEIR, com o comando de reprodução no artefato
  - comparado contra o `ts_rank_cd` nativo do Postgres no mesmo corpus — o baseline que o usuário já tem
  - o resultado é publicado mesmo se for pior: um honest-negative aqui vale mais que a ausência do número

> Registered 2026-08-09 by `/backlog-item` (slug: `lexical-qualidade-beir`).
>
> **Medido no mesmo dia** (`wiki/benchmarks/m186-lexical-ndcg-scifact-verdict.md`): nDCG@10 **0,6269** contra
> **0,3016** do `ts_rank_cd` nativo, sobre 300 consultas do BEIR SciFact com julgamento humano. Delta +0,3253,
> bootstrap pareado p < 0,0001. Dois dos três DoD estão cumpridos — falta a generalização, porque um dataset
> não é uma curva. **O item permanece `raw`**: SciFact é pequeno e de domínio científico, e o achado lateral
> (a superfície não expõe busca multi-termo) virou trabalho de produto que ainda não foi feito.

## B-005 — Híbrido: o ganho da fusão sobre o vetorial puro é estatisticamente não-significativo   [ ]

domain: lexical
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: o M123 mediu o ganho da fusão RRF sobre o vetorial puro e o teste pareado não o sustentou. É o pilar mais frágil do produto **e o único que está exposto prometendo algo que a medição não confirma** — pior que o lexical estar fora do binário, porque ali a ausência era honesta. Com apenas 4 testes em `hybrid.rs`, é também o menos protegido.
status: raw
dod:
  - uma fusão cujo ganho sobre o vetorial puro sobrevive a teste pareado de significância, no mesmo corpus do M123
  - ou, se nenhuma sobreviver: um honest-negative que retire a promessa da superfície pública em vez de mantê-la
  - cobertura de teste de `hybrid.rs` proporcional à superfície exposta

> Registered 2026-08-09 by `/backlog-item` (slug: `hibrido-fusao-significativa`).

## B-006 — Colunar: 43 queries do ClickBench medidas, a suíte completa nunca   [ ]

domain: colunar
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: o M128 mediu 43 queries do ClickBench com md5 byte-idêntico ao heap — correção provada. Mas a suíte completa nunca rodou publicada, e o M184 registrou que o ganho do colunar vive no pushdown, não no seqscan plano. Sem a suíte completa não dá para dizer onde o pilar é competitivo e onde não é, que é precisamente o que o objetivo B-002 vai precisar saber.
status: raw
dod:
  - suíte ClickBench completa executada e publicada em `wiki/benchmarks/`, com as queries que falham ou degradam nomeadas
  - por query, quanto do tempo é pushdown e quanto é seqscan plano — medido, não estimado
  - o hardware e o método de reprodução no artefato

> Registered 2026-08-09 by `/backlog-item` (slug: `colunar-clickbench-completo`).

## B-007 — Grafo: 23 funções expostas e nenhuma medição contra peer algum   [ ]

domain: colunar
repo: theo-db
suggested_mode: review
source: human
evidence: none-yet
why_now: o M184 mediu 23 funções de grafo no binário default e 35 testes — a maior superfície pública depois do vetorial. **Não existe um único artefato comparando o pilar com qualquer outro sistema**, nem um número de latência publicado. Qualquer afirmação sobre o grafo hoje, em qualquer direção, é sem lastro; e ele é superfície que o usuário recebe.
status: raw
dod:
  - latência e throughput medidos em ao menos duas operações de travessia, com dataset e método publicados
  - comparação contra um baseline — SQL recursivo no próprio Postgres serve, e é o que o usuário faria sem nós
  - o veredito é publicado mesmo se desfavorável

> Registered 2026-08-09 by `/backlog-item` (slug: `grafo-sem-baseline`).

## B-008 — Lakehouse: 4 funções expostas, escala e formatos nunca medidos   [ ]

domain: colunar
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: o M184 mediu 4 funções de parquet no default e **zero testes próprios** em `parquet.rs` contra uma nota que exigia "testado" — a nota estava alta, e isso está registrado. Foram adicionados 6 testes em 2026-08-09, que **não rodaram** (bloqueados por B-001). Escala, formatos além de Parquet e comportamento sob arquivo corrompido seguem sem medição.
status: raw
dod:
  - leitura medida em ao menos duas ordens de grandeza de tamanho de arquivo, com o tempo publicado
  - comportamento sob arquivo truncado/corrompido: erro tipado, nunca crash do backend
  - os 6 testes existentes efetivamente executados (depende de B-001)

> Registered 2026-08-09 by `/backlog-item` (slug: `lakehouse-escala-formatos`).

## B-009 — AI surface: `embed.rs` e `rerank.rs` têm 1 teste cada   [ ]

domain: ai-surface
repo: theo-db
suggested_mode: review
source: human
evidence: none-yet
why_now: o M184 contou **1** teste em `embed.rs` e **1** em `rerank.rs` — os dois extremos inferiores do crate inteiro. É a superfície que faz egress HTTP para provedor externo, ou seja, a que mais tem modo de falha que teste unitário pega: timeout, 5xx, resposta malformada, credencial ausente. O M177 mediu a performance desse caminho; a robustez dele não.
status: raw
dod:
  - cada modo de falha do egress (timeout, 5xx, corpo malformado, credencial ausente) tem teste que assere o erro **tipado**, não apenas que lança
  - nenhum segredo aparece em log ou mensagem de erro — verificado por teste
  - o comportamento fail-open/fail-closed de cada função está declarado e coberto

> Registered 2026-08-09 by `/backlog-item` (slug: `ai-surface-robustez-egress`).

## B-010 — Maturidade: zero uso real, e é o gargalo de todos os pilares   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: live-test
source: human
evidence: none-yet
why_now: 109 artefatos de benchmark sintético e nenhuma instalação real. `theo-rag` e `theo-memory` — os produtos de IA do próprio time — declaram `docker compose up -d pgvector`. O âncora de dogfood está `planned`. E o defeito do planner medido em 2026-08-09 prova o custo disso: o índice vetorial era rejeitado em todos os cenários, entregando 182 ms onde havia 6 ms — **nenhum dos 109 benchmarks pegou**, porque todos forçam o caminho que querem medir. Um usuário real teria pego no primeiro dia.
status: raw
dod:
  - `theo-rag` servindo consultas reais sobre TheoDB na infraestrutura que o time opera
  - âncora de dogfood em `running`, com ao menos 3 evidências e 1 história de falha (soft caps da golden rule)
  - ao menos um defeito encontrado por uso, não por benchmark — é a prova de que o dogfood está funcionando

> Registered 2026-08-09 by `/backlog-item` (slug: `dogfood-uso-real`).

Próximo id livre: **`B-011`**. Ids são monotônicos e nunca reusados.
