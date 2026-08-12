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

---

## Priorização — derivada do ADR 0060 (assinado 2026-08-09)

O ADR exige, no seu DoD, que este lote seja repriorizado contra os cinco eixos e que **os itens que não
servem a nenhum eixo sejam mortos, inclusive os que eu mesmo registrei**. Aplicado:

| ordem | item | eixo | por quê |
|---|---|---|---|
| **1** | **B-010** dogfood / uso real | **A5** | **o único eixo com estado `NÃO medido`.** Todos os outros já têm número; este não tem nenhum |
| 2 | B-001 suíte de testes | *(nenhum — habilitador)* | não é eixo, é o que permite provar qualquer eixo sem depender do CI |
| 3 | B-005 fusão híbrida | A2 | o ADR a classifica como **dívida, não eixo**: está exposta prometendo o que o M123 mediu como não-significativo |
| 4 | B-004 lexical em mais corpora | A2 | o eixo já tem número em dois corpora; falta a curva |
| 5 | B-009 robustez do egress | A1 | superfície exposta com 1 teste por arquivo, e é a que faz I/O externo |
| 6 | B-006 ClickBench completo | A2 | o ADR **explicitamente não promete** vencer o ClickBench; medir serve para saber onde não competimos |
| 7 | B-003 vetorial ≥100M | A3 | paridade já medida; a escala amplia, não muda a promessa |
| 8 | B-008 lakehouse escala | A1 | superfície presente e correta; escala é refinamento |
| **—** | **B-007 grafo sem baseline** | **nenhum** | **ver abaixo** |

### B-007 — mantido, com a razão corrigida

O item nasceu como "grafo nunca foi medido contra ninguém", o que é verdade e **não é um eixo**. Sob o ADR
ele deveria morrer. Não morre por um motivo que só ficou visível ao aplicar a régua: **23 funções de grafo
estão expostas no binário default** ([m184](wiki/benchmarks/m184-pilares-superficie-medida-verdict.md)), e o
eixo **A1 promete "um banco só"** — o que inclui não entregar superfície pública cuja qualidade ninguém
conhece. É o mesmo argumento que matou o SymQG no M176.

**A razão do item muda:** não é "queremos ser bons em grafo", é "temos superfície pública não caracterizada".
Se a medição mostrar que ela não serve, a saída correta é removê-la, não otimizá-la.

### Nenhum item foi morto

O ADR previa que alguns morreriam. Nenhum morreu — mas B-007 **quase**, e a régua mudou a sua justificativa.
Registro isso porque "nenhum item morreu" é o resultado que mais merece desconfiança numa repriorização: ou o
lote estava bem escolhido, ou a régua foi aplicada frouxa. Aqui foi o primeiro caso apenas porque cada item
já nascera atado a uma medição nossa (gate G5) — o que é diferente de estar atado a um eixo.

## B-001 — `cargo pgrx test` não roda: o binário de teste morre em `CurrentMemoryContext`   [x]

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
> **Diff feito no mesmo dia, e ele estreitou mais o espaço — sem fechar.**
>
> - **`.cargo/config.toml`:** o `cargo pgrx new` cria um, e nós não temos. **Mas ele só carrega flags para
>   macOS** (`-Wl,-undefined,dynamic_lookup`); no Linux é inerte. **Descartado.**
> - **`crate-type`:** idêntico (`["cdylib"]`). **Descartado.**
> - **Hipótese minha, testada e REFUTADA:** `CurrentMemoryContext` é símbolo de *dado*, e dados não podem ser
>   ligados preguiçosamente — supus que nossa crate o referenciasse e a referência não. Injetei
>   `palloc0`/`pfree` num `#[pg_extern]` do projeto de referência: ele **continua carregando e executando**
>   (`0 passed; 1 failed`, sem `symbol lookup error`). Tocar alocação de memória do PG **não** é o gatilho.
>
> **Bisseção executada — as dependências NÃO são o gatilho.** Primeiro corte grande: as **8** dependências
> diretas (`datafusion`, `arrow`, `tantivy`, `serde_json`, `minreq`, `futures`) injetadas no projeto de
> referência, mais um `#[pg_extern]` que usa três delas para o linker não podar. Resultado: **continua
> carregando e executando** (`0 passed; 1 failed`, sem `symbol lookup error`). Build de 98,6 s.
>
> *(Correção: eu havia dito "~40 dependências" — esse é o número de transitivas. Diretas são 8.)*
>
> **Espaço restante, e agora ele é pequeno:** não é o ambiente, não é o `crate-type`, não é o
> `.cargo/config.toml`, não é alocação do PG, **não são as dependências**. Sobra o que é exclusivamente
> **nosso código**: o `build.rs`, o `extension_sql!` declarativo, o facade `api.rs`
> ([ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)), e o que roda em inicialização estática.
>
> **Os dois candidatos seguintes, medidos — nenhum reproduz.**
>
> - **`build.rs`: eliminado por inspeção, sem gastar um build.** *Ele não existe.* O que eu vinha chamando de
>   `build.rs` era `src/am/build.rs`, um módulo nosso — não um script de build do Cargo. O candidato nasceu de
>   um erro meu de leitura.
> - **`extension_sql!`: não reproduz.** Injetado no projeto de referência no mesmo padrão do nosso `ai_op.rs` /
>   `graph_pgq.rs` (schema + função declarativa, com `name =`). Continua carregando e executando.
>
> **Os dois últimos candidatos caíram juntos — por busca direta, não por bisseção.** `nm` sobre o binário de
> teste e sobre cada `.rlib`:
>
> ```
> CurrentMemoryContext no binário ........ U (indefinido)
> objetos NOSSOS que o referenciam ....... NENHUM
> entra por .............................. libpgrx.rlib, libpgrx_pg_sys.rlib
> ```
>
> **A referência não vem do nosso código.** Vem das próprias crates do pgrx. Nem o facade `api.rs` nem
> inicialização estática são a causa — **nenhum objeto nosso toca o símbolo**.
>
> **Causa-raiz, então:** o projeto de referência usa uma fatia pequena da API do pgrx, e o linker poda os
> caminhos que referenciam `CurrentMemoryContext`. A nossa crate usa fatia larga o bastante para mantê-los. É
> um símbolo de **dado** — não pode ser ligado preguiçosamente, então basta ser alcançável para ser exigido no
> carregamento. Isso explica por que `palloc0`/`pfree` sozinhos não reproduziram: são símbolos de *função*.
>
> **Consequência:** não existe arquivo nosso para consertar. O defeito é estrutural — qualquer extensão pgrx
> grande o bastante encontra isto. A direção de solução é outra e nunca foi tentada: fazer o binário de teste
> resolver os símbolos do PG (linkar contra o binário do postgres, `-rdynamic` / `pg_config --libdir`), ou
> executar os testes de fato dentro de um backend.
>
> **Todos os 9 candidatos levantados foram medidos.** O item permanece aberto porque a *solução* não foi
> implementada — mas deixou de ser uma caça: o mecanismo está identificado e a direção é conhecida.

> **Metade resolvida em 2026-08-09** (commit `0229532`): os **69 testes puros** passaram a executar. A causa
> era dupla e eu tratava como uma só — `--unresolved-symbols=ignore-all` resolve o **link**, e
> `src/pg_test_stubs.rs` (16 símbolos medidos por `ldd -r`, sob `#[cfg(test)]`) resolve o **carregamento**.
>
> **A metade que falta — os 370 `#[pg_test]` — tem sintoma novo e diferente.** Não é mais `symbol lookup
> error`: o harness **trava**. Medido: com a compilação já em cache, `cargo test --lib --features pg_test sq8`
> ficou **17 minutos sem terminar e sem emitir saída**, e o contêiner não tinha processo `postgres` nem
> `initdb` rodando. O harness espera por um servidor de teste que nunca sobe, e **não falha rápido** — o que
> é pior que falhar, porque não deixa diagnóstico.
>
> **Medido em 2026-08-09 — `cargo pgrx start` falha em SILÊNCIO.** Rodado isolado, de dentro da crate, como
> usuário não-root, com `PGRX_HOME` próprio:
>
> ```
> cargo pgrx init --pg18 $(which pg_config)   → OK; data-18/ criado com base, global, pg_commit_ts, pg_hba.conf
> cargo pgrx start pg18                        → SEM SAÍDA, sem erro, exit 0
> pg_isready -h localhost -p 28818             → no response
> psql -p 28818                                → Connection refused
> ```
>
> **O `init` funciona; o `start` reporta sucesso e a porta nunca abre.** É exatamente por isso que o harness
> trava: ele espera por um servidor que foi declarado iniciado. Nenhum arquivo `*.log` foi produzido sob
> `~/.pgrx/` — não há nem o log do servidor para ler.
>
> **Dois candidatos eliminados de passagem:** `initdb` NÃO falha (o data dir está completo), e o `pgrx init`
> apontando para o Postgres do sistema NÃO impede a inicialização.
>
> **A separação foi feita, e o veredito é claro: o PostgreSQL funciona; quem falha é o `cargo pgrx start`.**
> `pg_ctl` invocado diretamente sobre o mesmo `~/.pgrx/data-18`, mesma porta, mesmo usuário não-root:
>
> ```
> pg_ctl -D ~/.pgrx/data-18 -l srv.log -o "-p 28818" start   → "server started"
> LOG:  listening on IPv4 address "0.0.0.0", port 28818
> LOG:  database system is ready to accept connections
> pg_isready -p 28818                                        → accepting connections
> ```
>
> O mesmo diretório de dados que o `cargo pgrx start` declarou iniciado sem abrir porta **sobe em segundos
> pelo `pg_ctl`**. O defeito está na camada do pgrx, não no servidor nem no ambiente.
>
> **Pista concreta no log, não seguida:** o socket Unix vai para `/var/run/postgresql/.s.PGSQL.28818` (default
> do Debian). Se o pgrx procura o socket noutro diretório — o data dir, ou `/tmp` —, ele conclui que o
> servidor não subiu quando ele subiu. É hipótese, não medição.
>
> **Duas saídas, e a segunda não depende de descobrir a causa:**
> 1. achar por que o `pgrx start` falha em silêncio (ler o que ele passa ao `pg_ctl`);
> 2. **contornar** — subir o servidor com `pg_ctl` antes e fazer o harness usar o que já está de pé. Se o
>    `pgrx-tests` aceitar um servidor existente, os 370 `#[pg_test]` destravam sem consertar o pgrx.
>
> **Nota de método que vale para todas estas rodadas:** o projeto de referência **nunca passa** neste
> ambiente — ele sempre termina em `test result: FAILED. 0 passed; 1 failed`, porque não sobe um postgres
> dentro do contêiner. A comparação que sustenta cada eliminação não é "passa vs falha", é **"o binário
> carrega e o harness executa" vs "o binário não carrega" (exit 127)**. Essa distinção se manteve idêntica nas
> quatro rodadas.

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
  - `theo-rag` servindo consultas reais sobre TheoDB no **`app-dev.usetheo.dev`** (ambiente trocado pelo owner em 2026-08-10; era "infraestrutura de produção")
  - âncora de dogfood em `running`, com ao menos 3 evidências e 1 história de falha (soft caps da golden rule)
  - ao menos um defeito encontrado por uso, não por benchmark — é a prova de que o dogfood está funcionando

> Registered 2026-08-09 by `/backlog-item` (slug: `dogfood-uso-real`).
>
> **Progresso 2026-08-10 — 4 evidências, e o caminho até o uso está inteiro.** Publicada
> `ghcr.io/usetheoai/theo-db:latest` + `:0.140.0` (a primeira vez na história do projeto); PR do `theo-rag`
> apontado para ela e verificado `Up (healthy)` com as 24 tabelas do schema real.
>
> **DoD 3 satisfeito com folga — TRÊS defeitos achados por uso**, nenhum pego por 109 benchmarks:
> a inversão de custo do planner, o mount do PG 18 que fazia o contêiner entrar em loop, e o workflow de
> publicação apontando para um org inexistente — este último provando que o PR **nunca poderia ter
> funcionado** para quem o mergeasse (a tag `usetheodev/theo-db:0.139.0` não existe).
>
> **DoD 1 e 2 seguem abertos** e dependem do merge + deploy: `running` exige uso sustentado em infraestrutura
> real, e nenhuma das 4 evidências é uso — todas são verificação.

## B-011 — O vector-join do HNSW perde exatamente um elemento   [x]

domain: vetorial
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: `wiki/benchmarks/m187-vector-join-recall-defeito.md` — `am::hnsw_page` vector-join devolve 199/200 e 59/60 onde o contrato exige igualdade com a busca exata. Os dois testes já existiam em `am/hnsw_page.rs` e falham na primeira execução da suíte (2026-08-10).
why_now: a suíte destravou pelo B-001 e executou pela primeira vez. **109 artefatos de benchmark não pegaram este off-by-one**, porque benchmark mede o caminho que se escolhe medir. O caso `k ≥ |b|` é o mais grave: pedir o conjunto inteiro tem de devolver o conjunto inteiro — não há trade-off de recall a fazer nesse regime.
status: raw
dod:
  - `pg_vector_join_recall_matches_exact_within_tol` e `pg_vector_join_threshold_correct` passam
  - a causa é nomeada em `hnsw_page.rs`, não contornada por afrouxar a tolerância do teste
  - verificado se é um bug ou dois — os dois erram por exatamente um elemento, o que sugere causa comum

> Registered 2026-08-10 by `/backlog-item` (slug: `hnsw-vector-join-off-by-one`).
>
> **FECHADO 2026-08-10 — e o produto não tinha o defeito.** A varredura com os dados exatos da semeadura
> mostrou 55 vetores distintos em 60 linhas (período 55 do padrão), e `ef_search` limitando o **beam**, não o
> resultado. Com dados aleatórios, `ef=60` devolve 60; com os do teste, precisa de `ef≈100`. A premissa
> "`k ≥ |b|` devolve tudo trivialmente" é falsa para HNSW e era a base das duas asserções. Beam elevado a 200
> nos dois testes, **alvo intacto** (igualdade exata com o oráculo seqscan). Verificado: `4 passed; 0 failed`.

> **Análise de código 2026-08-10 — o teste e o laço, lidos.** O teste (`src/am/hnsw_page/tests.rs`) faz:
> `SET theodb_hnsw.ef_search = 60` sobre uma tabela `vrb` de **exatamente 60 linhas**, e então
> `SELECT ... ORDER BY emb <=> probe LIMIT 70` com `enable_seqscan=off`. Espera 60, recebe 59.
>
> O laço de busca (`src/ann/scan_core.rs:110-165`) limita o resultado a `ef`:
> ```rust
> if nd < worst || result.len() < ef { ...; if result.len() > ef { result.pop(); } }
> ```
> Com `ef == |b| == 60`, o heap comporta 60 e o portão de entrada admite enquanto `result.len() < ef`.
> **Pela leitura, deveria chegar a 60.**
>
> **Duas explicações concorrentes, e a distinção importa:**
> 1. **off-by-one real** no limite — um slot consumido pelo ponto de entrada, ou `<` onde caberia `<=`;
> 2. **propriedade do HNSW** — um nó inalcançável no grafo dentro do beam, que é recall normal e não defeito.
>    Nesse caso o **teste** é que está errado ao exigir 1.0 com `ef == |b|`.
>
> A distinção é decisiva para o conserto: (1) conserta-se o código, (2) conserta-se o teste — e afrouxar o
> teste sem saber qual é o caso seria exatamente o bypass que o DoD proíbe.
>
> **Experimento que separa os dois, ainda não executado:** varrer `ef_search` em 40/59/60/61/80/120 sobre a
> mesma tabela de 60 linhas. Se voltar 60 a partir de 61, é (1) — o limite erra por um. Se ficar em 59 mesmo
> com ef=120, é (2) — há um nó que o grafo não alcança, e aí a pergunta vira por que ele ficou isolado.
> **Descartado por medição:** a hipótese de que a correção do planner (m175) o causara. Revertendo apenas a
> correção TOAST em `am/mod.rs`, os mesmos dois testes falham. O defeito é anterior e independente.

## B-012 — As outras 18 falhas da suíte seguem sem causa capturada   [x]

domain: engine-pgrx
repo: theo-db
suggested_mode: review
source: discover-review
evidence: 20 falhas na primeira execução da suíte; **2 causas capturadas** (B-011), 18 sem mensagem — o `tail` do script de execução cortou os pânicos.
why_now: 6 das 18 são do `lexical::engine`, o pilar promovido a binário default em 2026-08-09. Promover superfície pública com teste vermelho é o defeito que o M184 mediu no SymQG e o M176 removeu.
status: raw
dod:
  - as 18 causas capturadas e classificadas em defeito de produto vs limitação de ambiente
  - as 6 do lexical resolvidas ou o pilar sai do default até que estejam
  - *(o CI saiu deste DoD e virou B-013 — é infraestrutura, não diagnóstico)*

> Registered 2026-08-10 by `/backlog-item` (slug: `suite-18-falhas-sem-causa`).
>
> **FECHADO 2026-08-10** — os três DoD cumpridos:
> 1. **Causas capturadas e classificadas** — `wiki/benchmarks/m188-suite-18-falhas-classificadas.md`, uma
>    linha por teste com a mensagem verbatim do servidor.
> 2. **As 6 do lexical resolvidas** — faltava `#[pgrx::pg_schema]`; o pilar segue no default, agora verde.
> 3. **Suíte no CI** — B-013, com baseline já baixado de 20 para 10.
>
> **Resultado: 419/20 → 429/10, sem uma linha de mudança no produto.** As 10 que caíram eram todas defeito
> de registro ou classificação de teste. **Metade das falhas de uma suíte que passou meses sem rodar era a
> própria suíte apodrecendo** — um teste que não executa não protege nada e ainda acumula defeitos próprios.
>
> **As 10 abertas herdaram itens novos** (B-015, B-016), porque classificá-las mostrou que são duas famílias
> distintas e não um resto homogêneo.

## B-015 — Cinco testes falham com contador em zero: instrumentação ou o chunk-skip não poda?   [ ]

domain: colunar
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: `wiki/benchmarks/m188-suite-18-falhas-classificadas.md` — `explain_scan shows real pages_read (got 0)`, `the table must span >= 2 chunk groups (scanned 0)`, `with the GUC on the selective predicate must prune (got 0)`. Cinco testes, três módulos, **a mesma assinatura**.
why_now: cinco testes de módulos diferentes falhando com contador em zero sugere causa comum. A distinção decide tudo: se for instrumentação, conserta-se o teste; **se for produto, o chunk-skip não está podando** — um defeito de performance real e silencioso no pilar colunar, exatamente a classe que o m175 revelou no planner.
status: triaged
measured_evidence: `wiki/benchmarks/b015-cinco-contadores-em-zero-duas-causas.md` — medido 2026-08-11. **Não havia causa comum**, e a hipótese de paralelismo do item está refutada. Família A (3 testes colunares) = fixture: o `pg_test` roda em transação e o colunar nunca materializa stripe (0/0 na transação, 4/5 pós-commit, 30/31 com `maintenance_work_mem` baixo) — o produto sempre podou. Família B (2 testes de autotune) = defeito real: o caminho *resume* do M118, default de todo índice V1, nunca reportou as métricas (kill-switch `off` → `pages=112 cand=38`; `on` → `0/0`).
dod:
  - determinado por medição se os contadores não incrementam ou se o caminho de varredura não é tomado
  - se for produto: o chunk-skip poda e os cinco passam; se for instrumentação: os testes medem o que existe
  - **não** afrouxar a asserção antes de saber qual é o caso

> Registered 2026-08-10 by `/backlog-item` (slug: `contadores-em-zero`).

> **Medido em 2026-08-11 — os cinco NÃO têm causa comum. São dois defeitos diferentes, e a hipótese
> de paralelismo está REFUTADA nos dois.** Medido contra `ghcr.io/usetheoai/theo-db:0.140.0` (a imagem
> é de 2026-07-24, **posterior** ao commit `be00b86` que introduziu a instrumentação — o binário tem o
> código que se está medindo).
>
> **CAUSA DA FAMÍLIA A, provada em 2026-08-11 — é o FIXTURE, e a correção não afrouxa nada.** O
> `#[pg_test]` roda cada teste dentro de **uma transação** (revertida ao fim). O escritor colunar segura
> as linhas no *pending set* e só materializa um stripe durável quando o buffer excede
> `maintenance_work_mem` ou no pre-commit (M104). Sob os 64 MB default, 50 000 linhas estreitas nunca
> alcançam esse limite e o commit nunca chega — então **não existe chunk-group algum** para o zone-map
> podar. Medido na mesma sessão, mesmas linhas:
>
> | condição | `skipped` / `scanned` |
> |---|---|
> | dentro da transação (o que o `pg_test` faz) | **0 / 0** |
> | após `COMMIT` | 4 / 5 |
> | dentro da transação, com `maintenance_work_mem = '64kB'` | **30 / 31** |
>
> O zone-map estava podando o tempo todo; o fixture é que nunca produzia o que ele poda. **Correção
> aplicada em `seed_clustered`** (`columnar_project.rs`): baixar `maintenance_work_mem` para que o flush
> incremental que o produto **já implementa** de fato ocorra no teste. O DoD deste item proíbe
> explicitamente a outra rota — afrouxar a asserção —, que teria escondido uma feature funcionando
> atrás de um teste verde.
>
> **Família A — colunar (testes 3, 4, 5): o produto está CORRETO. O chunk-skip PODA.** Replicado o
> `seed_clustered(50_000)` em SQL puro: `SELECT a FROM t_col WHERE a = 25000` devolve
> **`skipped=4 scanned=5`** — quatro dos cinco chunk-groups podados. Idêntico em três caminhos: statement
> de topo, **SPI aninhado** (função PL/pgSQL, o mesmo mecanismo do `#[pg_test]`) e conexão nova com os
> GUCs default. E com `max_parallel_workers_per_gather` em 0 **e** em 4 o resultado é o mesmo, porque o
> `Custom Scan (theodb_columnar_project)` não paraleliza. A hipótese registrada aqui em 2026-08-10 —
> `thread_local` cego a workers paralelos — **não se sustenta**: além de o plano não paralelizar, o
> próprio `seed_clustered` já executa `SET max_parallel_workers_per_gather = 0`
> (`columnar_project.rs:840`), de modo que os testes nunca correram sob paralelismo.
>
> **Família B — autotune (testes 1, 2): DEFEITO REAL, reproduzido.** `theodb.explain_scan` devolve
> `pages_read=0` e `candidates_seen=0` **enquanto o plano usa o índice e a consulta devolve as linhas
> certas**:
>
> | escala | plano observado | `explain_scan` |
> |---|---|---|
> | 2 000 × `vector(64)` | `Index Scan using esc2_e_idx` | `pages=0 cand=0 lat_us=1105 results=5` |
> | 20 000 × `vector(64)` | `Index Scan using esc3_e_idx` (escolhido **sem** nenhum `SET`) | `pages=0 cand=0 lat_us=1697 results=5` |
> | idem, `max_parallel_workers_per_gather=0` | idem | `pages=0 cand=0 lat_us=2595` |
>
> `results=5` e `lat_us` não-trivial provam que a consulta executou; `pages=0` prova que
> `bump_scan_pages`/`bump_scan_candidates` (`hnsw_page/search.rs:337-338`) não foram alcançados. Não há
> caminho de saída entre o início de `traverse` e os dois bumps exceto índice vazio
> (`search.rs:181`) e os `Err` de codebook corrompido, então a leitura mais provável é que
> `traverse` não seja o caminho executado por este scan — hipótese ainda **não** confirmada.
>
> **Achado colateral com impacto próprio:** `explain_scan` só reconhece índices cujo AM é
> `theodb_hnsw`. Um índice criado pela sintaxe pgvector (`USING hnsw`, o alias do shim — `pg_am` OID
> distinto: `hnsw=19173` vs `theodb_hnsw=16568`) devolve
> `(no theodb_hnsw index on this table)`. **Todo índice que o `theo-rag` cria é invisível para o
> diagnóstico e para o autotune.** Segundo achado: um índice criado com o opclass **default** do AM
> (`theodb_hnsw_l2_ops`) nunca serve o operador `<=>`, e o plano cai em `Sort` + `Seq Scan` — o default
> do AM é L2 e a superfície de diagnóstico assume cosine.

> **Hipótese de causa, 2026-08-10 — lida no código, não medida ainda.** Os contadores são `thread_local!`
> (`src/am/columnar.rs:50-62`), e o próprio comentário admite ser *"best-effort under nested scans"*. O teste
> semeia **50 000 linhas** — escala em que o PostgreSQL escolhe varredura **paralela**, e a poda acontece nos
> *workers*, cada um com seu próprio `thread_local` (processos distintos, na verdade). **O líder, que responde
> ao `SELECT theodb_columnar_chunks_skipped()`, lê zero.**
>
> Isso explicaria `scanned 0` exatamente, e é a distinção que o DoD exige: se for isto, o chunk-skip **está
> podando** e a instrumentação é que é cega ao paralelismo — conserta-se a instrumentação (ou o teste), não o
> colunar.
>
> **Experimento decisivo, uma linha:** `SET max_parallel_workers_per_gather = 0` antes da consulta do teste.
> Se os contadores passarem a reportar > 0, a hipótese está confirmada e o pilar colunar não tem defeito. Se
> continuarem em zero mesmo em serial, aí sim o chunk-skip não está podando — e é defeito de performance real.

## B-016 — Os testes de egress esbarram na guarda SSRF do próprio produto   [ ]

domain: ai-surface
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: `pg_embed_unreachable_endpoint_fails_typed` e o par de `rerank` recebem `theodb.embed: refusing to call 127.0.0.1 — it resolves to a blocked internal address`. **Ampliado em 2026-08-11 — são TRÊS testes, não dois.** A suíte completa (`434 passed; 6 failed`) mostra que `http::m104_breaker_success_closes` tem a mesma causa por outro sintoma: o log traz cinco `theodb egress guard: bt denied host 127.0.0.1 -> blocked address 127.0.0.1` seguidos de `ERROR: open after K failures` (`http.rs:320`). O teste quer provar que **um sucesso em HalfOpen FECHA o disjuntor**, e nunca há sucesso — a guarda recusa loopback antes de qualquer conexão, então o disjuntor só acumula falhas e abre. É o mesmo produto-certo-teste-desatualizado, agora atingindo também a máquina de estados do circuit breaker.
why_now: os testes querem provar erro **tipado** para endpoint inalcançável e recebem um erro tipado **diferente** — a guarda SSRF recusa loopback antes de conectar. **O produto está certo e o teste está desatualizado:** a guarda é mais nova que ele.
status: triaged
resolvido: 2026-08-11 — os TRÊS corrigidos, sem tocar a guarda. `embed` e `rerank` passaram a apontar para `invalid.invalid` (TLD reservado RFC 2606, não resolve em lugar nenhum), com a mensagem MEDIDA no binário shipado, não adivinhada. O do disjuntor (`m104_breaker_success_closes`) passou a registrar as K falhas direto na máquina de estados via `breaker_record`, ficando hermético — coerente com o comentário que ele já carregava ("assert the state-machine directly") e com a segunda metade, que sempre usou essa porta. **Medição que decidiu o endereço:** um IP TEST-NET (`192.0.2.1`) passa a guarda e produz `endpoint call failed: the timeout of the request was reached`, mas leva **90,6 s** com os 2 retries — inaceitável numa suíte; o DNS falha em milissegundos.
dod:
  - os testes usam um endereço externo inalcançável, provando o erro que pretendem provar
  - a guarda SSRF permanece intacta — afrouxá-la para fazer teste passar seria abrir um SSRF
  - um teste separado cobre a própria guarda, que hoje só é exercitada por acidente
  - **os TRÊS testes passam** — `embed`, `rerank` e `http::m104_breaker_success_closes`; o do disjuntor precisa de um caminho que produza SUCESSO em HalfOpen, o que a guarda impede por loopback
  - a máquina de estados do disjuntor ganha cobertura que não dependa de rede alguma (o comentário do próprio teste admite que ele não consegue ser hermético: *"We can't hit a live 4xx hermetically"*)

> Registered 2026-08-10 by `/backlog-item` (slug: `egress-guarda-ssrf`).

## B-013 — A suíte não roda no CI, então a próxima regressão espera meses   [x]

domain: engine-pgrx
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: a suíte destravou em 2026-08-10 (B-001) e revelou **20 falhas na primeira execução**, das quais uma (B-011) é um defeito de recall que 109 artefatos de benchmark não pegaram. Nada garante que a próxima regressão apareça antes de meses: a execução hoje depende de alguém lembrar de rodá-la à mão, com uma receita de cinco peças que vive num runbook. Medido: a suíte inteira leva **480 s** — o argumento de custo não existe.
status: raw
dod:
  - a suíte roda em cada push para `workspace`, com a receita do runbook, e o resultado é visível sem abrir log
  - o número de falhas é um gate declarado (baseline aceito hoje = 20) e **subir esse número reprova**
  - o tempo de execução é publicado, para que a degradação do próprio CI seja visível

> Registered 2026-08-10 by `/backlog-item` (slug: `suite-no-ci`).
> Saiu do DoD do B-012 por ser trabalho diferente: B-012 é diagnosticar 18 falhas, este é impedir a próxima.

## B-014 — `bm25_search` aceita um termo por chamada; consulta de usuário tem vários   [ ]

domain: lexical
repo: theo-db
suggested_mode: evolve
source: human
evidence: none-yet
why_now: descoberto ao medir a qualidade do pilar contra o BEIR (`wiki/benchmarks/m186-lexical-ndcg-scifact-verdict.md`). Para avaliar uma consulta multi-termo eu tive de **somar os scores por termo do lado de fora** — aproximação grosseira que o BM25 real não faz, porque ele normaliza uma vez sobre a consulta inteira. O pilar entrou no binário default em 2026-08-09 expondo `bm25_search(index, termo, k)`: **nenhuma consulta real de usuário é um termo só.**
status: raw
dod:
  - `bm25_search` aceita uma consulta multi-termo e a pontua numa passagem, sem agregação externa
  - o nDCG@10 é re-medido nos dois corpora do m186 com a nova assinatura — a expectativa é SUBIR, já que a agregação atual subestima
  - a assinatura antiga permanece ou é migrada por script de upgrade; quebrar quem já usa não é aceitável

> Registered 2026-08-10 by `/backlog-item` (slug: `bm25-multi-termo`).

## B-017 — `running` exige tempo, e nenhuma ação instantânea o produz   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: live-test
source: human
evidence: `.claude/knowledge-base/dogfood/evidence/` — 4 evidências em 2026-08-10, todas `partial`/`pass` de **verificação**, nenhuma de uso.
why_now: o B-010 ficou com dois DoD abertos, e a razão é estrutural, não de esforço. A golden rule define `running` como *"ativamente usado pelo time em infraestrutura real"*, e exige ≥ 3 evidências mais **uma história de falha**. Uma falha em operação **emerge de operação** — não existe ação, minha ou de ninguém, que a produza num dia. Registrar isto como item próprio evita que o B-010 fique aberto para sempre parecendo trabalho não feito, quando o que falta é calendário.
status: raw
dod:
  - `theo-rag` mergeado e rodando sobre TheoDB em dev por ao menos duas semanas
  - ≥ 3 evidências de **operação** (não de verificação), com operador humano nomeado
  - ≥ 1 história de falha real — a única que não pode ser fabricada
  - só então o âncora vai a `running`

> Registered 2026-08-10 by `/backlog-item` (slug: `running-exige-tempo`).
> **Depende de:** merge de `usetheoai/theo-rag#206` e deploy. O caminho técnico foi verificado ponta a ponta
> em 2026-08-10 — imagem publicada, compose `Up (healthy)`, schema real aplicado, 197 testes passando.
>
> **Medido em 2026-08-10 — por que o merge não é meu para fazer, com fato em vez de princípio.** O PR
> `usetheoai/theo-rag#206` é `workspace → develop`: **550 arquivos, +2.973 / −83.022, 13 commits**, dos quais
> apenas 2 são meus. Não é a mudança de compose — é a **promoção de branch inteira** do `theo-rag`, e a troca
> do banco é uma linha dentro dela.
>
> Eu vinha descrevendo o risco como "troca o banco de um produto que serve usuários". A descrição correta é
> pior: mergear integraria 83 mil linhas removidas e o trabalho acumulado de outras pessoas, sem a revisão
> delas, num repositório que não é este. **O `gh pr diff` nem consegue exibir o diff** (HTTP 406, limite de
> 300 arquivos).
>
> **Feito em 2026-08-10: [`theo-rag#211`](https://github.com/usetheoai/theo-rag/pull/211)** — a mesma mudança
> em **um arquivo e 23 linhas**, saindo de `develop`, com as duas correções que só apareceram ao rodar de
> verdade (mount do PG 18 e a tag da imagem que não existia). O #206 foi apontado para ele.
>
> **Abrir o PR é meu; mergear não.** A decisão de trocar o banco de um produto que serve usuários é de quem o
> opera, e agora ela está diante deles como uma mudança que **cabe numa tela** em vez de 550 arquivos. Era
> disto que o item precisava, e é o último passo que existia do meu lado.

> **2026-08-10 — o gate mudou de ambiente, por decisão do owner.** `running` deixa de exigir produção e passa
> a exigir o `theo-rag` rodando no **`app-dev.usetheo.dev`** sobre o TheoDB, servindo consultas. Registrado na
> `dogfood-golden-rule.md § 3`.
>
> **Medido antes de aceitar o novo alvo:** o `app-dev` responde 200 em 0,5 s, mas **devolve a SPA para
> qualquer rota** — `/api/rag/health` retornou HTML, e uma rota inventada também deu 200. **Não há evidência
> de que o `theo-rag` esteja implantado lá**, nem sobre qual banco. O gate novo aponta para um ambiente cujo
> estado ainda não foi verificado; verificá-lo é a primeira coisa que ele exige.
>
> **2026-08-10 — o `theo-rag` adotou o TheoDB em `main`.** O owner mergeou o
> [#211](https://github.com/usetheoai/theo-rag/pull/211) e autorizou o merge dos demais PRs abertos; os 7
> foram analisados e mergeados. `origin/main` do `theo-rag` agora declara
> `image: ghcr.io/usetheoai/theo-db:0.140.0` com o mount corrigido do PG 18.
>
> **DoD 1 avança de "verificado localmente" para "adotado no repositório".** O que separa isto de `running`
> continua sendo o mesmo e não encolheu: **o produto rodando com carga real ao longo do tempo**, produzindo
> uma história de falha em operação. Adoção em `main` é uma declaração; uso é um fato — e só o segundo move o
> hard cap 2 da golden rule.
>
> **Um conflito que eu causei, registrado:** mergear os PRs do dependabot em `main` antes do release deixou o
> [#197](https://github.com/usetheoai/theo-rag/pull/197) `CONFLICTING` em três workflows. Resolvido por um PR
> de sincronização ([#212](https://github.com/usetheoai/theo-rag/pull/212)), honrando a deleção deliberada de
> `build-publish.yml` que o `develop` havia feito. A ordem inversa — release primeiro, bumps depois — teria
> evitado o conflito inteiro.

## B-018 — O planner não alcança o HNSW no caminho de JUNÇÃO, mesmo com `enable_seqscan = off`   [ ]

domain: vetorial
repo: theo-db
suggested_mode: bug
source: discover-live-test
evidence: suíte de integração do `theo-rag` contra `ghcr.io/usetheoai/theo-db:0.140.0` (2026-08-10): `o planner deveria alcançar o índice HNSW sob enable_seqscan = off`. Plano escolhido: `Limit → Sort → Nested Loop → Index Scan` — **há um `Sort` acima**, então o índice não está servindo a ordenação.
why_now: a correção do planner de hoje ([m175](wiki/benchmarks/m175-planner-cost-inversion-verdict.md)) resolveu a busca simples e **não cobre o caminho de junção**, que é o que o `theo-rag` usa de verdade. Encontrado pela suíte do produto, não por benchmark — o quarto defeito do dia pelo mesmo mecanismo.
status: raw
dod:
  - o teste `ensureHnswIndex` do `theo-rag` passa sem `Sort` acima do `Index Scan`
  - determinado se a causa é o mesmo modelo de custo (m175) num caminho não coberto, ou outra
  - regressão coberta por teste nosso, não só pelo do `theo-rag`

> Registered 2026-08-10 by `/backlog-item` (slug: `planner-hnsw-no-join`).

> **Medido em 2026-08-11 — NÃO REPRODUZIU em seis cenários. O item continua aberto, e o que muda é
> saber onde ele não está.** Reproduzida a query exata do `theo-rag` (`vector-retriever.integration.test.ts:386`
> — `embeddings ⋈ chunks ⋈ documents`, `ORDER BY e.vector <=> $1`, `LIMIT`), contra
> `ghcr.io/usetheoai/theo-db:0.140.0`, sob `SET LOCAL enable_seqscan = off`. Em **todos** os seis o plano
> foi o correto — `Index Scan using embeddings_vector_hnsw ... Order By: (vector <=> ...)`, **sem `Sort`
> acima**:
>
> | # | cenário | plano |
> |---|---|---|
> | 1 | literal via sub-select (InitPlan) | HNSW serve a ordenação |
> | 2 | parâmetro `$1` via `PREPARE`, execuções 1–5 (custom plan) | idem |
> | 3 | parâmetro `$1`, execuções 6–7 (generic plan) | idem |
> | 4 | pós-`TRUNCATE`, sem `ANALYZE` | idem |
> | 5 | índice criado **antes** do seed (tabela vazia) + 1000 linhas incrementais + `ANALYZE` | idem |
> | 6 | idem sem `ANALYZE` (`reltuples=0` no índice, `-1` nas três tabelas) | idem |
>
> Isto **não** absolve o produto: o próprio teste do `theo-rag` declara a falha como **1 em 11
> execuções** (`vector-retriever.integration.test.ts:428` — *"a falha não reproduz sob demanda"*), e seis
> tentativas determinísticas não derrubam um evento intermitente. O que a medição estabelece é que as
> hipóteses baratas — parâmetro vs literal, generic plan, estatística ausente, ordem de criação do
> índice — **não são o gatilho**.
>
> **Próxima medição, e ela é cara por natureza:** rodar a suíte do `theo-rag` em laço até a ocorrência,
> capturando o plano (o teste já o imprime na mensagem de falha desde o #167). Um evento de 1-em-11 exige
> repetição, não outro cenário inventado. Enquanto isso o item fica `raw`, com o espaço de busca
> reduzido — que é o resultado honesto desta rodada.

## B-019 — `CREATE INDEX` de HNSW não é idempotente: estoura em vez de ser no-op   [ ]

domain: vetorial
repo: theo-db
suggested_mode: bug
source: discover-live-test
evidence: `error: duplicate key value violates unique constraint "pg_class_relname_nsp_index"` no `ensureHnswIndex_is_idempotent` do `theo-rag`. **Medido em 2026-08-11:** reproduz só sob concorrência, e reproduz IDÊNTICO num btree nativo do PostgreSQL (controle sem uma linha de código nosso).
why_now: o `theo-rag` chama `ensureHnswIndex` no caminho de inicialização, e ele precisa ser seguro para reexecução — é o padrão de qualquer migração. Estourar em vez de ser no-op quebra reinício de serviço.
status: killed
kill_reason: não é defeito do TheoDB. Serial, `CREATE INDEX IF NOT EXISTS` sobre HNSW é no-op correto (`NOTICE ... skipping`) e sem `IF NOT EXISTS` dá o erro tipado `42P07` — nunca viola o catálogo. A falha só aparece com duas conexões concorrentes, e o **controle com btree nativo do PostgreSQL** (3M linhas, build 21,7 s, zero código nosso) falha com a mensagem idêntica: `IF NOT EXISTS` não é atômico no engine, e dois `CREATE INDEX` tomam `ShareLock`, que é compatível consigo mesmo. O que é nosso é apenas a LARGURA da janela — ver `B-020`. Ação real: serializar `ensureHnswIndex` no consumidor (`pg_advisory_lock`); 9 arquivos de teste do `theo-rag` o chamam e o vitest paraleliza arquivos contra um banco só.
dod:
  - recriar um índice HNSW existente é no-op ou erro tipado claro, nunca violação de constraint do catálogo
  - `ensureHnswIndex_is_idempotent` e `ensureHnswIndex_creates_missing_index` passam
  - verificado se `CREATE INDEX IF NOT EXISTS` se comporta corretamente no nosso AM

> Registered 2026-08-10 by `/backlog-item` (slug: `hnsw-create-index-idempotente`).

> **Medido em 2026-08-11 — o defeito NÃO é nosso, e o controle é o que prova.** Reproduzido contra
> `ghcr.io/usetheoai/theo-db:0.140.0`. Serial, o caminho é **correto**: `CREATE INDEX IF NOT EXISTS`
> repetido emite `NOTICE: relation already exists, skipping` e é no-op; sem `IF NOT EXISTS` emite o
> erro tipado `42P07`. **Nunca** viola o catálogo. O erro do `theo-rag` só reproduz com **duas conexões
> concorrentes** — e aí reproduz literalmente:
> `Key (relname, relnamespace)=(embeddings_vector_hnsw, 2200) already exists`.
>
> **Controle decisivo:** o mesmo `CREATE INDEX IF NOT EXISTS` concorrente sobre um **btree nativo do
> PostgreSQL** (3M linhas, build de 21,7 s — zero código nosso) falha com a **mensagem idêntica**. O
> mecanismo é do PostgreSQL upstream: `IF NOT EXISTS` não é atômico — a checagem de existência e a
> criação não são cobertas por lock exclusivo, e dois `CREATE INDEX` tomam `ShareLock`, que é compatível
> consigo mesmo. Nada no nosso AM participa disso.
>
> O que é nosso é a **largura da janela**: o build HNSW leva segundos (ver `B-020`), o que transforma
> uma corrida teórica em falha rotineira. E o gatilho no consumidor está medido: **9 arquivos de teste
> do `theo-rag` chamam `ensureHnswIndex`**, e o vitest roda arquivos em paralelo contra um banco só.
>
> **Veredito: `ITEM_KILLED` como defeito do TheoDB** (matar um item medido é resultado de sucesso do
> `/discover`, não falha). A ação real é do lado do consumidor — serializar o `ensureHnswIndex` com
> `pg_advisory_lock` — mais `B-020` do nosso lado, que encolhe a janela. Corrigir o nosso AM para
> "resolver" um comportamento do engine seria workaround sobre causa alheia.

## B-020 — `CREATE INDEX` de HNSW é 93× mais lento que inserir as mesmas linhas   [ ]

domain: vetorial
repo: theo-db
suggested_mode: evolve
source: discover-bug
evidence: medido em 2026-08-11 contra `ghcr.io/usetheoai/theo-db:0.140.0`, mesmas 1000 linhas `vector(1536)`, mesma sessão, mesmo binário — **build em lote (`CREATE INDEX` sobre a tabela cheia): 60 643 ms**; **incremental (índice vazio + `INSERT` das mesmas 1000 linhas): 647 ms**. Recall verificado íntegro nos dois caminhos (top-5 do índice ≡ top-5 exato do seqscan, interseção 5/5), então não é o índice incremental que está pulando trabalho.
why_now: a assimetria é o **inverso** do esperado — um build em lote enxerga todos os vetores de uma vez e deveria ganhar do caminho um-a-um, que é o que a doc do pgvector recomenda justamente por isso. O custo é sentido por uso real: o teste do planner do `theo-rag` gastava 29 s de um `testTimeout` de 30 s (97% do orçamento) e o próprio comentário do teste registra a reordenação feita para contorná-lo; nesta sessão o mesmo build estourou um timeout de 2 min. É também o que alarga a janela de corrida do `B-019`.
status: killed
kill_reason: **o número que abriu este item era MEU ERRO DE MEDIÇÃO, e as duas evidências dele caem.**

(1) **Os 60 s não reproduzem.** Medido em 2026-08-12 num host isolado, mesmo binário, mesma forma: `CREATE INDEX` HNSW sobre **1000 × vector(1536)** leva **3.524 ms**, não 60.643 ms — **17× inflado**. A medição original foi feita com outros contêineres disputando CPU, exatamente a contaminação que já havia falseado o `B-023`. É a segunda ocorrência do mesmo erro meu na mesma sessão.

(2) **A comparação de 93× era entre coisas diferentes.** O caminho "incremental" não constrói o mesmo objeto: medido, o índice resultante tem **2576 kB** contra **1504 kB** do build em lote — 71% maior, estrutura distinta. Comparar construir um grafo com empilhar num buffer não mede lentidão de build.

**O que a medição limpa mostra, e é o oposto do item:** o build é ~**linear em N e em dim**, sem patologia —

| N | dim | tempo |
|---|---|---|
| 2000 | 128 | 1.163 ms |
| 4000 | 128 | 2.526 ms (2,17× ao dobrar N) |
| 2000 | 512 | 4.393 ms (3,78× ao quadruplicar dim) |
| 2000 | 1536 | 18.097 ms (1,3× acima da extrapolação) |
| 1000 | 1536 | **3.524 ms** |

**O que NÃO foi explicado, e por isso não vira "nada a ver aqui":** o teste do planner do `theo-rag` gastava 29 s de um `testTimeout` de 30 s, e o comentário dele atribui isso ao seed. Com o build custando 3,5 s, a causa daquele tempo está em outro lugar — provavelmente no INSERT linha-a-linha via driver, não no `CREATE INDEX`. Se alguém quiser perseguir, o item novo deve nascer da medição do `theo-rag`, não desta.
dod:
  - ~~determinado por profiling qual etapa do `ambuild` domina os 60 s~~ **não se aplica: os 60 s não existem**

> Registered 2026-08-11 by `/discover --mode bug` (slug: `hnsw-build-em-lote-lento`), como achado
> colateral da medição do `B-019`.

## B-021 — O diagnóstico não enxerga índice criado pela sintaxe pgvector, e o opclass default não serve `<=>`   [ ]

domain: vetorial
repo: theo-db
suggested_mode: bug
source: discover-bug
evidence: medido em 2026-08-11 contra `ghcr.io/usetheoai/theo-db:0.140.0`. (a) `theodb.explain_scan` sobre uma tabela cujo índice foi criado com `USING hnsw` (o alias do shim, `sql/vector--0.6.0.sql:49`) devolve `(no theodb_hnsw index on this table)` — o alias é uma segunda entrada em `pg_am` (OID `hnsw=19173` vs `theodb_hnsw=16568`) e a resolução casa pelo nome do AM. (b) um índice criado com o opclass **default** do AM (`theodb_hnsw_l2_ops`, `opcdefault=t`) não serve o operador `<=>`: o plano medido cai em `Limit → Sort → Seq Scan`, enquanto `theodb_hnsw_cosine_ops` produz `Index Scan ... Order By`.
why_now: os dois se somam contra o consumidor real. **Todo** índice que o `theo-rag` cria usa `USING hnsw` — portanto é invisível ao `explain_scan` e ao autotune, justamente nas consultas que o dogfood exercita. E `scan_stats` hardcoda `<=>` (`autotune.rs:210`), então quem cria o índice sem nomear o opclass (aceitando o default L2) nunca é medido e não recebe aviso — o diagnóstico devolve zeros silenciosos em vez de dizer "este índice não responde a este operador".
status: raw
status: triaged
resolvido: 2026-08-12 — as TRÊS partes do DoD, mais o script de upgrade que elas exigiam.
dod:
  - ~~`explain_scan` resolve índices pelo **handler**, não pelo nome do AM~~ **FEITO** — join por `pg_proc` (não `'…'::regproc`, que dependeria do `search_path`). Medido: `hnsw` OID 17174 e `theodb_hnsw` OID 16568 compartilham `theodb_hnsw_amhandler`, então resolver por handler cobre as duas sintaxes e qualquer alias futuro.
  - ~~operador incompatível com o opclass produz erro tipado, nunca zero silencioso~~ **FEITO** — `scan_stats` verifica em `pg_amop` se algum índice da tabela responde `<=>` e recusa com erro que nomeia o opclass a usar. **Erro e não aviso**: os números seguintes viriam de um seqscan, e reportá-los sob o nome `explain_scan` seria medir uma coisa e rotular outra.
  - ~~regressão coberta por teste que cria o índice pelas DUAS sintaxes~~ **FEITO** — dois testes. Um cria um AM com nome diferente e o MESMO handler (a forma do shim, sem acoplar o teste ao empacotamento da extensão `vector`); o outro exige que um índice `l2_ops` seja RECUSADO. Os testes que já existiam não podiam ver nenhum dos dois defeitos: só criavam índice com `USING theodb_hnsw` + `cosine_ops`.
  - **script de upgrade** `theodb_rs--1.4.0--1.5.0.sql` gerado por `scripts/gen-upgrade-script.py` (94 KB; 129 `CREATE FUNCTION` → `OR REPLACE`, 19 objetos guardados, 2 `DROP IF EXISTS`), `default_version` bumpado para 1.5.0. Gerado, não escrito à mão — o próprio script avisa que transcrever à mão produz erro silencioso.

> Registered 2026-08-11 by `/discover --mode bug` (slug: `diagnostico-cego-ao-shim`), como achado
> colateral da medição do `B-015`.

> **Erro meu durante a implementação, registrado porque a classe importa mais que o bug.** A primeira versão
> da checagem de opclass interpolou `{tbl}::regclass` **sem aspas** no SQL, e isso QUEBROU três testes que
> passavam (`explain_scan_shows_index_and_candidates`, `scan_stats_records_real_pages_read`,
> `scan_stats_instruments_the_resume_path`) com `ERROR: column "esc1" does not exist` — o nome da relação
> virou identificador de coluna.
>
> É **exatamente a forma** que o doc de `resolve_relation` neste mesmo arquivo documenta como lição do #172:
> *"patch the SHAPE (every interpolation in the builder), never the payload"*. Ali foram três eixos
> (`qvec`, `col`, `tbl`) e o texto avisa que corrigir dois e declarar vitória é pior que não corrigir. Eu
> adicionei um quarto ponto de interpolação e repeti a forma — o sintoma foi outro (erro de sintaxe, não
> injeção, porque o valor vem do catálogo), mas a origem é a mesma.
>
> Agravante de método: **anunciei o item "implementado por completo" tendo validado só com
> `cargo check --lib`**, que não compila `#[cfg(test)]`. Eu havia escrito isso nesta mesma sessão, ao
> justificar por que a compilação limpa não cobria um teste novo — e mesmo assim declarei conclusão sobre
> uma verificação que não alcançava o que eu tinha escrito.

## B-022 — Dois testes declaram FRAGMENTO em `#[pg_test(error = …)]`, e o pgrx compara a mensagem INTEIRA   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: suíte completa de 2026-08-11 (`434 passed; 6 failed`). `graph::csr_build_guards_u32_boundary` declara `error = "must fit in u32"` e o produto emite `theodb.graph_build: node ids must fit in u32 (max 4294967295)`; `vectorizer::process_delete_failure_does_not_mark_done` declara `error = "does not exist"` e o produto emite `column "emb" of relation "dst_bad" does not exist`. **A comparação do pgrx é igualdade exata** — lido no fonte da dependência, `pgrx-tests-0.19.0/src/framework.rs:174`: `if Some(received_error_message) == expected_error`.
status: triaged
status_nota: resolvido em 2026-08-11 — os dois passaram a declarar a mensagem INTEIRA. Efeito colateral desejado e registrado: o texto do erro vira contrato, e mudá-lo passa a quebrar o teste (para o do `vectorizer` a mensagem é do ENGINE, `analyze.c`, então o contrato é do PostgreSQL).
why_now: **o produto está CORRETO nos dois** — cada um emitiu exatamente o erro tipado que o teste existe para provar, e a asserção reprova mesmo assim. É a classe que o m188 chamou de classificação errada de teste, e ela custa duas vagas permanentes no baseline de falhas do CI, protegendo dívida em vez de produto. O conserto é declarar a mensagem inteira; o cuidado é que ela então vira contrato — mudar o texto do erro passa a quebrar o teste, que é o comportamento desejado para um erro tipado.
status: raw
dod:
  - os dois testes passam com a mensagem completa declarada, sem afrouxar para `should_panic` genérico
  - ~~verificado se há OUTROS `#[pg_test(error = …)]` declarando fragmento~~ **FEITO 2026-08-12, e a prova é a suíte verde.** São **30** asserções `#[pg_test(error = …)]` no repositório, e a suíte fechou **440/0**. Como o pgrx compara por IGUALDADE (`framework.rs:174`), um fragmento que não casasse reprovaria o teste — logo, as 30 mensagens casam exatamente. A categoria "fragmento ainda verde por coincidência" **não existe** sob comparação exata: ou casa inteiro, ou falha.
  - ~~baseline de falhas do CI baixado no mesmo commit~~ **FEITO: 6 → 0**

> Registered 2026-08-11 by `/discover --mode review` (slug: `pg-test-error-fragmento`), da investigação
> das 6 falhas remanescentes após o `B-015`.

## B-023 — Um teste de PERFORMANCE mora na suíte funcional, e ele reprovou com AVX 51% mais lento   [ ]

domain: hot-path
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: suíte completa de 2026-08-11 — `vec::cosine_simd_per_candidate_speedup` falha com `SIMD cosine must not be slower than scalar (avx=15.75800491 scalar=10.426654568)` (`vec.rs:553`). A execução ocorreu numa máquina com outros dois contêineres ativos e uma suíte de 440 testes concorrendo por CPU.
why_now: as duas leituras possíveis têm consequências opostas e **a medição atual não as separa**. Se for ruído de ambiente, o teste é flaky por construção — `rules/testing.md § 6` proíbe teste dependente de tempo sem isolamento, e um teste vermelho intermitente treina o time a ignorar vermelho. Se for real, o kernel SIMD do caminho crítico regrediu e está 51% mais lento que a versão escalar que ele existe para superar, o que é defeito de performance no pilar vetorial. Um teste de vazão dentro da suíte funcional não consegue emitir esse veredito: `papers/rigorous-perf-eval-georges-2007.pdf` exige isolamento e variância, e a suíte não oferece nenhum dos dois.
status: triaged
medido: 2026-08-11 — **NÃO reproduz isolado, e a falha era do meu AMBIENTE de medição, não do teste.** Rodado sozinho num host sem outros contêineres: `pg_cosine_simd_per_candidate_speedup ... ok` (467,33 s). A execução que reprovou tinha TRÊS contêineres e 440 testes disputando CPU — contenção que eu mesmo criei. **Não há regressão no kernel SIMD**, e não havia teste quebrado para consertar; havia uma medição feita em condição ruim.
fragilidade_confirmada: o teste continua frágil POR CONSTRUÇÃO, e isso não muda com o resultado acima: mede tempo de parede, **uma amostra de cada**, ordem fixa (AVX sempre primeiro), sem pinagem de núcleo — num host cuja CPU é **híbrida** (i7-1355U, P-cores + E-cores), onde as duas metades podem cair em tipos de núcleo diferentes e a diferença entre eles supera a tolerância de 20%. Ele passa quando a máquina está livre, que é a condição do runner do CI; quebra sob qualquer contenção.
dod:
  - ~~determinado por repetição em máquina isolada se `avx < scalar` reproduz~~ **FEITO: não reproduz — é contenção**
  - alternar as duas medições e comparar MEDIANAS de N repetições, em vez de uma amostra de cada, para cancelar drift de frequência e migração entre P-core/E-core
  - se ruído: o teste sai da suíte funcional para o harness de benchmark, com variância declarada — não é afrouxado no lugar
  - se real: causa capturada por profiling e regressão coberta por benchmark reproduzível em `wiki/benchmarks/`

> Registered 2026-08-11 by `/discover --mode review` (slug: `simd-speedup-na-suite-funcional`), da
> investigação das 6 falhas remanescentes após o `B-015`.

## B-024 — O autotune recomendou `ef_search` sobre contadores em ZERO, e ninguém mediu o alcance   [ ]

domain: vetorial
repo: theo-db
suggested_mode: review
source: discover-review
evidence: consequência direta do `B-015`, medida em 2026-08-11. O recomendador lê `pages_read` do coletor (`autotune.rs:219` — `let (pages_read, candidates) = (read_scan_pages(), read_scan_candidates())`), e para todo índice V1 exact-f32 esses contadores eram **0** desde o M118, porque o caminho *resume* nunca reportou. O `B-015` corrigiu a instrumentação; **não** investigou o que o recomendador fez enquanto ela esteve cega.
why_now: um recomendador que decide sobre zero não erra de forma aleatória — erra de forma **sistemática e silenciosa**, e o `theodb._index_scan_stats` guarda essas observações persistidas. Duas perguntas ficaram abertas no review e nenhuma tem resposta medida: (a) recomendações emitidas nesse período são recuperáveis do catálogo e estão erradas? (b) o `recommend_ef` chega a ser sensível a `pages_read`, ou o zero foi inócuo porque a bisseção decide por recall? A segunda pode inocentar tudo — e é exatamente por isso que precisa ser medida em vez de suposta em qualquer direção.
status: killed
kill_reason: **honest-negative — o autotune NUNCA decidiu sobre os zeros.** Medido em 2026-08-12 lendo o caminho de decisão: `recommend_ef` (`autotune.rs`) faz doubling + bisseção sobre `recall_at_ef(&tbl, col, samples, &gts, k, ef) >= target` — a decisão é por **recall medido contra ground-truth exato**, e `pages_read` **não aparece** no corpo da função. Ele era apenas persistido em `theodb._index_scan_stats` por `record_scan_stat`, como observação, não como insumo. O DoD previa este desfecho e ele se realizou.
**Isto corrige uma afirmação minha.** O review do B-015 e o corpo do PR #226 diziam que "o recomendador de `ef_search` consumiu esses zeros". Era especulação a partir da proximidade no código, não leitura do caminho de decisão. O impacto do B-015 era **menor** do que afirmei: atingiu o diagnóstico (`explain_scan`/`scan_stats`) e o histórico do catálogo, não a recomendação. Fica registrado em vez de corrigido em silêncio, porque a afirmação errada saiu num PR.
dod:
  - ~~determinado por leitura + medição se `recommend_ef` consome `pages_read`~~ **FEITO: não consome**
  - se consome: quantificado o erro sobre um índice real, comparando recomendação com contador cego vs instrumentado
  - se não consome: registrado como honest-negative, e o campo deixa de ser citado como insumo de decisão

> Registered 2026-08-11 by `/review` (slug: `autotune-sobre-zeros`), como followup HIGH obrigatório do
> verdict `READY_TO_MERGE_WITH_FOLLOWUPS` — `.claude/knowledge-base/reviews/b015-review-2026-08-11.md`.

## B-025 — A imagem `theodb-builder` não traz `cargo-clippy`, então o gate de lint não roda fora do CI   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: medido em 2026-08-11 ao tentar rodar o gate de lint localmente contra a imagem que o próprio CI constrói (`docker build --target theodb-rs-builder -t theodb-builder .`): `error: 'cargo-clippy' is not installed for the toolchain '1.97.0-x86_64-unknown-linux-gnu'`, exit 1 — **ferramenta ausente, não lint reprovado**. O `lint-rust.yml` roda no runner self-hosted `theodb-do`, que tem o componente instalado fora da imagem.
why_now: o `.clippy_args` existe declaradamente para que "CI e local leem o MESMO baseline, sem drift" (comentário no topo do arquivo), e o drift que ele previne é de *argumentos* — mas a **ferramenta** diverge, o que é pior: quem tenta rodar o gate pela imagem oficial recebe exit 1 e, se ler o código de saída sem ler a mensagem, conclui que o lint reprovou. É a forma de falso-negativo que o `code-quality-golden-rule` nomeia `auditor_unavailable_{tool}` e manda registrar em vez de fabricar saída limpa. Nesta sessão o contorno foi `rustup component add` dentro do contêiner, o que funciona e **não** é o conserto: cada quem paga o download de novo.
status: raw
dod:
  - `rustup component add clippy` entra no estágio `theodb-rs-builder` do Dockerfile, e a imagem roda o gate sem passo extra
  - ~~verificado se `rustfmt` tem o mesmo problema~~ **VERIFICADO 2026-08-11: SIM, e é pior.** `cargo fmt` na imagem devolve `error: 'cargo-fmt' is not installed for the toolchain '1.97.0'`. E o modo de falha é traiçoeiro: `cargo fmt -- --check | grep -c "^Diff in"` imprimiu **`0`** — não porque não havia diffs, mas porque o comando falhou e não produziu saída nenhuma. **Um falso "está tudo limpo" indistinguível do verdadeiro**, exatamente o que o golden rule chama de fabricar saída limpa. Ambos os componentes (`clippy` e `rustfmt`) precisam entrar na imagem.
  - o toolchain local do desenvolvedor **não** serve de substituto: medido, o `cargo` do host é 1.91.0 e o projeto pina 1.97.0 (`rustfmt 1.9.0-stable`), então formatar fora do contêiner produz um resultado que o CI recusa

> **RESOLVIDO 2026-08-12, e a causa era mais funda que o item registrava.** O `Dockerfile` usa
> `rustup ... --profile minimal`, que não traz clippy nem rustfmt. Adicionar `--component clippy,rustfmt`
> ao mesmo comando **não bastaria**: havia um DRIFT DE VERSÃO invisível — o `Dockerfile` pinava
> `RUST_VERSION=1.97.1` enquanto `theodb_rs/rust-toolchain.toml` declara `1.97.0`, e o
> `rust-toolchain.toml` VENCE dentro do crate. Medido no log do build: o rustup baixa 1.97.0 on-demand ao
> entrar em `theodb_rs/`. Ou seja, os componentes iriam para o 1.97.1 — que **nunca compilou nada** — e o
> gate continuaria quebrado, exatamente como estava.
>
> Corolário que ninguém tinha notado: **o repin do M142 para 1.97.1 nunca teve efeito.** Tudo que compila o
> crate sempre usou 1.97.0.
>
> Correção: `RUST_VERSION=1.97.0` (alinhado ao `rust-toolchain.toml`, uma versão num lugar só) +
> `--component clippy,rustfmt` na mesma invocação do rustup. Verificação: o build passou a baixar **5
> componentes** em vez de 3.
>
> **A necessidade do alinhamento foi PROVADA experimentalmente, não deduzida.** Uma imagem construída com
> `--component` mas ainda com as versões desalinhadas mostra os dois toolchains convivendo e o componente
> só num deles:
>
> ```
> toolchains: 1.97.0  +  1.97.1 (active, default)
> fora do crate   → clippy 0.1.97                                      OK
> DENTRO do crate → 'cargo-clippy' is not installed for '1.97.0'       FALHA
> ```
>
> Ou seja: a correção "óbvia" (só adicionar o componente) teria sido publicada como resolvida e o gate
> continuaria quebrado exatamente do mesmo jeito — com o agravante de agora parecer consertado.
  - o script local documentado no `.clippy_args` roda de ponta a ponta contra a imagem

> Registered 2026-08-11 by `/code-quality` (slug: `builder-sem-clippy`), ao executar o gate de lint do
> ciclo do `B-015`.

## B-026 — Resíduo do SymQG: função morta com null-deref latente, e o gate de clippy está vermelho por ela   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: bug
source: discover-review
evidence: gate de lint executado em 2026-08-11 (`cargo clippy --features pg18 --no-deps` com o baseline `.clippy_args`): **4 erros, exit 101**, de dois lints que **não estão no baseline** — `clippy::needless_ifs` em `src/am/options.rs:447` e `clippy::implicit_saturating_sub` em `src/am/df_executor.rs:49`. O primeiro é o defeito: `degree_bound_from_relation` (`options.rs:445`) faz `if rd_options.is_null() { }` com **corpo vazio** e desreferencia o ponteiro na linha seguinte. As duas funções irmãs do mesmo arquivo fazem o certo — `lists_from_relation` tem `return DEFAULT_LISTS;` e `sbq_bits_from_relation` tem `return 0;`. O `return` se perdeu só nesta. Introduzido em `34a49d1` (2026-07-17, `impl(E2 T4.1): theodb_symqg reloption`).
why_now: **o risco de crash hoje é ZERO e é importante dizer isso** — `grep` confirma que a função tem **zero callers**: ela é resíduo do `theodb_symqg`, aposentado e removido da distribuição no M176. Não é um null-deref alcançável; é dead code carregando um. Duas consequências mesmo assim: (a) o `code-quality-golden-rule` classifica dead code exportado sem caller como `FAIL_HARD`, e (b) o gate de lint do repositório está **vermelho**, o que significa que ele não protege mais nada — o próximo defeito de verdade entra sem ser barrado, porque o vermelho já é o estado normal.
status: triaged
parcialmente_resolvido: 2026-08-11 — as duas causas do gate vermelho foram corrigidas no mesmo ciclo, porque elas **bloqueavam o merge** do release e a parte segura do conserto não tinha o risco que motivou o adiamento. (a) `degree_bound_from_relation` **removida** (zero callers, verificado por grep antes de apagar); (b) `df_executor.rs:49` passou a usar `saturating_sub` — e a checagem do tipo foi o que tornou isso seguro: `InterruptHoldoffCount` é `volatile uint32` (`miscadmin.h:104`), então satura em 0; sobre um inteiro COM sinal a mesma sugestão do lint daria `-1` e mudaria o comportamento. **O que NÃO foi feito**, e é o que mantém o item aberto: o reloption `degree_bound` (`options.rs:97,286-288`) segue registrado, e removê-lo afeta índices já criados que o declarem.
dod:
  - ~~`degree_bound_from_relation` removida~~ **FEITO** (não "consertada" — preencher o `if` manteria código morto de um pilar aposentado)
  - ~~avaliado se o reloption `degree_bound` também sai, e o que isso faz com índices já criados~~ **FEITO 2026-08-12 — REMOVIDO, e o risco que motivou o adiamento NÃO EXISTIA.** A contagem revelou o que a leitura não: `degree_bound` aparecia no `relopt_parse_elt` mas **nunca** no `add_int_reloption` — todos os outros nove aparecem nos DOIS. Sem registro, o PostgreSQL rejeita a opção antes do parse. Medido no binário shipado: `CREATE INDEX ... WITH (degree_bound = 32)` → `ERROR: unrecognized parameter "degree_bound"`, enquanto `WITH (lists = 4)` → `CREATE INDEX`. Logo **nenhum índice existente pode tê-la gravada**, e remover não afeta dump/restore. O campo era o ÚLTIMO do struct, então sair não desloca offset de nenhum outro. Array de parse: 10 → 9.
  - `df_executor.rs:49` resolvido ou entrando no baseline com justificativa — o código lá está **correto** (guarda explícita `> 0`), é só idioma
  - gate de clippy volta a exit 0 contra a imagem oficial (depende de `B-025`)

> Registered 2026-08-11 by `/code-quality` (slug: `residuo-symqg-null-deref`), ao executar o gate de lint
> do ciclo do `B-015`. **Nenhum dos 4 erros está nos arquivos alterados por aquele ciclo.**

> **O padrão, medido ao corrigir: foram QUATRO resíduos da MESMA remoção, e o gate só os revelou um por
> vez.** Cada correção destravava o clippy até o erro seguinte — 4 erros → 2 → 1 → 0, em quatro rodadas.
> Todos vêm da aposentadoria do `theodb_symqg`/FastScan (M176):
>
> | # | Onde | O que sobrou | Custo real |
> |---|---|---|---|
> | 1 | `options.rs:445` | `degree_bound_from_relation` com `if is_null(){}` vazio + desreferência | **null-deref latente** (zero callers) |
> | 2 | `options.rs:70` | doc do `degree_bound`, órfão sobre `DEFAULT_SOAR_LAMBDA_MILLI` | gate vermelho |
> | 3 | `vec/ah.rs:116` | doc de um "sign LUT" inexistente, órfão sobre `nibble` | gate vermelho |
> | 4 | `df_executor.rs:49` | `if > 0 { -= 1 }` | nenhum — código correto, só idioma |
>
> **Dois dos quatro tinham a frase TRUNCADA no meio** (`int16-accumulator-safe,` / `(a multiple of`), que é
> a assinatura de um corte parcial — alguém apagou linhas de dentro de um bloco em vez do bloco inteiro.
> Isso sugere que o M176 foi desfeito por remoção manual, não por uma verificação do que ficava órfão.
>
> **A lição operacional:** um gate que reprova em cascata (um erro por vez) esconde a dimensão do
> problema. Se o gate estivesse verde antes do M176, o primeiro resíduo teria aparecido sozinho, na hora,
> em vez de quatro deles emergirem meses depois durante um release não relacionado.

## B-027 — Run cancelado deixa o contêiner `suite` órfão, e o run seguinte reprova sem rodar um teste   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: bug
source: discover-live-test
evidence: medido em 2026-08-11 no PR #226. O job `suite` falhou com `docker: Error response from daemon: Conflict. The container name "/suite" is already in use by container "d14937ebd80f…"` — **antes de executar um único teste**. Causa: `rust-suite.yml` usa `docker run --name suite` (nome fixo) e só faz `docker rm -f suite` **depois** do run; quando o run é cancelado — e cada push novo no mesmo PR cancela o anterior — o passo morre antes da limpeza e o contêiner sobrevive no runner self-hosted.
why_now: o sintoma **mente sobre a causa**. Sem `test result` no log, o gate seguinte faz `line=$(grep -E "^test result" suite.log | tail -1)` e o job aparece no PR como **"suíte reprovou"** — indistinguível de uma regressão real de testes. Passei um ciclo investigando uma falha de testes que não existia. Num repositório onde o gate é "falhas não podem aumentar", confundir lixo de infraestrutura com regressão é o caminho mais curto para alguém subir o baseline sem precisar.
status: raw
dod:
  - ~~`docker rm -f suite` roda ANTES do `docker run`~~ **FEITO 2026-08-11** (remédio)
  - ~~avaliado usar nome único por run~~ **FEITO 2026-08-12 — a classe foi ELIMINADA.** O nome passou a ser `suite-${{ github.run_id }}-${{ github.run_attempt }}`: dois runs nunca disputam o mesmo nome, mesmo que o anterior morra no meio, então não existe colisão a remediar. O `rm -f` fica como higiene do próprio attempt, e um `docker container prune --filter until=24h` varre os órfãos que runs cancelados antigos já deixaram no runner — sem isso eles ocupariam disco para sempre.
  - o gate distingue "a suíte não emitiu resultado" de "a suíte reprovou" na saída do PR, em vez de os dois lerem igual

> Registered 2026-08-11 by `/discover --mode live-test` (slug: `suite-container-orfao`), ao rodar o
> release do `B-015`. **Provocado pelos meus próprios pushes sucessivos** — mas qualquer cancelamento
> reproduz, e cancelamento é uso normal de PR.

Próximo id livre: **`B-028`**. Ids são monotônicos e nunca reusados.
